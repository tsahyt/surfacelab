use crate::{broker, lang};
use petgraph::graph;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::Arc;
use std::thread;

#[derive(Debug)]
struct Node {
    operator: lang::Operator,
    resource: lang::Resource,
    inputs: HashMap<String, lang::ImageType>,
    outputs: HashMap<String, lang::ImageType>,
}

impl Node {
    fn new(operator: lang::Operator, resource: lang::Resource) -> Self {
        Node {
            operator,
            resource,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
        }
    }

    /// A node can be considered a Mask if and only if it has exactly one output
    /// which produces a Value.
    fn is_mask(&self) -> bool {
        self.outputs.len() == 1
            && self
                .outputs
                .iter()
                .all(|(_, x)| *x == lang::ImageType::Value)
    }
}

type EdgeLabel = (String, String);
type NodeGraph = graph::Graph<Node, EdgeLabel, petgraph::Directed>;

struct NodeManager {
    node_graph: NodeGraph,
    node_indices: HashMap<lang::Resource, graph::NodeIndex>,
    outputs: HashSet<graph::NodeIndex>,
}

impl NodeManager {
    pub fn new() -> Self {
        let node_graph = graph::Graph::new();
        NodeManager {
            node_graph,
            node_indices: HashMap::new(),
            outputs: HashSet::new(),
        }
    }

    pub fn process_event(&mut self, event: Arc<lang::Lang>) -> Option<Vec<lang::Lang>> {
        use crate::lang::*;
        let mut response = vec![];

        match &*event {
            Lang::UserNodeEvent(event) => match event {
                UserNodeEvent::NewNode(op) => {
                    let resource = self.new_node(op);
                    response.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                        resource,
                        op.clone(),
                    )))
                }
                UserNodeEvent::RemoveNode(res) => match self.remove_node(res) {
                    Ok(removed_conns) => {
                        response = removed_conns
                            .iter()
                            .map(|c| {
                                Lang::GraphEvent(GraphEvent::DisconnectedSockets(
                                    c.0.clone(),
                                    c.1.clone(),
                                ))
                            })
                            .collect();
                        response.push(Lang::GraphEvent(GraphEvent::NodeRemoved(res.clone())));
                    }
                    Err(e) => log::error!("{}", e),
                },
                UserNodeEvent::ConnectSockets(from, to) => {
                    self.connect_sockets(from, to)
                        .unwrap_or_else(|e| log::error!("{}", e));
                    response.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                        from.clone(),
                        to.clone(),
                    )))
                }
                UserNodeEvent::DisconnectSockets(from, to) => self
                    .disconnect_sockets(from, to)
                    .unwrap_or_else(|e| log::error!("{}", e)),
                UserNodeEvent::ForceRecompute => self.recompute(),
            },
            Lang::UserEvent(UserEvent::Quit) => return None,
            Lang::GraphEvent(..) => {}
        }

        Some(response)
    }

    fn next_free_name(&self, base_name: &str) -> lang::Resource {
        let mut resource = lang::Resource::unregistered_node();

        for i in 1.. {
            let name =
                lang::Resource::try_from(format!("node:{}.{}", base_name, i).as_ref()).unwrap();

            if !self.node_indices.contains_key(&name) {
                resource = name;
                break;
            }
        }

        resource
    }

    /// Add a new node to the node graph, defined by the operator.
    fn new_node(&mut self, op: &lang::Operator) -> lang::Resource {
        let node_id = self.next_free_name(op.default_name());

        log::trace!(
            "Adding {:?} to node graph with identifier {:?}",
            op,
            node_id
        );
        let node = Node::new(op.clone(), node_id.clone());
        let idx = self.node_graph.add_node(node);
        self.node_indices.insert(node_id.clone(), idx);

        if op.is_output() {
            self.outputs.insert(idx);
        }

        node_id
    }

    /// Remove a node with the given Resource if it exists.
    ///
    /// **Errors** if the node does not exist.
    fn remove_node(
        &mut self,
        resource: &lang::Resource,
    ) -> Result<Vec<(lang::Resource, lang::Resource)>, String> {
        use petgraph::visit::EdgeRef;

        let node = self
            .node_by_uri(resource)
            .ok_or(format!("Node for URI {} not found!", resource))?;

        log::trace!(
            "Removing node with identifier {:?}, indexed {:?}",
            &resource,
            node
        );

        // FIXME: removal sometimes fails when it shouldn't
        debug_assert!(self.node_graph.node_weight(node).is_some());

        // Remove from output vector
        if self
            .node_graph
            .node_weight(node)
            .unwrap()
            .operator
            .is_output()
        {
            self.outputs.remove(&node);
        }

        // Get all connections
        let edges = {
            let incoming = self
                .node_graph
                .edges_directed(node, petgraph::Direction::Incoming);
            let outgoing = self
                .node_graph
                .edges_directed(node, petgraph::Direction::Outgoing);
            incoming.chain(outgoing)
        };
        let es: Vec<_> = edges
            .map(|x| {
                let source = &self.node_graph.node_weight(x.source()).unwrap().resource;
                let sink = &self.node_graph.node_weight(x.target()).unwrap().resource;
                let sockets = x.weight();
                (
                    source.extend_fragment(&sockets.0),
                    sink.extend_fragment(&sockets.1),
                )
            })
            .collect();

        // Remove node
        self.node_graph.remove_node(node);
        self.node_indices.remove(&resource);

        Ok(es)
    }

    /// Connect two sockets in the node graph.
    ///
    /// **Errors** and aborts if either of the two Resources does not exist!
    fn connect_sockets(
        &mut self,
        from: &lang::Resource,
        to: &lang::Resource,
    ) -> Result<(), String> {
        let from_path = self
            .node_by_uri(from)
            .ok_or(format!("Node for URI {} not found!", &from))?;
        let from_socket = from
            .fragment()
            .ok_or("Missing socket specification")?
            .to_string();
        let to_path = self
            .node_by_uri(to)
            .ok_or(format!("Node for URI {} not found!", &to))?;
        let to_socket = to
            .fragment()
            .ok_or("Missing socket specification")?
            .to_string();

        log::trace!(
            "Connecting {:?} with {:?} from socket {:?} to socket {:?}",
            from_path,
            to_path,
            from_socket,
            to_socket,
        );
        self.node_graph
            .add_edge(from_path, to_path, (from_socket, to_socket));

        Ok(())
    }

    /// Disconnect two sockets in the node graph. If the two nodes are not
    /// connected, the graph remains the same.
    ///
    /// **Errors** and aborts if either of the two URIs does not exist!
    fn disconnect_sockets(
        &mut self,
        from: &lang::Resource,
        to: &lang::Resource,
    ) -> Result<(), String> {
        use petgraph::visit::EdgeRef;

        let from_path = self
            .node_by_uri(&from)
            .ok_or(format!("Node for URI {} not found!", &from))?;
        let from_socket = from
            .fragment()
            .ok_or("Missing socket specification")?
            .to_string();
        let to_path = self
            .node_by_uri(&to)
            .ok_or(format!("Node for URI {} not found!", &to))?;
        let to_socket = to
            .fragment()
            .ok_or("Missing socket specification")?
            .to_string();

        log::trace!(
            "Disconnecting {:?} with {:?} from socket {:?} to socket {:?}",
            from_path,
            to_path,
            from_socket,
            to_socket,
        );

        let mut to_delete: Vec<graph::EdgeIndex> = vec![];

        // Accumulate edges to be deleted first. This should only be one.
        for e in self.node_graph.edges_connecting(from_path, to_path) {
            if e.weight().0 == from_socket && e.weight().1 == to_socket {
                to_delete.push(e.id());
            }
        }

        // Ensure no double edges existed
        debug_assert_eq!(to_delete.len(), 1);

        // Delete accumulated edges
        for id in to_delete {
            self.node_graph.remove_edge(id);
        }

        Ok(())
    }

    fn node_by_uri(&self, resource: &lang::Resource) -> Option<graph::NodeIndex> {
        self.node_indices.get(&resource.drop_fragment()).cloned()
    }

    // TODO: should be in its own scope
    fn recompute(&self) {
        use petgraph::visit::EdgeRef;

        log::debug!("Relinearizing Node Graph");

        enum Action {
            Traverse(Option<(EdgeLabel, graph::NodeIndex)>),
            Visit(Option<(EdgeLabel, graph::NodeIndex)>),
        };

        let mut stack: Vec<(graph::NodeIndex, Action)> = self
            .outputs
            .iter()
            .map(|x| (*x, Action::Traverse(None)))
            .collect();

        let mut traversal = Vec::new();

        while let Some((nx, mark)) = stack.pop() {
            match mark {
                Action::Traverse(l) => {
                    stack.push((nx, Action::Visit(l)));
                    for edge in self
                        .node_graph
                        .edges_directed(nx, petgraph::Direction::Incoming)
                    {
                        let label = edge.weight();
                        let sink = edge.target();
                        stack.push((
                            edge.source(),
                            Action::Traverse(Some((label.to_owned(), sink))),
                        ));
                    }
                }
                Action::Visit(l) => {
                    let node = &self.node_graph.node_weight(nx).unwrap().resource;
                    traversal.push((
                        node,
                        l.map(|(edge, idx)| {
                            (
                                edge.0,
                                edge.1,
                                &self.node_graph.node_weight(idx).unwrap().resource,
                            )
                        }),
                    ));
                }
            }
        }
    }
}

pub fn start_nodes_thread(broker: &mut broker::Broker<lang::Lang>) -> thread::JoinHandle<()> {
    log::info!("Starting Node Manager");
    let (sender, receiver) = broker.subscribe();

    thread::spawn(move || {
        let mut node_mgr = NodeManager::new();

        for event in receiver {
            match node_mgr.process_event(event) {
                None => break,
                Some(response) => {
                    for ev in response {
                        if let Err(e) = sender.send(ev) {
                            log::error!("Node Manager lost connection to application bus! {}", e);
                        }
                    }
                }
            }
        }

        log::info!("Node Manager terminating");
    })
}
