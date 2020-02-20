use crate::{bus, lang};
use petgraph::graph;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::thread;

#[derive(Debug)]
pub struct Node {
    pub operator: lang::Operator,
    pub inputs: HashMap<String, lang::ImageType>,
    pub outputs: HashMap<String, lang::ImageType>,
}

impl Node {
    pub fn new(operator: lang::Operator) -> Self {
        Node {
            operator,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
        }
    }

    /// A node can be considered a Mask if and only if it has exactly one output
    /// which produces a Value.
    pub fn is_mask(&self) -> bool {
        self.outputs.len() == 1
            && self
                .outputs
                .iter()
                .all(|(_, x)| *x == lang::ImageType::Value)
    }
}

type NodeGraph = graph::Graph<Node, (String, String), petgraph::Directed>;

struct NodeManager {
    node_graph: NodeGraph,
    node_indices: HashMap<lang::Resource, graph::NodeIndex>,
}

impl NodeManager {
    pub fn new() -> Self {
        let node_graph = graph::Graph::new();
        NodeManager {
            node_graph,
            node_indices: HashMap::new(),
        }
    }

    pub fn process_event(&mut self, event: bus::Lang) -> Option<bus::Lang> {
        use crate::lang::*;
        let mut response = None;

        log::trace!("Node Manager processing event {:?}", event);
        match event {
            Lang::UserNodeEvent(event) => match event {
                UserNodeEvent::NewNode(op) => {
                    let resource = self.new_node(op.clone());
                    response = Some(Lang::GraphEvent(GraphEvent::NodeAdded(resource, op)))
                }
                UserNodeEvent::RemoveNode(uri) => self
                    .remove_node(uri)
                    .unwrap_or_else(|e| log::error!("{}", e)),
                UserNodeEvent::ConnectSockets(from, to) => self
                    .connect_sockets(from, to)
                    .unwrap_or_else(|e| log::error!("{}", e)),
                UserNodeEvent::DisconnectSockets(from, to) => self
                    .disconnect_sockets(from, to)
                    .unwrap_or_else(|e| log::error!("{}", e)),
            },
            Lang::GraphEvent(..) => {}
        }

        response
    }

    fn next_free_name(&self, base_name: String) -> lang::Resource {
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
    fn new_node(&mut self, op: lang::Operator) -> lang::Resource {
        let node_id = self.next_free_name(op.default_name());

        log::trace!(
            "Adding {:?} to node graph with identifier {:?}",
            op,
            node_id
        );
        let node = Node::new(op);
        let idx = self.node_graph.add_node(node);
        self.node_indices.insert(node_id.clone(), idx);

        node_id
    }

    /// Remove a node with the given URI if it exists.
    ///
    /// **Errors** if the node does not exist.
    fn remove_node(&mut self, resource: lang::Resource) -> Result<(), String> {
        let node = self
            .node_by_uri(&resource)
            .ok_or(format!("Node for URI {} not found!", resource))?;

        log::trace!(
            "Removing node with identifier {:?}, indexed {:?}",
            &resource,
            node
        );
        self.node_graph.remove_node(node);
        self.node_indices.remove(&resource);

        Ok(())
    }

    /// Connect two sockets in the node graph.
    ///
    /// **Errors** and aborts if either of the two URIs does not exist!
    fn connect_sockets(&mut self, from: lang::Resource, to: lang::Resource) -> Result<(), String> {
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
        from: lang::Resource,
        to: lang::Resource,
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
        self.node_indices.get(&resource).map(|i| i.clone())
    }
}

pub fn start_nodes_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    log::info!("Starting Node Manager");
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        let mut node_mgr = NodeManager::new();

        for event in receiver {
            if let Some(response) = node_mgr.process_event(event) {
                bus::emit(&sender, response);
            }
        }

        log::info!("Node Manager terminating");
    })
}
