use crate::{
    broker,
    lang::{self, Socketed},
};
use petgraph::graph;
use serde_derive::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fs::File;
use std::iter::FromIterator;
use std::path::Path;
use std::sync::Arc;
use std::thread;

pub mod io;

#[derive(Debug, Clone, Serialize, Deserialize)]
enum NodeOperator {
    Atomic(lang::Operator),
    Complex
}

impl NodeOperator {
    pub fn atomic(&self) -> Option<&lang::Operator> {
        match self {
            Self::Atomic(op) => Some(op),
            _ => None
        }
    }
}

impl lang::Socketed for NodeOperator {
    fn inputs(&self) -> HashMap<String, lang::OperatorType> {
        match self {
            Self::Atomic(op) => op.inputs(),
            _ => HashMap::new(),
        }
    }

    fn outputs(&self) -> HashMap<String, lang::OperatorType> {
        match self {
            Self::Atomic(op) => op.outputs(),
            _ => HashMap::new(),
        }
    }

    fn default_name<'a>(&'a self) -> &'static str {
        match self {
            Self::Atomic(op) => op.default_name(),
            _ => "unknown",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::Atomic(op) => op.title(),
            _ => "Unknown",
        }
    }
}

impl lang::Parameters for NodeOperator {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
        match self {
            Self::Atomic(op) => op.set_parameter(field, data),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    operator: NodeOperator,
    resource: lang::Resource,
    position: (i32, i32),
    absolute_size: bool,
    size: i32,
    type_variables: HashMap<lang::TypeVariable, lang::ImageType>,
}

type Connections = Vec<(lang::Resource, lang::Resource)>;

impl Node {
    fn new(operator: lang::Operator, resource: lang::Resource) -> Self {
        Node {
            operator: NodeOperator::Atomic(operator),
            resource,
            position: (0, 0),
            size: 0,
            absolute_size: false,
            type_variables: HashMap::new(),
        }
    }

    fn monomorphic_type(&self, socket: &str) -> Result<lang::OperatorType, String> {
        let ty = self
            .operator
            .inputs()
            .get(socket)
            .cloned()
            .or_else(|| self.operator.outputs().get(socket).cloned())
            .ok_or("Missing socket type")?;
        if let lang::OperatorType::Polymorphic(p) = ty {
            match self.type_variables.get(&p) {
                Some(x) => Ok(lang::OperatorType::Monomorphic(*x)),
                _ => Ok(ty),
            }
        } else {
            Ok(ty)
        }
    }

    pub fn node_size(&self, parent: i32) -> u32 {
        if self.absolute_size {
            if self.size > 0 {
                2 << self.size as i16
            } else {
                2 >> -self.size as i16
            }
        } else {
            if self.size > 0 {
                parent << self.size as i16
            } else {
                parent >> -self.size as i16
            }
        }
        .clamp(32, 16384) as u32
    }
}

type EdgeLabel = (String, String);
type NodeGraph = graph::Graph<Node, EdgeLabel, petgraph::Directed>;

struct NodeManager {
    parent_size: i32,
    node_graph: NodeGraph,
    node_indices: HashMap<lang::Resource, graph::NodeIndex>,
    outputs: HashSet<graph::NodeIndex>,
}

// FIXME: Changing output socket type after connection has already been made does not propagate type changes into preceeding polymorphic nodes!
impl NodeManager {
    pub fn new() -> Self {
        let node_graph = graph::Graph::new();
        NodeManager {
            parent_size: 1024,
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
                    let (resource, size) = self.new_node(op);
                    response.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                        resource,
                        op.clone(),
                        None,
                        size as u32,
                    )))
                }
                UserNodeEvent::RemoveNode(res) => match self.remove_node(res) {
                    Ok((ty, removed_conns)) => {
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
                        if let Some(ty) = ty {
                            response
                                .push(Lang::GraphEvent(GraphEvent::OutputRemoved(res.clone(), ty)))
                        }
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
                UserNodeEvent::DisconnectSinkSocket(sink) => {
                    match self.disconnect_sink_socket(sink) {
                        Ok(mut r) => response.append(&mut r),
                        Err(e) => log::error!("Error while disconnecting sink {}", e),
                    }
                }
                UserNodeEvent::ParameterChange(res, field, data) => {
                    self.parameter_change(res, field, data)
                        .unwrap_or_else(|e| log::error!("{}", e));
                    let instructions = self.recompute();
                    response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
                }
                UserNodeEvent::ForceRecompute => {
                    let instructions = self.recompute();
                    response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
                }
                UserNodeEvent::PositionNode(res, (x, y)) => self.position_node(res, *x, *y),
                UserNodeEvent::RenameNode(from, to) => {
                    if let Some(r) = self.rename_node(from, to) {
                        response.push(r);
                    }
                }
                UserNodeEvent::OutputSizeChange(res, size) => {
                    if let Some(r) = self.resize_node(res, Some(*size), None) {
                        response.push(r);
                    };
                }
                UserNodeEvent::OutputSizeAbsolute(res, abs) => {
                    if let Some(r) = self.resize_node(res, None, Some(*abs)) {
                        response.push(r);
                    };
                }
            },
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::UserIOEvent(UserIOEvent::RequestExport(None)) => {
                let exportable = self.get_output_sockets();
                response.push(Lang::UserIOEvent(UserIOEvent::RequestExport(Some(
                    exportable,
                ))));
            }
            Lang::UserIOEvent(UserIOEvent::OpenSurface(path)) => {
                match self.open_node_graph(path) {
                    Ok(mut evs) => {
                        response.push(Lang::GraphEvent(GraphEvent::Cleared));
                        response.append(&mut evs);

                        // Automatically recompute on load
                        let instructions = self.recompute();
                        response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
                    }
                    Err(e) => log::error!("{}", e),
                }
            }
            Lang::UserIOEvent(UserIOEvent::SaveSurface(path)) => {
                if let Err(e) = self.save_node_graph(path) {
                    log::error!("{}", e)
                }
            }
            Lang::UserIOEvent(UserIOEvent::NewSurface) => {
                self.reset();
                response.push(Lang::GraphEvent(GraphEvent::Cleared));
            }
            Lang::UserIOEvent(UserIOEvent::SetParentSize(size)) => {
                response.append(&mut self.resize_all(*size as i32));
                // Recompute on size change
                let instructions = self.recompute();
                response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
            }
            _ => {}
        }

        Some(response)
    }

    fn reset(&mut self) {
        self.outputs.clear();
        self.node_indices.clear();
        self.node_graph.clear();
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
    fn new_node(&mut self, op: &lang::Operator) -> (lang::Resource, u32) {
        let node_id = self.next_free_name(op.default_name());

        log::trace!(
            "Adding {:?} to node graph with identifier {:?}",
            op,
            node_id
        );
        let node = Node::new(op.clone(), node_id.clone());
        let size = node.node_size(self.parent_size);
        let idx = self.node_graph.add_node(node);
        self.node_indices.insert(node_id.clone(), idx);

        if op.is_output() {
            self.outputs.insert(idx);
        }

        (node_id, size)
    }

    /// Remove a node with the given Resource if it exists.
    ///
    /// **Errors** if the node does not exist.
    fn remove_node(
        &mut self,
        resource: &lang::Resource,
    ) -> Result<(Option<lang::OutputType>, Connections), String> {
        use petgraph::visit::EdgeRef;

        let node = self
            .node_by_uri(resource)
            .ok_or(format!("Node for URI {} not found!", resource))?;

        log::trace!(
            "Removing node with identifier {:?}, indexed {:?}",
            &resource,
            node
        );

        debug_assert!(self.node_graph.node_weight(node).is_some());

        // Remove from output vector
        let operator = &self.node_graph.node_weight(node).unwrap().operator;
        let mut output_type = None;
        if let NodeOperator::Atomic(lang::Operator::Output(lang::Output { output_type: ty })) = operator {
            self.outputs.remove(&node);
            output_type = Some(*ty)
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

        // Obtain last node before removal for reindexing
        let last = self
            .node_graph
            .node_weight(self.node_graph.node_indices().next_back().unwrap())
            .unwrap()
            .resource
            .clone();

        // Remove node
        self.node_graph.remove_node(node);
        self.node_indices.remove(&resource);

        // Reindex last node
        self.node_indices.insert(last, node);

        Ok((output_type, es))
    }

    /// Connect two sockets in the node graph. If there is already a connection
    /// on the sink, it will be replaced!
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

        // Check that from is a source and to is a sink
        if !(self
            .node_graph
            .node_weight(from_path)
            .unwrap()
            .operator
            .outputs()
            .contains_key(&from_socket)
            && self
                .node_graph
                .node_weight(to_path)
                .unwrap()
                .operator
                .inputs()
                .contains_key(&to_socket))
        {
            return Err("Tried to connect from a sink to a source".into());
        }

        // Handle type checking/inference
        let from_type = self.socket_type(from).unwrap();
        let to_type = self.socket_type(to).unwrap();
        match (from_type, to_type) {
            (lang::OperatorType::Polymorphic(..), lang::OperatorType::Polymorphic(..)) => {
                // TODO: polymorphism over multiple arcs
                return Err("Unable to connect polymorphic socket to polymorphic socket".into());
            }
            (lang::OperatorType::Monomorphic(ty1), lang::OperatorType::Monomorphic(ty2)) => {
                if ty1 != ty2 {
                    return Err("Type mismatch".into());
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

    /// Disconnect all (1) inputs from a sink socket.
    fn disconnect_sink_socket(&mut self, sink: &lang::Resource) -> Result<Vec<lang::Lang>, String> {
        use petgraph::visit::EdgeRef;

        let sink_path = self
            .node_by_uri(&sink)
            .ok_or(format!("Sink for URI {} not found", &sink))?;
        let sink_socket = sink
            .fragment()
            .ok_or("Missing sink socket specification")?
            .to_string();

        let mut resp = Vec::new();

        // Demonomorphize if nothing else keeps the type variable occupied
        let node = self.node_graph.node_weight(sink_path).unwrap();
        if let lang::OperatorType::Polymorphic(tvar) =
            node.operator.inputs().get(&sink_socket).unwrap()
        {
            let others: HashSet<String> =
                HashSet::from_iter(node.operator.sockets_by_type_variable(*tvar));

            if self
                .node_graph
                .edges_directed(sink_path, petgraph::EdgeDirection::Incoming)
                .filter(|e| others.contains(&e.weight().1))
                .chain(
                    self.node_graph
                        .edges_directed(sink_path, petgraph::EdgeDirection::Outgoing)
                        .filter(|e| others.contains(&e.weight().0)),
                )
                .next()
                .is_none()
            {
                self.set_type_variable(&sink.drop_fragment(), *tvar, None)
                    .unwrap();
                resp.push(lang::Lang::GraphEvent(
                    lang::GraphEvent::SocketDemonomorphized(sink.clone()),
                ));
            }
        }

        let source = self
            .node_graph
            .edges_directed(sink_path, petgraph::Direction::Incoming)
            .filter(|e| e.weight().1 == sink_socket)
            .map(|e| {
                (
                    self.node_graph
                        .node_weight(e.source())
                        .unwrap()
                        .resource
                        .extend_fragment(&e.weight().0),
                    e.id(),
                )
            })
            .next();

        if let Some(s) = &source {
            self.node_graph.remove_edge(s.1);
            resp.push(lang::Lang::GraphEvent(
                lang::GraphEvent::DisconnectedSockets(s.0.clone(), sink.clone()),
            ));
        }

        Ok(resp)
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
            .sockets_by_type_variable(variable)
            .iter()
            .map(|x| node.extend_fragment(x))
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
                    let op = node.operator.atomic().expect("Complex nodes are not yet supported").to_owned();
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

    /// Get all output sockets in the current node graph, as well as all
    /// *inputs* of Output nodes, i.e. everything that can be exported.
    fn get_output_sockets(&self) -> Vec<(lang::Resource, lang::ImageType)> {
        let mut result = Vec::new();

        for node_index in self.node_graph.node_indices() {
            let node = self.node_graph.node_weight(node_index).unwrap();

            if let NodeOperator::Atomic(lang::Operator::Output { .. }) = node.operator {
                for input in node.operator.inputs().iter() {
                    if let Ok(lang::OperatorType::Monomorphic(ty)) = node.monomorphic_type(&input.0)
                    {
                        result.push((node.resource.extend_fragment(&input.0), ty))
                    }
                }
            } else {
                for output in node.operator.outputs().iter() {
                    if let Ok(lang::OperatorType::Monomorphic(ty)) =
                        node.monomorphic_type(&output.0)
                    {
                        result.push((node.resource.extend_fragment(&output.0), ty))
                    }
                }
            }
        }

        result
    }

    /// Write the layout position of a node.
    fn position_node(&mut self, resource: &lang::Resource, x: i32, y: i32) {
        if let Some(node) = self.node_by_uri(resource) {
            let nw = self.node_graph.node_weight_mut(node).unwrap();
            nw.position = (x, y);
        }
    }

    fn rename_node(&mut self, from: &lang::Resource, to: &lang::Resource) -> Option<lang::Lang> {
        log::trace!("Renaming node {} to {}", from, to);
        if let Some(idx) = self.node_indices.remove(from) {
            let node = self.node_graph.node_weight_mut(idx).unwrap();
            node.resource = to.clone();
            self.node_indices.insert(to.clone(), idx);
            Some(lang::Lang::GraphEvent(lang::GraphEvent::NodeRenamed(
                from.clone(),
                to.clone(),
            )))
        } else {
            None
        }
    }

    fn resize_node(
        &mut self,
        res: &lang::Resource,
        size: Option<i32>,
        absolute: Option<bool>,
    ) -> Option<lang::Lang> {
        let idx = self.node_by_uri(res)?;
        let mut node = self.node_graph.node_weight_mut(idx).unwrap();

        if let Some(s) = size {
            node.size = s;
        }

        if let Some(a) = absolute {
            node.absolute_size = a;
        }

        let new_size = node.node_size(self.parent_size);

        Some(lang::Lang::GraphEvent(lang::GraphEvent::NodeResized(
            res.clone(),
            new_size,
        )))
    }

    fn resize_all(&mut self, parent: i32) -> Vec<lang::Lang> {
        log::debug!("Resizing all nodes");
        self.parent_size = parent;

        self.node_graph
            .node_indices()
            .filter_map(|idx| {
                self.node_graph.node_weight(idx).and_then(|x| {
                    if !x.absolute_size {
                        Some(lang::Lang::GraphEvent(lang::GraphEvent::NodeResized(
                            x.resource.clone(),
                            x.node_size(self.parent_size),
                        )))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

pub fn start_nodes_thread(broker: &mut broker::Broker<lang::Lang>) -> thread::JoinHandle<()> {
    log::info!("Starting Node Manager");
    let (sender, receiver, disconnector) = broker.subscribe();

    thread::Builder::new()
        .name("nodes".to_string())
        .spawn(move || {
            let mut node_mgr = NodeManager::new();

            for event in receiver {
                match node_mgr.process_event(event) {
                    None => break,
                    Some(response) => {
                        for ev in response {
                            if let Err(e) = sender.send(ev) {
                                log::error!(
                                    "Node Manager lost connection to application bus! {}",
                                    e
                                );
                            }
                        }
                    }
                }
            }

            log::info!("Node Manager terminating");
            disconnector.disconnect();
        })
        .expect("Failed to start nodes thread!")
}
