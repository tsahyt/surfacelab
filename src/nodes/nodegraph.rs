use crate::lang::*;

use bimap::BiHashMap;
use petgraph::graph;
use serde_derive::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::iter::FromIterator;

pub type Graph = graph::Graph<Node, EdgeLabel, petgraph::Directed>;

/// Edge labels in the node graph determine the sink/source sockets for this
/// connection in the multigraph.
type EdgeLabel = (String, String);

/// A vector of resource tuples describing connections between sockets.
type Connections = Vec<(Resource, Resource)>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComplexOperator {
    graph: Resource,
    instance: ResourcePart,
}

/// Enum to differentiate between atomic and complex operators. An atomic
/// operator is an Operator proper as defined in the language module and
/// understood by the compute components. A complex operator is a node graph in
/// and of itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum NodeOperator {
    Atomic(Operator),
    Complex(ComplexOperator),
}

impl NodeOperator {
    pub fn to_atomic(&self) -> Option<&Operator> {
        match self {
            Self::Atomic(op) => Some(op),
            _ => None,
        }
    }
}

impl Socketed for NodeOperator {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        match self {
            Self::Atomic(op) => op.inputs(),
            _ => HashMap::new(),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
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

impl Parameters for NodeOperator {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
        match self {
            Self::Atomic(op) => op.set_parameter(field, data),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    operator: NodeOperator,
    position: (i32, i32),
    absolute_size: bool,
    size: i32,
    type_variables: HashMap<TypeVariable, ImageType>,
}

impl Node {
    pub fn new(operator: Operator) -> Self {
        Node {
            operator: NodeOperator::Atomic(operator),
            position: (0, 0),
            size: 0,
            absolute_size: false,
            type_variables: HashMap::new(),
        }
    }

    pub fn monomorphic_type(&self, socket: &str) -> Result<OperatorType, String> {
        let ty = self
            .operator
            .inputs()
            .get(socket)
            .cloned()
            .or_else(|| self.operator.outputs().get(socket).cloned())
            .ok_or("Missing socket type")?;
        if let OperatorType::Polymorphic(p) = ty {
            match self.type_variables.get(&p) {
                Some(x) => Ok(OperatorType::Monomorphic(*x)),
                _ => Ok(ty),
            }
        } else {
            Ok(ty)
        }
    }

    pub fn node_size(&self, parent: u32) -> u32 {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeGraph {
    graph: graph::Graph<Node, EdgeLabel, petgraph::Directed>,
    indices: BiHashMap<Resource, graph::NodeIndex>,
    outputs: HashSet<graph::NodeIndex>,
}

impl NodeGraph {
    pub fn new() -> Self {
        NodeGraph {
            graph: graph::Graph::default(),
            indices: BiHashMap::new(),
            outputs: HashSet::new(),
        }
    }

    pub fn rebuild_events(&self, parent_size: u32) -> Vec<Lang> {
        let mut events = Vec::new();

        for idx in self.graph.node_indices() {
            let node = self.graph.node_weight(idx).unwrap();
            events.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                self.indices.get_by_right(&idx).unwrap().to_owned(),
                node.operator
                    .to_atomic()
                    .expect("Complex operators not yet supported in file IO")
                    .clone(),
                Some(node.position),
                node.node_size(parent_size) as u32,
            )));
        }

        for idx in self.graph.edge_indices() {
            let conn = self.graph.edge_weight(idx).unwrap();
            let (source_idx, sink_idx) = self.graph.edge_endpoints(idx).unwrap();
            events.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                self.indices
                    .get_by_right(&source_idx)
                    .unwrap()
                    .extend_fragment(&conn.0),
                self.indices
                    .get_by_right(&sink_idx)
                    .unwrap()
                    .extend_fragment(&conn.1),
            )));
        }

        // Create monomorphization events for all known type variables
        for idx in self.graph.node_indices() {
            let node = self.graph.node_weight(idx).unwrap();
            for tvar in node.type_variables.iter() {
                for res in node
                    .operator
                    .inputs()
                    .iter()
                    .chain(node.operator.outputs().iter())
                    .filter(|(_, t)| **t == OperatorType::Polymorphic(*tvar.0))
                    .map(|x| {
                        self.indices
                            .get_by_right(&idx)
                            .unwrap()
                            .extend_fragment(x.0)
                    })
                {
                    events.push(Lang::GraphEvent(GraphEvent::SocketMonomorphized(
                        res, *tvar.1,
                    )));
                }
            }
        }

        events
    }

    /// Reset, i.e. clear, the node graph entirely. This removes all nodes and
    /// connections.
    pub fn reset(&mut self) {
        self.outputs.clear();
        self.indices.clear();
        self.graph.clear();
    }

    fn next_free_name(&self, base_name: &str) -> Resource {
        let mut resource = Resource::unregistered_node();

        for i in 1.. {
            let name = Resource::try_from(format!("node:{}.{}", base_name, i).as_ref()).unwrap();

            if !self.indices.contains_left(&name) {
                resource = name;
                break;
            }
        }

        resource
    }

    /// Add a new node to the node graph, defined by the operator.
    pub fn new_node(&mut self, op: &Operator, parent_size: u32) -> (Resource, u32) {
        let node_id = self.next_free_name(op.default_name());

        log::trace!(
            "Adding {:?} to node graph with identifier {:?}",
            op,
            node_id
        );
        let node = Node::new(op.clone());
        let size = node.node_size(parent_size);
        let idx = self.graph.add_node(node);
        self.indices.insert(node_id.clone(), idx);

        if op.is_output() {
            self.outputs.insert(idx);
        }

        (node_id, size)
    }

    /// Remove a node with the given Resource if it exists.
    ///
    /// **Errors** if the node does not exist.
    pub fn remove_node(
        &mut self,
        resource: &Resource,
    ) -> Result<(Option<OutputType>, Connections), String> {
        use petgraph::visit::EdgeRef;

        let node = self
            .node_by_uri(resource)
            .ok_or(format!("Node for URI {} not found!", resource))?;

        log::trace!(
            "Removing node with identifier {:?}, indexed {:?}",
            &resource,
            node
        );

        debug_assert!(self.graph.node_weight(node).is_some());

        // Remove from output vector
        let operator = &self.graph.node_weight(node).unwrap().operator;
        let mut output_type = None;
        if let NodeOperator::Atomic(Operator::Output(Output { output_type: ty })) = operator {
            self.outputs.remove(&node);
            output_type = Some(*ty)
        }

        // Get all connections
        let edges = {
            let incoming = self
                .graph
                .edges_directed(node, petgraph::Direction::Incoming);
            let outgoing = self
                .graph
                .edges_directed(node, petgraph::Direction::Outgoing);
            incoming.chain(outgoing)
        };
        let es: Vec<_> = edges
            .map(|x| {
                let source = self.indices.get_by_right(&x.source()).unwrap();
                let sink = self.indices.get_by_right(&x.target()).unwrap();
                let sockets = x.weight();
                (
                    source.extend_fragment(&sockets.0),
                    sink.extend_fragment(&sockets.1),
                )
            })
            .collect();

        // Obtain last node before removal for reindexing
        let last = self
            .indices
            .get_by_right(&self.graph.node_indices().next_back().unwrap())
            .unwrap()
            .to_owned();

        // Remove node
        self.graph.remove_node(node);
        self.indices.remove_by_left(&resource);

        // Reindex last node
        self.indices.insert(last, node);

        Ok((output_type, es))
    }

    /// Change a parameter in a resource in this graph. Will return an error if
    /// the resource does not exist in this graph.
    pub fn parameter_change(
        &mut self,
        res: &Resource,
        field: &'static str,
        data: &[u8],
    ) -> Result<(), String> {
        let node = self
            .node_by_uri(res)
            .ok_or("Missing node for parameter change")?;
        let node_data = self.graph.node_weight_mut(node).unwrap();
        node_data.operator.set_parameter(field, data);

        log::trace!("Parameter changed to {:?}", node_data.operator);

        Ok(())
    }

    fn node_by_uri(&self, resource: &Resource) -> Option<graph::NodeIndex> {
        self.indices.get_by_left(&resource.drop_fragment()).cloned()
    }

    /// Connect two sockets in the node graph. If there is already a connection
    /// on the sink, it will be replaced!
    ///
    /// **Errors** and aborts if either of the two Resources does not exist!
    pub fn connect_sockets(&mut self, from: &Resource, to: &Resource) -> Result<Vec<Lang>, String> {
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
            .graph
            .node_weight(from_path)
            .unwrap()
            .operator
            .outputs()
            .contains_key(&from_socket)
            && self
                .graph
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
            (OperatorType::Polymorphic(..), OperatorType::Polymorphic(..)) => {
                // TODO: polymorphism over multiple arcs
                return Err("Unable to connect polymorphic socket to polymorphic socket".into());
            }
            (OperatorType::Monomorphic(ty1), OperatorType::Monomorphic(ty2)) => {
                if ty1 != ty2 {
                    return Err("Type mismatch".into());
                }
            }
            (OperatorType::Monomorphic(ty), OperatorType::Polymorphic(p)) => {
                let affected = self.set_type_variable(&to.drop_fragment(), p, Some(ty))?;
                for res in affected {
                    response.push(Lang::GraphEvent(GraphEvent::SocketMonomorphized(res, ty)))
                }
            }
            (OperatorType::Polymorphic(p), OperatorType::Monomorphic(ty)) => {
                let affected = self.set_type_variable(&from.drop_fragment(), p, Some(ty))?;
                for res in affected {
                    response.push(Lang::GraphEvent(GraphEvent::SocketMonomorphized(res, ty)))
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
        self.graph
            .add_edge(from_path, to_path, (from_socket, to_socket));

        Ok(response)
    }

    /// Disconnect all (1) inputs from a sink socket.
    pub fn disconnect_sink_socket(&mut self, sink: &Resource) -> Result<Vec<Lang>, String> {
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
        let node = self.graph.node_weight(sink_path).unwrap();
        if let OperatorType::Polymorphic(tvar) = node.operator.inputs().get(&sink_socket).unwrap() {
            let others: HashSet<String> =
                HashSet::from_iter(node.operator.sockets_by_type_variable(*tvar));

            if self
                .graph
                .edges_directed(sink_path, petgraph::EdgeDirection::Incoming)
                .filter(|e| others.contains(&e.weight().1))
                .chain(
                    self.graph
                        .edges_directed(sink_path, petgraph::EdgeDirection::Outgoing)
                        .filter(|e| others.contains(&e.weight().0)),
                )
                .next()
                .is_none()
            {
                self.set_type_variable(&sink.drop_fragment(), *tvar, None)
                    .unwrap();
                resp.push(Lang::GraphEvent(GraphEvent::SocketDemonomorphized(
                    sink.clone(),
                )));
            }
        }

        let source = self
            .graph
            .edges_directed(sink_path, petgraph::Direction::Incoming)
            .filter(|e| e.weight().1 == sink_socket)
            .map(|e| {
                (
                    self.indices
                        .get_by_right(&e.source())
                        .unwrap()
                        .extend_fragment(&e.weight().0),
                    e.id(),
                )
            })
            .next();

        if let Some(s) = &source {
            self.graph.remove_edge(s.1);
            resp.push(Lang::GraphEvent(GraphEvent::DisconnectedSockets(
                s.0.clone(),
                sink.clone(),
            )));
        }

        Ok(resp)
    }

    /// Assign a type variable with a concrete type or erase it.
    /// Returns a vector of all affected sockets.
    fn set_type_variable(
        &mut self,
        node: &Resource,
        variable: TypeVariable,
        ty: Option<ImageType>,
    ) -> Result<Vec<Resource>, String> {
        let path = self
            .node_by_uri(node)
            .ok_or(format!("Node for URI {} not found!", &node))?;

        let node_data = self
            .graph
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

    fn socket_type(&self, socket: &Resource) -> Result<OperatorType, String> {
        let path = self
            .node_by_uri(socket)
            .ok_or(format!("Node for URI {} not found!", &socket))?;
        let socket_name = socket
            .fragment()
            .ok_or("Missing socket specification")?
            .to_string();

        let node = self
            .graph
            .node_weight(path)
            .expect("Missing node during type lookup");
        node.monomorphic_type(&socket_name)
    }

    /// Write the layout position of a node.
    pub fn position_node(&mut self, resource: &Resource, x: i32, y: i32) {
        if let Some(node) = self.node_by_uri(resource) {
            let nw = self.graph.node_weight_mut(node).unwrap();
            nw.position = (x, y);
        }
    }

    /// Rename a node from a resource to a resource.
    ///
    /// This function does currently not check whether the resource is
    /// namespaced correctly for this graph!
    pub fn rename_node(&mut self, from: &Resource, to: &Resource) -> Option<Lang> {
        log::trace!("Renaming node {} to {}", from, to);
        if let Some((_, idx)) = self.indices.remove_by_left(from) {
            self.indices.insert(to.clone(), idx);
            Some(Lang::GraphEvent(GraphEvent::NodeRenamed(
                from.clone(),
                to.clone(),
            )))
        } else {
            None
        }
    }

    /// Resize a node given potential changes to absolute and size.
    pub fn resize_node(
        &mut self,
        res: &Resource,
        size: Option<i32>,
        absolute: Option<bool>,
        parent_size: u32,
    ) -> Option<Lang> {
        let idx = self.node_by_uri(res)?;
        let mut node = self.graph.node_weight_mut(idx).unwrap();

        if let Some(s) = size {
            node.size = s;
        }

        if let Some(a) = absolute {
            node.absolute_size = a;
        }

        let new_size = node.node_size(parent_size);

        Some(Lang::GraphEvent(GraphEvent::NodeResized(
            res.clone(),
            new_size,
        )))
    }

    pub fn resize_all(&mut self, parent_size: u32) -> Vec<Lang> {
        self.graph
            .node_indices()
            .filter_map(|idx| {
                self.graph.node_weight(idx).and_then(|x| {
                    if !x.absolute_size {
                        Some(Lang::GraphEvent(GraphEvent::NodeResized(
                            self.indices.get_by_right(&idx).unwrap().to_owned(),
                            x.node_size(parent_size),
                        )))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Get all output sockets in the current node graph, as well as all
    /// *inputs* of Output nodes, i.e. everything that can be exported.
    pub fn get_output_sockets(&self) -> Vec<(Resource, ImageType)> {
        let mut result = Vec::new();

        for node_index in self.graph.node_indices() {
            let node = self.graph.node_weight(node_index).unwrap();
            let res = self.indices.get_by_right(&node_index).unwrap();

            if let NodeOperator::Atomic(Operator::Output { .. }) = node.operator {
                for input in node.operator.inputs().iter() {
                    if let Ok(OperatorType::Monomorphic(ty)) = node.monomorphic_type(&input.0) {
                        result.push((res.extend_fragment(&input.0), ty))
                    }
                }
            } else {
                for output in node.operator.outputs().iter() {
                    if let Ok(OperatorType::Monomorphic(ty)) = node.monomorphic_type(&output.0) {
                        result.push((res.extend_fragment(&output.0), ty))
                    }
                }
            }
        }

        result
    }

    pub fn linearize(&self) -> Vec<Instruction> {
        use petgraph::visit::EdgeRef;

        enum Action {
            Traverse(Option<(EdgeLabel, graph::NodeIndex)>),
            Visit(Option<(EdgeLabel, graph::NodeIndex)>),
        };

        let mut stack: Vec<(graph::NodeIndex, Action)> = self
            .outputs
            .iter()
            .filter(|x| {
                self.graph
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
                    for edge in self.graph.edges_directed(nx, petgraph::Direction::Incoming) {
                        let label = edge.weight();
                        let sink = edge.target();
                        stack.push((
                            edge.source(),
                            Action::Traverse(Some((label.to_owned(), sink))),
                        ));
                    }
                }
                Action::Visit(l) => {
                    let node = self.graph.node_weight(nx).unwrap();
                    let op = node
                        .operator
                        .to_atomic()
                        .expect("Complex nodes are not yet supported")
                        .to_owned();
                    let res = self.indices.get_by_right(&nx).unwrap().to_owned();
                    traversal.push(Instruction::Execute(res.clone(), op));
                    if let Some(((source, sink), idx)) = l {
                        let to_node = self.indices.get_by_right(&idx).unwrap().to_owned();
                        let from = res.extend_fragment(&source);
                        let to = to_node.extend_fragment(&sink);
                        traversal.push(Instruction::Move(from, to));
                    }
                }
            }
        }

        traversal
    }
}
