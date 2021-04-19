/// Node graphs. A node graph knows two major types of resources, nodes and
/// sockets. Each node has some number of sockets, partitioned into input and
/// output sockets.
///
/// Internally we reference by `&str` here, instead of using the resource
/// abstraction. This is slightly faster and the full resource is unnecessary,
/// since we already know the graph part of the resource.
use super::{ExposedParameters, LinearizationMode, NodeCollection};
use crate::lang::resource as r;
use crate::lang::*;
use thiserror::Error;

use bimap::BiHashMap;
use petgraph::graph;
use serde_derive::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;

/// Specialized graph type used in this module.
pub type Graph = graph::Graph<Node, EdgeLabel, petgraph::Directed>;

/// Edge labels in the node graph determine the sink/source sockets for this
/// connection in the multigraph.
type EdgeLabel = (String, String);

/// A connection is a tuple of sockets.
pub type Connection = (Resource<r::Socket>, Resource<r::Socket>);

/// A vector of resource tuples describing connections between sockets.
pub type Connections = Vec<Connection>;

/// A single node in the graph. Nodes each have exactly one operator that they
/// correspond to. They are connected in the graph with edges that denote the
/// sockets that are being connected.
///
/// Each node also has a set of type variables to support polymorphism.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Node operator
    pub operator: Operator,
    /// Node position, stored here such that it can be retrieved from a file
    position: (f64, f64),
    /// Operator size of this node, possibly overridden by size request.
    size: OperatorSize,
    /// Type variables of this node, with their assignments
    type_variables: HashMap<TypeVariable, ImageType>,
}

impl Node {
    /// Create a new node from an operator.
    pub fn new(operator: Operator) -> Self {
        Node {
            position: (0.0, 0.0),
            size: OperatorSize::RelativeToParent(0),
            type_variables: HashMap::new(),
            operator,
        }
    }

    /// Obtain the absolute size of a node, dependent on parent size and size
    /// settings of the node.
    pub fn node_size(&self, parent: u32) -> u32 {
        if let Some(request) = self.operator.size_request() {
            return request;
        }

        self.size.absolute(parent)
    }

    pub fn absolutely_sized(&self) -> bool {
        matches!(self.size, OperatorSize::AbsoluteSize(..))
    }
}

#[derive(Error, Debug)]
pub enum SocketTypeError {
    #[error("Type Mismatch")]
    Mismatch,
    #[error("Tried connecting polymorphic socket to polymorphic socket")]
    PolyPolyConnection,
}

#[derive(Error, Debug)]
pub enum NodeGraphError {
    #[error("Socket type error")]
    ConnectionTypeError(#[from] SocketTypeError),
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Socket not found")]
    SocketNotFound(String),
    #[error("Invalid connection")]
    InvalidConnection,
    #[error("Monomorphization Error")]
    MonomorphizationError(#[from] MonomorphizationError),
}

/// Container type for a node graph. Contains the actual graph, as well as
/// metadata, and index structures for faster access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeGraph {
    graph: graph::Graph<Node, EdgeLabel, petgraph::Directed>,
    name: String,
    indices: BiHashMap<String, graph::NodeIndex>,
    outputs: HashSet<graph::NodeIndex>,
    parameters: HashMap<String, GraphParameter>,
}

impl NodeGraph {
    /// Create a new empty node graph
    pub fn new(name: &str) -> Self {
        NodeGraph {
            graph: graph::Graph::default(),
            name: name.to_string(),
            indices: BiHashMap::new(),
            outputs: HashSet::new(),
            parameters: HashMap::new(),
        }
    }

    /// Obtain the resource corresponding to a node by graph index
    fn node_resource(&self, idx: &petgraph::graph::NodeIndex) -> Resource<r::Node> {
        Resource::node(
            [&self.name, self.indices.get_by_right(idx).unwrap()]
                .iter()
                .collect::<std::path::PathBuf>(),
        )
    }

    /// Obtain an iterator over all node resources in the graph
    pub fn nodes(&self) -> impl Iterator<Item = Resource<r::Node>> + '_ {
        self.graph
            .node_indices()
            .map(move |idx| self.node_resource(&idx))
    }

    /// Obtain an iterator over all connections in the graph
    pub fn connections(&self) -> impl Iterator<Item = Connection> + '_ {
        self.graph.edge_indices().map(move |idx| {
            let (source_idx, sink_idx) = self.graph.edge_endpoints(idx).unwrap();
            let (source_socket, sink_socket) = self.graph.edge_weight(idx).unwrap();
            (
                self.node_resource(&source_idx).node_socket(source_socket),
                self.node_resource(&sink_idx).node_socket(sink_socket),
            )
        })
    }

    /// Reset, i.e. clear, the node graph entirely. This removes all nodes and
    /// connections.
    pub fn reset(&mut self) {
        self.outputs.clear();
        self.indices.clear();
        self.graph.clear();
    }

    /// Obtain a free resource name given a base name
    ///
    /// This will try `base.1`, `base.2`, etc, until it succeeds in finding a
    /// free name.
    fn next_free_name(&self, base_name: &str) -> String {
        let mut resource = String::new();

        for i in 1.. {
            let name = format!("{}.{}", base_name, i);

            if !self.indices.contains_left(&name) {
                resource = name;
                break;
            }
        }

        resource
    }

    /// Add a new node to the node graph, defined by the operator.
    pub fn new_node(&mut self, op: &Operator, parent_size: u32) -> (String, u32) {
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

        if op.as_atomic().map(|x| x.is_output()).unwrap_or(false) {
            self.outputs.insert(idx);
        }

        (node_id, size)
    }

    /// Remove a node with the given Resource if it exists. Returns the node, a
    /// list of connections that have been removed, as well as a boolean
    /// determining whether the complex operators associated with this graph
    /// require updating.
    ///
    /// **Errors** if the node does not exist.
    pub fn remove_node(
        &mut self,
        resource: &str,
    ) -> Result<(Node, Connections, bool), NodeGraphError> {
        use petgraph::visit::EdgeRef;

        let mut co_change = false;
        let node = *self
            .indices
            .get_by_left(&resource.to_string())
            .ok_or_else(|| NodeGraphError::NodeNotFound(resource.to_string()))?;

        log::trace!(
            "Removing node with identifier {:?}, indexed {:?}",
            &resource,
            node
        );

        debug_assert!(self.graph.node_weight(node).is_some());

        // Remove from output vector
        let operator = &self.graph.node_weight(node).unwrap().operator;
        match operator {
            Operator::AtomicOperator(AtomicOperator::Output(Output { .. })) => {
                self.outputs.remove(&node);
                co_change = true;
            }
            Operator::AtomicOperator(AtomicOperator::Input(..)) => {
                co_change = true;
            }
            _ => {}
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
                let source = self.node_resource(&x.source());
                let sink = self.node_resource(&x.target());
                let sockets = x.weight();
                (source.node_socket(&sockets.0), sink.node_socket(&sockets.1))
            })
            .collect();

        // Obtain last node before removal for reindexing
        let last_idx = self.graph.node_indices().next_back().unwrap();
        let last = self.indices.get_by_right(&last_idx).unwrap().to_owned();

        // Remove node
        let node_data = self.graph.remove_node(node).unwrap();
        self.indices.remove_by_left(&resource.to_string());

        // Reindex last node
        if last_idx != node {
            self.indices.insert(last, node);

            if self.outputs.remove(&last_idx) {
                self.outputs.insert(node);
            }
        }

        Ok((node_data, es, co_change))
    }

    /// Connect two sockets in the node graph. If there is already a connection
    /// on the sink, it will be replaced!
    ///
    /// **Errors** and aborts if either of the two Resources does not exist!
    pub fn connect_sockets(
        &mut self,
        source_node: &str,
        source_socket: &str,
        sink_node: &str,
        sink_socket: &str,
    ) -> Result<Vec<Lang>, NodeGraphError> {
        let mut response = Vec::new();
        // Get relevant resources
        let source_idx = *self
            .indices
            .get_by_left(&source_node.to_string())
            .ok_or_else(|| NodeGraphError::NodeNotFound(source_node.to_string()))?;
        let sink_idx = *self
            .indices
            .get_by_left(&sink_node.to_string())
            .ok_or_else(|| NodeGraphError::NodeNotFound(sink_node.to_string()))?;

        // Check that source and to are two different nodes
        if source_node == sink_node {
            return Err(NodeGraphError::InvalidConnection);
        }

        // Check that source is a source and sink is a sink
        if !(self
            .graph
            .node_weight(source_idx)
            .unwrap()
            .operator
            .outputs()
            .contains_key(source_socket)
            && self
                .graph
                .node_weight(sink_idx)
                .unwrap()
                .operator
                .inputs()
                .contains_key(sink_socket))
        {
            return Err(NodeGraphError::InvalidConnection);
        }

        // Disconnect sink
        response.append(&mut self.disconnect_sink_socket(sink_node, sink_socket)?);

        // Handle type checking/inference
        let source_type = self.socket_type(source_node, source_socket).unwrap();
        let sink_type = self.socket_type(sink_node, sink_socket).unwrap();
        match (source_type, sink_type) {
            (OperatorType::Polymorphic(..), OperatorType::Polymorphic(..)) => {
                return Err(NodeGraphError::ConnectionTypeError(
                    SocketTypeError::PolyPolyConnection,
                ));
            }
            (OperatorType::Monomorphic(ty1), OperatorType::Monomorphic(ty2)) => {
                if ty1 != ty2 {
                    return Err(NodeGraphError::ConnectionTypeError(
                        SocketTypeError::Mismatch,
                    ));
                }
            }
            (OperatorType::Monomorphic(ty), OperatorType::Polymorphic(p)) => {
                let affected = self.set_type_variable(&sink_node, p, Some(ty))?;
                for res in affected {
                    response.push(Lang::GraphEvent(GraphEvent::SocketMonomorphized(res, ty)))
                }
            }
            (OperatorType::Polymorphic(p), OperatorType::Monomorphic(ty)) => {
                let affected = self.set_type_variable(&source_node, p, Some(ty))?;
                for res in affected {
                    response.push(Lang::GraphEvent(GraphEvent::SocketMonomorphized(res, ty)))
                }
            }
        }

        // Perform connection
        log::trace!(
            "Connecting {:?} with {:?} from socket {:?} to socket {:?}",
            source_idx,
            sink_idx,
            source_socket,
            sink_socket,
        );
        self.graph.add_edge(
            source_idx,
            sink_idx,
            (source_socket.to_string(), sink_socket.to_string()),
        );

        Ok(response)
    }

    /// Disconnect all (1) inputs from a sink socket.
    pub fn disconnect_sink_socket(
        &mut self,
        sink_node: &str,
        sink_socket: &str,
    ) -> Result<Vec<Lang>, NodeGraphError> {
        use petgraph::visit::EdgeRef;

        let sink_path = *self
            .indices
            .get_by_left(&sink_node.to_string())
            .ok_or_else(|| NodeGraphError::NodeNotFound(sink_node.to_string()))?;
        let sink = self.node_resource(&sink_path).node_socket(sink_socket);

        let mut resp = Vec::new();

        let source = self
            .graph
            .edges_directed(sink_path, petgraph::Direction::Incoming)
            .filter(|e| e.weight().1 == sink_socket)
            .map(|e| {
                (
                    self.node_resource(&e.source()).node_socket(&e.weight().0),
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

        // Demonomorphize if nothing else keeps the type variable occupied
        let node = self.graph.node_weight(sink_path).unwrap();
        if let OperatorType::Polymorphic(tvar) = node.operator.inputs().get(sink_socket).unwrap() {
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
                self.set_type_variable(sink_node, *tvar, None).unwrap();
                resp.push(Lang::GraphEvent(GraphEvent::SocketDemonomorphized(sink)));
            }
        }

        Ok(resp)
    }

    /// Assign a type variable with a concrete type or erase it.
    /// Returns a vector of all affected sockets.
    fn set_type_variable(
        &mut self,
        node: &str,
        variable: TypeVariable,
        ty: Option<ImageType>,
    ) -> Result<Vec<Resource<r::Socket>>, NodeGraphError> {
        let path = *self
            .indices
            .get_by_left(&node.to_string())
            .ok_or_else(|| NodeGraphError::NodeNotFound(node.to_string()))?;
        let node_res = self.node_resource(&path);

        let node_data = self.graph.node_weight_mut(path).unwrap();

        match ty {
            Some(t) => node_data.type_variables.insert(variable, t),
            None => node_data.type_variables.remove(&variable),
        };

        let affected = node_data
            .operator
            .sockets_by_type_variable(variable)
            .iter()
            .map(|x| node_res.node_socket(x))
            .collect();

        Ok(affected)
    }

    /// Get the type for a socket.
    fn socket_type(
        &self,
        socket_node: &str,
        socket_name: &str,
    ) -> Result<OperatorType, NodeGraphError> {
        let path = self
            .indices
            .get_by_left(&socket_node.to_string())
            .ok_or_else(|| NodeGraphError::NodeNotFound(socket_node.to_string()))?;
        let node = self.graph.node_weight(*path).unwrap();
        match node
            .operator
            .monomorphic_type(&socket_name, &node.type_variables)
        {
            Ok(t) => Ok(OperatorType::Monomorphic(t)),
            Err(MonomorphizationError::PolymorphicSocket(v)) => Ok(OperatorType::Polymorphic(v)),
            Err(e) => Err(NodeGraphError::MonomorphizationError(e)),
        }
    }

    /// Update the layout position of a node.
    pub fn position_node(&mut self, name: &str, x: f64, y: f64) {
        if let Some(node) = self.indices.get_by_left(&name.to_string()) {
            let nw = self.graph.node_weight_mut(*node).unwrap();
            nw.position = (x, y);
        }
    }

    /// Rename a node from a given name to a new name.
    pub fn rename_node(&mut self, from: &str, to: &str) -> Option<Lang> {
        log::trace!("Renaming node {} to {}", from, to);
        if let Some((_, idx)) = self.indices.remove_by_left(&from.to_string()) {
            self.indices.insert(to.to_string(), idx);
            Some(Lang::GraphEvent(GraphEvent::NodeRenamed(
                Resource::node([&self.name, from].iter().collect::<std::path::PathBuf>()),
                Resource::node([&self.name, to].iter().collect::<std::path::PathBuf>()),
            )))
        } else {
            None
        }
    }

    /// Resize a node given potential changes to absolute and size.
    pub fn resize_node(
        &mut self,
        node: &str,
        size: OperatorSize,
        parent_size: u32,
    ) -> Option<Lang> {
        let idx = self.indices.get_by_left(&node.to_string())?;
        let mut node = self.graph.node_weight_mut(*idx).unwrap();

        node.size = size;

        let new_size = node.node_size(parent_size);
        let scalable = node.operator.scalable() && !node.absolutely_sized();

        Some(Lang::GraphEvent(GraphEvent::NodeResized(
            self.node_resource(idx),
            new_size,
            scalable,
        )))
    }

    /// Helper function to determine whether all inputs of a node have
    /// connections.
    fn all_node_inputs_connected(&self, idx: graph::NodeIndex) -> bool {
        self.graph.node_weight(idx).unwrap().operator.inputs().len()
            == self
                .graph
                .edges_directed(idx, petgraph::EdgeDirection::Incoming)
                .count()
    }

    /// Extract the nodes determined by the iterator and construct a new graph
    /// from them. Edges going into or out of the subgraph will be terminated
    /// with inputs and outputs in the new graph respectively. Finally, the
    /// extracted nodes will be replaced by a new complex operator. The new
    /// graph will have the given name.
    ///
    /// Along with the new graph, a vector of events is yielded, describing the
    /// transformation of *this* graph and construction of the *new* graph.
    ///
    /// The iterator is assumed to be nonempty!
    pub fn extract<'a, I>(
        &mut self,
        name: &str,
        parent_size: u32,
        nodes: I,
    ) -> Result<(Self, Vec<Lang>), NodeGraphError>
    where
        I: Iterator<Item = &'a str> + Clone,
    {
        let mut new = Self::new(name);
        let mut evs = vec![];
        let mut conns = vec![];

        let mut complex_pos = (0., 0.);
        let nodes_count = nodes.clone().count() as f64;

        // Move nodes to new graph and record positions for later
        for node in nodes {
            let (rnode, mut rconns, _) = self.remove_node(node)?;

            evs.extend(rconns.iter().map(|c| {
                Lang::GraphEvent(GraphEvent::DisconnectedSockets(c.0.clone(), c.1.clone()))
            }));
            evs.push(Lang::GraphEvent(GraphEvent::NodeRemoved(Resource::node(
                [&self.name, node].iter().collect::<std::path::PathBuf>(),
            ))));

            let (new_node, _) = new.new_node(&rnode.operator, parent_size);
            new.resize_node(&new_node, rnode.size, parent_size);
            new.position_node(&new_node, rnode.position.0, rnode.position.1);
            new.rename_node(&new_node, node);

            complex_pos.0 += rnode.position.0;
            complex_pos.1 += rnode.position.1;

            conns.append(&mut rconns);
        }

        complex_pos.0 /= nodes_count;
        complex_pos.1 /= nodes_count;

        // Rebuild all connections and log any required inputs and outputs
        let mut inputs = vec![];
        let mut outputs = vec![];

        for conn in conns {
            let source_node = conn.0.file().unwrap();
            let source_socket = conn.0.fragment().unwrap();
            let sink_node = conn.1.file().unwrap();
            let sink_socket = conn.1.fragment().unwrap();

            match new.connect_sockets(source_node, source_socket, sink_node, sink_socket) {
                Ok(_) => {}
                Err(NodeGraphError::NodeNotFound(missing)) => {
                    if missing == source_node {
                        let ty = new.socket_type(sink_node, sink_socket)?;
                        let (new_input, _) = new.new_node(
                            &Operator::AtomicOperator(AtomicOperator::Input(Input {
                                input_type: match ty {
                                    OperatorType::Monomorphic(t) => t,
                                    OperatorType::Polymorphic(_) => ImageType::Grayscale,
                                },
                            })),
                            parent_size,
                        );
                        new.connect_sockets(&new_input, "data", sink_node, sink_socket)?;
                        inputs.push((
                            new_input,
                            source_node.to_string(),
                            source_socket.to_string(),
                            ty,
                        ));
                    } else if missing == sink_node {
                        let ty = new.socket_type(source_node, source_socket)?;
                        let (new_output, _) = new.new_node(
                            &Operator::AtomicOperator(AtomicOperator::Output(Output {
                                output_type: OutputType::from(ty),
                            })),
                            parent_size,
                        );
                        new.connect_sockets(source_node, source_socket, &new_output, "data")?;
                        outputs.push((
                            new_output,
                            sink_node.to_string(),
                            sink_socket.to_string(),
                            ty,
                        ));
                    } else {
                        return Err(NodeGraphError::NodeNotFound(missing));
                    }
                }
                Err(e) => return Err(e),
            }
        }

        evs.append(&mut new.rebuild_events(parent_size));

        // Create Complex Operator in stead of the old nodes
        let complex_op = Operator::ComplexOperator({
            let mut co = ComplexOperator::new(Resource::graph(name));
            co.inputs = inputs
                .iter()
                .map(|(input_node, _, _, ty)| {
                    (
                        input_node.clone(),
                        (
                            OperatorType::from(*ty),
                            new.graph_resource().graph_node(input_node),
                        ),
                    )
                })
                .collect();
            co.outputs = outputs
                .iter()
                .map(|(output_node, _, _, ty)| {
                    (
                        output_node.clone(),
                        (*ty, new.graph_resource().graph_node(output_node)),
                    )
                })
                .collect();
            co
        });
        let (complex_node, _) = self.new_node(&complex_op, parent_size);
        let complex_res = self.graph_resource().graph_node(&complex_node);
        let complex_pbox = self.element_param_box(&complex_res).merge(
            new.param_box_description(complex_op.title().to_owned())
                .transmitters_into(),
        );
        self.position_node(&complex_node, complex_pos.0, complex_pos.1);

        evs.push(Lang::GraphEvent(GraphEvent::NodeAdded(
            complex_res.clone(),
            complex_op,
            complex_pbox,
            Some(complex_pos),
            parent_size,
        )));

        // Redo the input/output connections, create output sockets
        for (input, source_node, source_socket, _) in inputs {
            self.connect_sockets(&source_node, &source_socket, &complex_node, &input)?;
            evs.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                self.graph_resource()
                    .graph_node(&source_node)
                    .node_socket(&source_socket),
                complex_res.node_socket(&input),
            )));
        }

        for (output, sink_node, sink_socket, ty) in outputs {
            evs.push(Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                complex_res.node_socket(&output),
                OperatorType::from(ty),
                true,
                parent_size,
            )));
            self.connect_sockets(&complex_node, &output, &sink_node, &sink_socket)?;
            evs.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                complex_res.node_socket(&output),
                self.graph_resource()
                    .graph_node(&sink_node)
                    .node_socket(&sink_socket),
            )));
        }

        Ok((new, evs))
    }
}

impl ExposedParameters for NodeGraph {
    fn exposed_parameters(&self) -> &HashMap<String, GraphParameter> {
        &self.parameters
    }

    fn exposed_parameters_mut(&mut self) -> &mut HashMap<String, GraphParameter> {
        &mut self.parameters
    }
}

impl NodeCollection for NodeGraph {
    fn inputs(&self) -> HashMap<String, (OperatorType, Resource<r::Node>)> {
        self.graph
            .node_indices()
            .filter_map(|idx| {
                let node = self.graph.node_weight(idx).unwrap();
                let res = self.node_resource(&idx);
                match &node.operator {
                    Operator::AtomicOperator(AtomicOperator::Input(inp)) => Some((
                        res.file().unwrap().to_string(),
                        (*inp.outputs().get("data").unwrap(), res.clone()),
                    )),
                    _ => None,
                }
            })
            .collect()
    }

    fn outputs(&self) -> HashMap<String, (OperatorType, Resource<r::Node>)> {
        let mut result = HashMap::new();

        for idx in self.outputs.iter() {
            let res = self.node_resource(idx);
            let name = res.file().unwrap().to_string();
            let ty = *self
                .graph
                .node_weight(*idx)
                .unwrap()
                .operator
                .inputs()
                .get("data")
                .unwrap();
            result.insert(name, (ty, res));
        }

        result
    }

    fn output_type(&self, node: &Resource<r::Node>) -> Option<OutputType> {
        self.outputs
            .iter()
            .find(|idx| &self.node_resource(idx) == node)
            .and_then(|idx| match self.graph.node_weight(*idx).unwrap().operator {
                Operator::AtomicOperator(AtomicOperator::Output(Output { output_type })) => {
                    Some(output_type)
                }
                _ => None,
            })
    }

    fn graph_resource(&self) -> Resource<r::Graph> {
        Resource::graph(self.name.clone())
    }

    fn rename(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Linearize this node graph into a vector of instructions that can be
    /// interpreted by the compute backend.
    ///
    /// Takes a LinearizationMode as a parameter. The default mode should be
    /// TopoSort, which avoids revisiting nodes. However, a FullTraversal can be
    /// required on low end machines with little VRAM. In such cases it may
    /// becomes necessary to evict images temporarily if they are not needed
    /// immediately. In this case, they need to be recomputed later. The full
    /// traversal visits each node as many times as its output is used.
    ///
    /// In general using full traversals does not cause any immediate harm
    /// however, as the compute manager is expected to take care of any caching.
    /// TopoSort will however reduce the load on all components, including the
    /// linearization procedure itself.
    ///
    /// Linearization may fail when a node is missing inputs, and will return
    /// None in this case.
    fn linearize(&self, mode: LinearizationMode) -> Option<(Linearization, UsePoints)> {
        use petgraph::visit::EdgeRef;

        enum Action<'a> {
            /// Traverse deeper into the node graph, coming from the given label
            Traverse(Option<(&'a EdgeLabel, graph::NodeIndex)>),
            /// Execute the given node, emitting output, coming from this label
            Visit(Option<(&'a EdgeLabel, graph::NodeIndex)>),
            /// Indicates a use point of the given node
            Use(graph::NodeIndex),
        }

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

        let mut use_points: HashMap<Resource<r::Node>, UsePoint> = HashMap::new();
        let mut traversal = Vec::new();
        let mut step = 0;

        while let Some((nx, mark)) = stack.pop() {
            match mark {
                Action::Traverse(l) => {
                    if !self.all_node_inputs_connected(nx) {
                        return None;
                    }
                    stack.push((nx, Action::Visit(l)));
                    for edge in self.graph.edges_directed(nx, petgraph::Direction::Incoming) {
                        stack.push((edge.target(), Action::Use(edge.source())));
                    }
                    for edge in self.graph.edges_directed(nx, petgraph::Direction::Incoming) {
                        let label = edge.weight();
                        let sink = edge.target();
                        stack.push((edge.source(), Action::Traverse(Some((label, sink)))));
                    }
                }
                Action::Visit(l) => {
                    let node = self.graph.node_weight(nx).unwrap();
                    let res = self.node_resource(&nx);

                    if !use_points.contains_key(&res) || mode == LinearizationMode::FullTraversal {
                        match &node.operator {
                            Operator::AtomicOperator(op) => {
                                traversal.push(Instruction::Execute(res.clone(), op.to_owned()));
                            }
                            Operator::ComplexOperator(op) => {
                                for (socket, (_, input)) in op.inputs.iter() {
                                    traversal.push(Instruction::Copy(
                                        res.node_socket(socket),
                                        input.node_socket("data"),
                                    ))
                                }
                                traversal.push(Instruction::Call(res.clone(), op.to_owned()));
                                for (socket, (_, output)) in op.outputs.iter() {
                                    traversal.push(Instruction::Copy(
                                        output.node_socket("data"),
                                        res.node_socket(socket),
                                    ))
                                }
                            }
                        }

                        use_points
                            .entry(res.clone())
                            .and_modify(|e| e.creation = step)
                            .or_insert(UsePoint {
                                last: usize::MAX,
                                creation: step,
                            });

                        if let Some(thumbnail_output) = node.operator.outputs().keys().next() {
                            traversal
                                .push(Instruction::Thumbnail(res.node_socket(thumbnail_output)));
                        }
                    }

                    if let Some(((source, sink), idx)) = l {
                        let to_node = self.node_resource(&idx);
                        let from = res.node_socket(&source);
                        let to = to_node.node_socket(&sink);
                        traversal.push(Instruction::Move(from, to));
                    }

                    step += 1;
                }
                Action::Use(idx) => {
                    use_points
                        .entry(self.node_resource(&idx))
                        .and_modify(|e| e.last = step)
                        .or_insert(UsePoint {
                            last: step,
                            creation: usize::MIN,
                        });
                }
            }
        }

        Some((traversal, use_points.drain().collect()))
    }

    /// Change a parameter in a resource in this graph. Will return an error if
    /// the resource does not exist in this graph. May return a message as a
    /// side effect of changing the parameter.
    fn parameter_change(&mut self, resource: &Resource<Param>, data: &[u8]) -> Option<Lang> {
        let res = resource.file().unwrap();
        let field = resource.fragment().unwrap();

        let node = self.indices.get_by_left(&res.to_string())?;
        let node_data = self.graph.node_weight_mut(*node).unwrap();
        node_data.operator.set_parameter(field, data);

        log::trace!("Parameter changed to {:?}", node_data.operator);

        None
    }

    /// Update all the complex operators matching a call to the old graph.
    /// Returns a vector of all node resources that have been updated.
    fn update_complex_operators(
        &mut self,
        parent_size: u32,
        graph: &Resource<r::Graph>,
        new: &ComplexOperator,
    ) -> (Vec<super::ComplexOperatorUpdate>, Vec<GraphEvent>) {
        let mut updated = Vec::new();
        let mut evs = Vec::new();

        for idx in self.graph.node_indices() {
            let node_res = self.node_resource(&idx);
            let node = self.graph.node_weight_mut(idx).unwrap();
            let node_size = node.node_size(parent_size);

            if let Operator::ComplexOperator(complex) = &mut node.operator {
                if &complex.graph == graph {
                    complex.graph = new.graph.clone();
                    complex.title = new.title.clone();
                    complex.inputs = new.inputs.clone();

                    evs.extend(
                        new.outputs
                            .iter()
                            .filter(|(k, _)| !complex.outputs.contains_key(*k))
                            .map(|(socket, (ty, _))| {
                                GraphEvent::OutputSocketAdded(
                                    node_res.node_socket(socket),
                                    *ty,
                                    false,
                                    node_size,
                                )
                            }),
                    );

                    complex.outputs = new.outputs.clone();

                    for (field, subs) in &new.parameters {
                        if complex.parameters.get(field).is_none() {
                            complex.parameters.insert(field.clone(), subs.clone());
                        }
                    }

                    for (_, subs) in complex.parameters.iter_mut() {
                        subs.resource_mut().set_graph(new.graph.path())
                    }

                    let params = complex.parameters.clone();
                    updated.push((self.node_resource(&idx), params));
                }
            }
        }

        (updated, evs)
    }

    /// Resize all nodes according to a new parent size, creating appropriate
    /// resize events.
    fn resize_all(&mut self, parent_size: u32) -> Vec<Lang> {
        self.graph
            .node_indices()
            .filter_map(|idx| {
                self.graph.node_weight(idx).and_then(|x| {
                    Some(Lang::GraphEvent(GraphEvent::NodeResized(
                        self.node_resource(&idx),
                        x.node_size(parent_size),
                        x.operator.scalable() && !x.absolutely_sized(),
                    )))
                })
            })
            .collect()
    }

    fn rebuild_events(&self, parent_size: u32) -> Vec<Lang> {
        let mut events = Vec::new();

        for idx in self.graph.node_indices() {
            let node = self.graph.node_weight(idx).unwrap();
            events.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                self.node_resource(&idx),
                node.operator.clone(),
                ParamBoxDescription::empty(),
                Some(node.position),
                node.node_size(parent_size) as u32,
            )));

            for (socket, imgtype) in node.operator.outputs().iter() {
                events.push(Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                    self.node_resource(&idx).node_socket(socket),
                    *imgtype,
                    node.operator.external_data(),
                    node.node_size(parent_size) as u32,
                )));
            }
        }

        for idx in self.graph.edge_indices() {
            let conn = self.graph.edge_weight(idx).unwrap();
            let (source_idx, sink_idx) = self.graph.edge_endpoints(idx).unwrap();
            events.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                self.node_resource(&source_idx).node_socket(&conn.0),
                self.node_resource(&sink_idx).node_socket(&conn.1),
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
                    .map(|x| self.node_resource(&idx).node_socket(x.0))
                {
                    events.push(Lang::GraphEvent(GraphEvent::SocketMonomorphized(
                        res, *tvar.1,
                    )));
                }
            }
        }

        // Create parameter exposure events for all exposed parameters
        let graph = self.graph_resource();
        for param in self.parameters.values() {
            events.push(Lang::GraphEvent(GraphEvent::ParameterExposed(
                graph.clone(),
                param.clone(),
            )))
        }

        events
    }

    fn element_param_box(
        &self,
        element: &Resource<r::Node>,
    ) -> ParamBoxDescription<MessageWriters> {
        if let Some(idx) = self
            .indices
            .get_by_left(&element.file().unwrap().to_string())
            .copied()
        {
            let node = self.graph.node_weight(idx).expect("Corrupted node graph");
            ParamBoxDescription::node_parameters(
                element,
                if node.operator.scalable() {
                    Some(node.size)
                } else {
                    None
                },
            )
            .transmitters_into()
        } else {
            ParamBoxDescription::empty()
        }
    }
}
