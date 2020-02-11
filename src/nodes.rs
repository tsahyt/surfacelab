use crate::{bus, lang};
use petgraph::graph;
use std::collections::HashMap;
use std::thread;

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
    identifiers: HashMap<String, u32>,
    node_indices: HashMap<String, graph::NodeIndex>,
}

impl NodeManager {
    pub fn new() -> Self {
        let node_graph = graph::Graph::new();
        NodeManager {
            node_graph,
            identifiers: HashMap::new(),
            node_indices: HashMap::new(),
        }
    }

    pub fn process_event(&mut self, event: bus::Lang) {
        use crate::lang::*;

        log::trace!("Node Manager processing event {:?}", event);
        match event {
            Lang::UserNodeEvent(event) => match event {
                UserNodeEvent::NewNode(op) => {
                    self.new_node(op);
                }
                UserNodeEvent::RemoveNode(uri) => {
                    self.remove_node(uri);
                }
                UserNodeEvent::ConnectSockets(from, to) => {
                    self.connect_sockets(from, to)
                        .unwrap_or_else(|e| log::error!("{}", e));
                }
                UserNodeEvent::DisconnectSockets(from, to) => {
                    self.disconnect_sockets(from, to)
                        .unwrap_or_else(|e| log::error!("{}", e));
                }
            },
        }
    }

    fn new_node(&mut self, op: lang::Operator) {
        let node_id = {
            let stem = op.default_name();
            let num = self
                .identifiers
                .entry(stem.clone())
                .and_modify(|e| *e += 1)
                .or_insert(1);
            format!("{}.{}", stem, num)
        };
        log::trace!(
            "Adding {:?} to node graph with identifier {:?}",
            op,
            node_id
        );
        let node = Node::new(op);
        let idx = self.node_graph.add_node(node);
        self.node_indices.insert(node_id, idx);
    }

    fn remove_node(&mut self, uri: lang::URI) {
        let node = self.node_by_uri(&uri).unwrap();
        log::trace!(
            "Removing node with identifier {:?}, indexed {:?}",
            &uri,
            node
        );
        self.node_graph.remove_node(node);
    }

    /// Connect two sockets in the node graph.
    ///
    /// **Errors** and aborts if either of the two URIs does not exist!
    fn connect_sockets(&mut self, from: lang::URI, to: lang::URI) -> Result<(), String> {
        let from_path = self
            .node_by_uri(&from)
            .ok_or(format!("Node for URI {} not found!", &from))?;
        let from_socket = from
            .fragment()
            .ok_or("Missing socket specification")?
            .as_str()
            .to_string();
        let to_path = self
            .node_by_uri(&to)
            .ok_or(format!("Node for URI {} not found!", &to))?;
        let to_socket = to
            .fragment()
            .ok_or("Missing socket specification")?
            .as_str()
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
    fn disconnect_sockets(&mut self, from: lang::URI, to: lang::URI) -> Result<(), String> {
        use petgraph::visit::EdgeRef;

        let from_path = self
            .node_by_uri(&from)
            .ok_or(format!("Node for URI {} not found!", &from))?;
        let from_socket = from
            .fragment()
            .ok_or("Missing socket specification")?
            .as_str()
            .to_string();
        let to_path = self
            .node_by_uri(&to)
            .ok_or(format!("Node for URI {} not found!", &to))?;
        let to_socket = to
            .fragment()
            .ok_or("Missing socket specification")?
            .as_str()
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

    fn node_by_uri(&self, uri: &lang::URI) -> Option<graph::NodeIndex> {
        let path = format!("{}", uri.path());
        self.node_indices.get(&path).map(|i| i.clone())
    }
}

pub fn start_nodes_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    log::info!("Starting Node Manager");
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        let mut node_mgr = NodeManager::new();

        for event in receiver {
            node_mgr.process_event(event);
        }

        log::info!("Node Manager terminating");
    })
}
