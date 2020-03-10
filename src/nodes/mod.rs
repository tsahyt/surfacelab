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
    type_variables: HashMap<lang::TypeVariable, lang::ImageType>,
}

impl Node {
    fn new(operator: lang::Operator, resource: lang::Resource) -> Self {
        Node {
            operator,
            resource,
            type_variables: HashMap::new(),
        }
    }

    /// A node can be considered a Mask if and only if it has exactly one output
    /// which produces a Value.
    fn is_mask(&self) -> bool {
        self.operator.outputs().len() == 1
            && self.operator.outputs().iter().all(|(_, x)| match x {
                lang::OperatorType::Monomorphic(z) => *z == lang::ImageType::Grayscale,
                lang::OperatorType::Polymorphic(p) => self
                    .type_variables
                    .get(p)
                    .map_or(false, |z| *z == lang::ImageType::Grayscale),
            })
    }

    fn monomorphic_type(&self, socket: &str) -> Result<lang::OperatorType, String> {
        let ty = self
            .operator
            .inputs()
            .get(socket)
            .or(self.operator.outputs().get(socket))
            .cloned()
            .ok_or("Missing socket type".to_string())?;
        if let lang::OperatorType::Polymorphic(p) = ty {
            match self.type_variables.get(&p) {
                Some(x) => Ok(lang::OperatorType::Monomorphic(*x)),
                _ => Ok(ty),
            }
        } else {
            Ok(ty)
        }
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
                UserNodeEvent::ConnectSockets(from, to) => match self.connect_sockets(from, to) {
                    Ok(mut res) => {
                        response.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                            from.clone(),
                            to.clone(),
                        )));
                        response.append(&mut res);
                    }
                    Err(e) => log::error!("{}", e),
                },
                UserNodeEvent::DisconnectSockets(from, to) => self
                    .disconnect_sockets(from, to)
                    .unwrap_or_else(|e| log::error!("{}", e)),
                UserNodeEvent::ParameterChange(res, field, data) => self
                    .parameter_change(res, field, data)
                    .unwrap_or_else(|e| log::error!("{}", e)),
                UserNodeEvent::ForceRecompute => {
                    let instructions = self.recompute();
                    response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
                }
            },
            Lang::UserEvent(UserEvent::Quit) => return None,
            Lang::GraphEvent(..) => {}
        }

        Some(response)
    }

    fn parameter_change(
        &mut self,
        res: &lang::Resource,
        field: &'static str,
        data: &[u8],
    ) -> Result<(), String> {
        use lang::Parameters;

        let node = self
            .node_by_uri(res)
            .ok_or("Missing node for parameter change")?;
        let node_data = self.node_graph.node_weight_mut(node).unwrap();
        node_data.operator.set_parameter(field, data);

        log::trace!("Parameter changed to {:?}", node_data.operator);

        Ok(())
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
    ) -> Result<Vec<lang::Lang>, String> {
        let mut response = Vec::new();
        // Get relevant resources
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

        // Handle type checking/inference
        let from_type = self.socket_type(from).unwrap();
        let to_type = self.socket_type(to).unwrap();
        match (from_type, to_type) {
            (lang::OperatorType::Polymorphic(..), lang::OperatorType::Polymorphic(..)) => {
                Err("Unable to connect polymorphic socket to polymorphic socket")?
            }
            (lang::OperatorType::Monomorphic(ty1), lang::OperatorType::Monomorphic(ty2)) => {
                if ty1 != ty2 {
                    Err("Type mismatch")?
                }
            }
            (lang::OperatorType::Monomorphic(ty), lang::OperatorType::Polymorphic(p)) => {
                let affected = self.set_type_variable(&to.drop_fragment(), p, Some(ty))?;
                for res in affected {
                    response.push(lang::Lang::GraphEvent(
                        lang::GraphEvent::SocketMonomorphized(res, ty),
                    ))
                }
            }
            (lang::OperatorType::Polymorphic(p), lang::OperatorType::Monomorphic(ty)) => {
                let affected = self.set_type_variable(&from.drop_fragment(), p, Some(ty))?;
                for res in affected {
                    response.push(lang::Lang::GraphEvent(
                        lang::GraphEvent::SocketMonomorphized(res, ty),
                    ))
                }
            }
        }

        // Perform connection
        log::trace!(
            "Connecting {:?} with {:?} from socket {:?} to socket {:?}",
            from_path,
            to_path,
            from_socket,
            to_socket,
        );
        self.node_graph
            .add_edge(from_path, to_path, (from_socket, to_socket));

        Ok(response)
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

    fn socket_type(&self, socket: &lang::Resource) -> Result<lang::OperatorType, String> {
        let path = self
            .node_by_uri(socket)
            .ok_or(format!("Node for URI {} not found!", &socket))?;
        let socket_name = socket
            .fragment()
            .ok_or("Missing socket specification")?
            .to_string();

        let node = self
            .node_graph
            .node_weight(path)
            .expect("Missing node during type lookup");
        node.monomorphic_type(&socket_name)
    }

    /// Assign a type variable with a concrete type or erase it.
    /// Returns a vector of all affected sockets.
    fn set_type_variable(
        &mut self,
        node: &lang::Resource,
        variable: lang::TypeVariable,
        ty: Option<lang::ImageType>,
    ) -> Result<Vec<lang::Resource>, String> {
        let path = self
            .node_by_uri(node)
            .ok_or(format!("Node for URI {} not found!", &node))?;

        let node_data = self
            .node_graph
            .node_weight_mut(path)
            .expect("Missing node during type lookup");

        match ty {
            Some(t) => node_data.type_variables.insert(variable, t),
            None => node_data.type_variables.remove(&variable),
        };

        let affected = node_data
            .operator
            .inputs()
            .iter()
            .chain(node_data.operator.outputs().iter())
            .filter(|(_, t)| **t == lang::OperatorType::Polymorphic(variable))
            .map(|x| node.extend_fragment(x.0))
            .collect();

        Ok(affected)
    }

    fn recompute(&self) -> Vec<lang::Instruction> {
        use petgraph::visit::EdgeRef;

        log::debug!("Relinearizing Node Graph");

        enum Action {
            Traverse(Option<(EdgeLabel, graph::NodeIndex)>),
            Visit(Option<(EdgeLabel, graph::NodeIndex)>),
        };

        let mut stack: Vec<(graph::NodeIndex, Action)> = self
            .outputs
            .iter()
            .filter(|x| {
                self.node_graph
                    .neighbors_directed(**x, petgraph::Direction::Incoming)
                    .count()
                    != 0
            })
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
                    let node = self.node_graph.node_weight(nx).unwrap();
                    let op = node.operator.to_owned();
                    let res = node.resource.to_owned();
                    traversal.push(lang::Instruction::Execute(res.clone(), op));
                    if let Some(((source, sink), idx)) = l {
                        let to_node = self
                            .node_graph
                            .node_weight(idx)
                            .unwrap()
                            .resource
                            .to_owned();
                        let from = res.extend_fragment(&source);
                        let to = to_node.extend_fragment(&sink);
                        traversal.push(lang::Instruction::Move(from, to));
                    }
                }
            }
        }

        traversal
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
