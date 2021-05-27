use crate::{broker, lang, lang::OperatorParamBox, lang::*};

use serde_derive::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use enum_dispatch::*;
use maplit::hashmap;

pub mod io;
pub mod layers;
pub mod nodegraph;

/// Trait describing functionality relating to exposed parameters on node
/// graphs. These operations only mutate backend data, other parts of the system
/// need to be notified accordingly.
#[enum_dispatch]
trait ExposedParameters: NodeCollection {
    /// Get a mutable reference to the exposed parameters of the node graph.
    fn exposed_parameters_mut(&mut self) -> &mut HashMap<String, GraphParameter>;

    /// Get a reference to the exposed parameters of the node graph.
    fn exposed_parameters(&self) -> &HashMap<String, GraphParameter>;

    /// Expose a parameter
    fn expose_parameter(
        &mut self,
        parameter: Resource<Param>,
        graph_field: &str,
        title: &str,
        control: Control,
    ) -> Option<&GraphParameter> {
        self.exposed_parameters_mut().insert(
            graph_field.to_owned(),
            GraphParameter {
                graph_field: graph_field.to_owned(),
                parameter,
                title: title.to_string(),
                control,
            },
        );
        self.exposed_parameters().get(graph_field)
    }

    /// Conceal a parameter
    fn conceal_parameter(&mut self, graph_field: &str) -> Option<GraphParameter> {
        self.exposed_parameters_mut().remove(graph_field)
    }

    /// Retitle a parameter
    fn retitle_parameter(&mut self, graph_field: &str, new_title: &str) {
        if let Some(param) = self.exposed_parameters_mut().get_mut(graph_field) {
            param.title = new_title.to_owned();
        }
    }

    /// Refield a parameter, i.e. change the name of the field.
    fn refield_parameter(&mut self, graph_field: &str, new_field: &str) {
        if let Some(mut param) = self.exposed_parameters_mut().remove(graph_field) {
            param.graph_field = new_field.to_owned();
            self.exposed_parameters_mut()
                .insert(new_field.to_owned(), param);
        }
    }

    /// Obtain a ParamBoxDescription for the exposed parameters of this node graph
    fn param_box_description(&self, title: String) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: title.clone(),
            preset_tag: Some(title),
            categories: vec![ParamCategory {
                name: "Exposed Parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: self
                    .exposed_parameters()
                    .iter()
                    .map(|(k, v)| Parameter {
                        name: v.title.clone(),
                        transmitter: Field(k.clone()),
                        control: v.control.clone(),
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    })
                    .collect(),
            }],
        }
    }

    /// Construct the default map of parameter substitutions from this graph.
    /// This will include all parameters with their default values.
    fn default_substitutions(&self) -> HashMap<String, ParamSubstitution> {
        self.exposed_parameters()
            .values()
            .map(|v| (v.graph_field.clone(), v.to_substitution()))
            .collect()
    }

    /// Create a stub for a complex operator representing this node graph.
    fn complex_operator_stub(&self) -> ComplexOperator {
        let mut co = ComplexOperator::new(self.graph_resource());
        co.outputs = self.outputs();
        co.inputs = self.inputs();
        co.parameters = self.default_substitutions();
        co
    }
}

/// Modes for linearization. Applicable to node graphs rather than layer stacks,
/// since layer stacks are by nature already linear.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LinearizationMode {
    /// Produces a topological sort of the node graph. This means that nodes
    /// will only be visited once. Can place higher demands on memory during
    /// execution because more things have to be kept in cache.
    TopoSort,
    /// A full traversal produces a depth first ordering of the node graph. As a
    /// result, nodes can be visited multiple times. If still cached, the
    /// compute component is unlikely (but free) to recompute them.
    FullTraversal,
}

/// Information pertaining to the update of a complex operator in case the
/// underlying graph was changed.
pub type ComplexOperatorUpdate = (Resource<Node>, HashMap<String, ParamSubstitution>);

/// General functions of a node graph
#[enum_dispatch]
trait NodeCollection {
    /// Obtain the inputs, i.e. set of input nodes, in the node graph
    fn inputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)>;

    /// Obtain the outputs, i.e. set of output nodes, in the node graph
    fn outputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)>;

    /// Obtain the Output Type of a given node if it is an output in the collection
    fn output_type(&self, node: &Resource<Node>) -> Option<OutputType>;

    /// Obtain the graph resource for this collection
    fn graph_resource(&self) -> Resource<Graph>;

    /// Rename the collection
    fn rename(&mut self, name: &str);

    /// Linearize this node graph into a vector of instructions that can be
    /// interpreted by the compute backend.
    fn linearize(&self, mode: LinearizationMode) -> Option<(Linearization, UsePoints)>;

    /// Change a parameter in a resource in this node collection. May optionally
    /// return an event or fail silently.
    fn parameter_change(&mut self, resource: &Resource<Param>, data: &[u8]) -> Option<Lang>;

    /// Update all the complex operators matching a call to the old graph.
    /// Returns a vector of all node resources that have been updated.
    fn update_complex_operators(
        &mut self,
        parent_size: u32,
        graph: &Resource<Graph>,
        new: &ComplexOperator,
    ) -> (Vec<ComplexOperatorUpdate>, Vec<GraphEvent>);

    /// Resize all the nodes in the collection with the new parent size.
    fn resize_all(&mut self, parent_size: u32) -> Vec<Lang>;

    /// Rebuild all events that create this collection. Note that parameter boxes
    /// will be left empty, since not all information is available to build them
    /// in the case of complex operators.
    fn rebuild_events(&self, parent_size: u32) -> Vec<Lang>;

    /// Construct a parameter box description for elements in this node
    /// collection. E.g. nodes in the case of graphs or layers/masks in the case
    /// of layer stacks.
    fn element_param_box(&self, element: &Resource<Node>) -> ParamBoxDescription<MessageWriters>;
}

/// A node collection that can be stored and managed by the node manager.
#[enum_dispatch(ExposedParameters, NodeCollection)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManagedNodeCollection {
    NodeGraph(nodegraph::NodeGraph),
    LayerStack(layers::LayerStack),
}

impl Default for ManagedNodeCollection {
    /// The default collection is the empty graph, named base
    fn default() -> Self {
        Self::NodeGraph(nodegraph::NodeGraph::new("base"))
    }
}

/// The node manager is responsible for storing and modifying the node networks
/// in the current surface file.
struct NodeManager {
    parent_size: u32,
    export_specs: Vec<lang::ExportSpec>,
    graphs: HashMap<String, ManagedNodeCollection>,
    active_graph: lang::Resource<lang::Graph>,
}

impl NodeManager {
    /// Initialize the node manager with default settings.
    pub fn new() -> Self {
        NodeManager {
            parent_size: 1024,
            export_specs: Vec::new(),
            graphs: hashmap! { "base".to_string() => ManagedNodeCollection::default() },
            active_graph: lang::Resource::graph("base"),
        }
    }

    /// Create the parameter box for an operator. This needs to be done by the
    /// node manager in order to support complex operator, for which we need
    /// information about other graphs.
    pub fn operator_param_box(
        &self,
        operator: &lang::Operator,
    ) -> lang::ParamBoxDescription<lang::Field> {
        match operator {
            lang::Operator::AtomicOperator(ao) => ao.param_box_description(),
            lang::Operator::ComplexOperator(co) => {
                if let Some(g) = self.graphs.get(co.graph.path().to_str().unwrap()) {
                    g.param_box_description(operator.title().to_owned())
                } else {
                    lang::ParamBoxDescription::empty()
                }
            }
        }
    }

    /// Parameter box for an "element", i.e. the collection itself.
    pub fn element_param_box(
        &self,
        operator: &lang::Operator,
        element: &Resource<Node>,
    ) -> lang::ParamBoxDescription<lang::MessageWriters> {
        let opbox = self.operator_param_box(operator);
        let elbox = self
            .graphs
            .get(element.directory().unwrap())
            .expect("Unknown node collection")
            .element_param_box(element);
        elbox.merge(opbox.transmitters_into())
    }

    /// Process an event from the application bus and dispatch the necessary
    /// operations to respond.
    ///
    /// Returning None indicates shutdown of the component.
    pub fn process_event(&mut self, event: Arc<lang::Lang>) -> Option<Vec<lang::Lang>> {
        use crate::lang::*;

        match &*event {
            Lang::UserNodeEvent(event) => Some(self.process_user_node_event(event)),
            Lang::UserGraphEvent(event) => Some(self.process_user_graph_event(event)),
            Lang::UserLayersEvent(event) => Some(self.process_user_layers_event(event)),
            Lang::UserIOEvent(event) => self.process_user_io_event(event),
            Lang::IOEvent(IOEvent::NodeDataLoaded(data)) => {
                let mut evs = self.deserialize(data).ok()?;
                let mut response = vec![lang::Lang::GraphEvent(lang::GraphEvent::Cleared)];
                response.append(&mut evs);
                Some(response)
            }
            _ => Some(vec![]),
        }
    }

    fn process_user_node_event(&mut self, event: &lang::UserNodeEvent) -> Vec<lang::Lang> {
        let mut response = vec![];

        match event {
            UserNodeEvent::NewNode(graph_res, op, pos, socket, name) => {
                let graph_name = graph_res.path().to_str().unwrap();
                let op = self.complete_operator(op);
                let mut update_co = None;
                let mut relinearize = false;

                if let Some(ManagedNodeCollection::NodeGraph(graph)) =
                    self.graphs.get_mut(graph_name)
                {
                    // Add node to graph
                    let (node_id, size) = graph.new_node(&op, self.parent_size, name.as_deref());
                    graph.position_node(&node_id, pos.0, pos.1);
                    let resource = Resource::node(
                        [graph_name, &node_id]
                            .iter()
                            .collect::<std::path::PathBuf>(),
                    );

                    // If the node is an input or output, construct update
                    // events for complex operators
                    match op {
                        Operator::AtomicOperator(AtomicOperator::Output(..))
                        | Operator::AtomicOperator(AtomicOperator::Input(..)) => {
                            update_co = Some(graph.complex_operator_stub());
                        }
                        _ => {}
                    }

                    // Autoconnect with socket if specified
                    let mut autoconnect_events = match socket {
                        Some(socket) => match graph.auto_connect(
                            &node_id,
                            socket.file().unwrap(),
                            socket.fragment().unwrap(),
                        ) {
                            Ok(rs) => rs,
                            Err(e) => {
                                log::error!("{}", e);
                                Vec::new()
                            }
                        },
                        None => Vec::new(),
                    };

                    // Construct ordinary responses
                    response.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                        resource.clone(),
                        op.clone(),
                        self.element_param_box(&op, &resource),
                        Some(*pos),
                        size as u32,
                    )));
                    for (socket, imgtype) in op.outputs().iter() {
                        response.push(Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                            resource.node_socket(socket),
                            *imgtype,
                            op.external_data(),
                            size as u32,
                        )));
                    }
                    if !autoconnect_events.is_empty() {
                        response.append(&mut autoconnect_events);
                        relinearize = true;
                    }
                }

                // Process update event if required. This is separate for borrowing reasons
                if let Some(co_stub) = update_co {
                    response.append(&mut self.update_complex_operators(graph_res, &co_stub));
                }

                if relinearize {
                    if let Some(ManagedNodeCollection::NodeGraph(graph)) =
                        self.graphs.get_mut(graph_name)
                    {
                        if let Some(instrs) = graph.linearize(LinearizationMode::TopoSort).map(
                            |(instructions, last_use)| {
                                lang::Lang::GraphEvent(lang::GraphEvent::Relinearized(
                                    graph.graph_resource(),
                                    instructions,
                                    last_use,
                                ))
                            },
                        ) {
                            response.push(instrs);
                            response.push(Lang::GraphEvent(GraphEvent::Recompute(
                                self.active_graph.clone(),
                                Vec::new(),
                            )));
                        }
                    }
                }
            }
            UserNodeEvent::RemoveNode(res) => {
                let node = res.file().unwrap();
                let graph = res.directory().unwrap();

                let mut update_co = None;

                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    match graph.remove_node(node) {
                        Ok((node, removed_conns, co_change)) => {
                            response = removed_conns
                                .iter()
                                .map(|c| {
                                    Lang::GraphEvent(GraphEvent::DisconnectedSockets(
                                        c.0.clone(),
                                        c.1.clone(),
                                    ))
                                })
                                .collect();
                            response.push(Lang::GraphEvent(GraphEvent::NodeRemoved(
                                res.clone(),
                                node.operator.clone(),
                                node.position.clone(),
                            )));
                            if let nodegraph::Node {
                                operator:
                                    Operator::AtomicOperator(AtomicOperator::Output(Output {
                                        output_type,
                                    })),
                                ..
                            } = node
                            {
                                response.push(Lang::GraphEvent(GraphEvent::OutputRemoved(
                                    res.clone(),
                                    output_type,
                                )))
                            }

                            if co_change {
                                let co_stub = graph.complex_operator_stub();
                                update_co = Some((graph.graph_resource(), co_stub));
                            }
                        }
                        Err(e) => log::error!("{}", e),
                    }
                }

                if let Some((res, stub)) = update_co {
                    response.append(&mut self.update_complex_operators(&res, &stub));
                }
            }
            UserNodeEvent::ConnectSockets(from, to) => {
                let from_node = from.file().unwrap();
                let from_socket = from.fragment().unwrap();
                let to_node = to.file().unwrap();
                let to_socket = to.fragment().unwrap();
                let graph = from.directory().unwrap();

                debug_assert_eq!(graph, to.directory().unwrap());

                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    match graph.connect_sockets(from_node, from_socket, to_node, to_socket) {
                        Ok(mut res) => {
                            response.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                                from.clone(),
                                to.clone(),
                            )));
                            response.append(&mut res);

                            if let Some(instrs) = graph.linearize(LinearizationMode::TopoSort).map(
                                |(instructions, last_use)| {
                                    lang::Lang::GraphEvent(lang::GraphEvent::Relinearized(
                                        graph.graph_resource(),
                                        instructions,
                                        last_use,
                                    ))
                                },
                            ) {
                                response.push(instrs);
                                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                                    self.active_graph.clone(),
                                    Vec::new(),
                                )));
                            }
                        }
                        Err(e) => log::error!("{}", e),
                    }
                }
            }
            UserNodeEvent::ConnectBetweenSockets(node, source, sink) => {
                let between_node = node.file().unwrap();
                let source_node = source.file().unwrap();
                let source_socket = source.fragment().unwrap();
                let sink_node = sink.file().unwrap();
                let sink_socket = sink.fragment().unwrap();
                let graph = node.directory().unwrap();

                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    match graph.connect_between(
                        between_node,
                        source_node,
                        source_socket,
                        sink_node,
                        sink_socket,
                    ) {
                        Ok(mut res) => {
                            response.append(&mut res);

                            if let Some(instrs) = graph.linearize(LinearizationMode::TopoSort).map(
                                |(instructions, last_use)| {
                                    lang::Lang::GraphEvent(lang::GraphEvent::Relinearized(
                                        graph.graph_resource(),
                                        instructions,
                                        last_use,
                                    ))
                                },
                            ) {
                                response.push(instrs);
                                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                                    self.active_graph.clone(),
                                    Vec::new(),
                                )));
                            }
                        }
                        Err(e) => log::error!("{}", e),
                    }
                }
            }
            UserNodeEvent::DisconnectSinkSocket(sink) => {
                let node = sink.file().unwrap();
                let socket = sink.fragment().unwrap();
                let graph = sink.directory().unwrap();

                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    match graph.disconnect_sink_socket(node, socket) {
                        Ok(mut r) => response.append(&mut r),
                        Err(e) => log::error!("Error while disconnecting sink {}", e),
                    }
                }
            }
            UserNodeEvent::QuickCombine(op, node_1, node_2) => {
                debug_assert!(node_1.node_graph() == node_2.node_graph());

                let node_1_name = node_1.file().unwrap();
                let node_2_name = node_2.file().unwrap();
                let graph = node_1.directory().unwrap();

                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    match graph.quick_combine(&op, node_1_name, node_2_name, self.parent_size) {
                        Ok(mut res) => {
                            let g_res = graph.graph_resource();
                            let g_instrs = graph.linearize(LinearizationMode::TopoSort);

                            // Rebuild parameter boxes for node added events before publishing
                            for ev in res.iter_mut() {
                                if let Lang::GraphEvent(GraphEvent::NodeAdded(
                                    res,
                                    op,
                                    pbox,
                                    _,
                                    _,
                                )) = ev
                                {
                                    *pbox = self.element_param_box(&op, res)
                                }
                            }

                            response.append(&mut res);

                            if let Some(instrs) = g_instrs {
                                response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                                    g_res, instrs.0, instrs.1,
                                )));
                                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                                    self.active_graph.clone(),
                                    Vec::new(),
                                )));
                            }
                        }
                        Err(e) => log::error!("{}", e),
                    }
                }
            }
            UserNodeEvent::ParameterChange(res, _, data) => {
                if let Some(graph) = self.graphs.get_mut(res.directory().unwrap()) {
                    if let Some(side_effect) = graph.parameter_change(res, data) {
                        response.push(side_effect);
                    }
                    if let Some(instrs) = graph.linearize(LinearizationMode::TopoSort).map(
                        |(instructions, last_use)| {
                            lang::Lang::GraphEvent(lang::GraphEvent::Relinearized(
                                graph.graph_resource(),
                                instructions,
                                last_use,
                            ))
                        },
                    ) {
                        response.push(instrs);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }
            }
            UserNodeEvent::PositionNode(res, (x, y)) => {
                let node = res.file().unwrap();
                let graph = res.directory().unwrap();
                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    graph.position_node(node, *x, *y);
                }
            }
            UserNodeEvent::RenameNode(from, to) => {
                let from_node = from.file().unwrap();
                let to_node = to.file().unwrap();
                let graph = from.directory().unwrap();

                debug_assert_eq!(graph, to.directory().unwrap());

                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    if let Some(r) = graph.rename_node(from_node, to_node) {
                        response.push(r);

                        for spec in self
                            .export_specs
                            .iter_mut()
                            .filter(|spec| &spec.node == from)
                        {
                            spec.node = to.clone();
                        }
                    }
                }
            }
            UserNodeEvent::OutputSizeChange(res, size) => {
                let node = res.file().unwrap();
                let graph = res.directory().unwrap();

                if let Some(ManagedNodeCollection::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                    if let Some(r) = graph.resize_node(node, *size, self.parent_size) {
                        response.push(r);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    };
                }
            }
            UserNodeEvent::ViewSocket(_) => {
                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                    self.active_graph.clone(),
                    Vec::new(),
                )));
            }
        }

        response
    }

    fn process_user_graph_event(&mut self, event: &lang::UserGraphEvent) -> Vec<lang::Lang> {
        let mut response = Vec::new();

        match event {
            UserGraphEvent::AddGraph => {
                let name = (0..)
                    .map(|i| format!("unnamed.{}", i))
                    .find(|n| !self.graphs.contains_key(n))
                    .unwrap();
                self.graphs.insert(
                    name.to_string(),
                    ManagedNodeCollection::NodeGraph(nodegraph::NodeGraph::new(&name)),
                );
                response.push(lang::Lang::GraphEvent(lang::GraphEvent::GraphAdded(
                    Resource::graph(name),
                )));
            }
            UserGraphEvent::ChangeGraph(res) => {
                if let Some(instrs) = self.relinearize(&self.active_graph) {
                    response.push(instrs);
                }
                self.active_graph = res.clone();
                if let Some(instrs) = self.relinearize(&self.active_graph) {
                    response.push(instrs);
                    response.push(Lang::GraphEvent(GraphEvent::Recompute(
                        self.active_graph.clone(),
                        Vec::new(),
                    )));
                }
            }
            UserGraphEvent::RenameGraph(from, to) => {
                if let Some(mut graph) = self.graphs.remove(from.path().to_str().unwrap()) {
                    log::trace!("Renaming graph {} to {}", from, to);
                    // Renaming
                    let new_name = to.path().to_str().unwrap();
                    graph.rename(new_name);

                    // Typically we're renaming the active graph, and thus need to update this
                    if &self.active_graph == from {
                        self.active_graph = to.clone();
                    }

                    // Creating a new complex operator representing this graph
                    let operator = graph.complex_operator_stub();
                    let instructions = graph.linearize(LinearizationMode::TopoSort);

                    self.graphs.insert(new_name.to_string(), graph);
                    response.push(lang::Lang::GraphEvent(lang::GraphEvent::GraphRenamed(
                        from.clone(),
                        to.clone(),
                    )));

                    // Publish linearization of newly named graph
                    if let Some((instrs, last_use)) = instructions {
                        response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                            to.clone(),
                            instrs,
                            last_use,
                        )));
                    }

                    // Update all graphs and linearizations that call the renamed graph
                    response.append(&mut self.update_complex_operators(&from, &operator));
                }
            }
            UserGraphEvent::DeleteGraph(res) => {
                if self.graphs.len() > 1 {
                    self.graphs.remove(res.path_str().unwrap());
                    response.push(Lang::GraphEvent(GraphEvent::GraphRemoved(res.clone())));
                }
            }
            UserGraphEvent::ExposeParameter(res, graph_field, title, control) => {
                let op_stub = {
                    let graph = self
                        .graphs
                        .get_mut(res.directory().unwrap())
                        .expect("Node Graph not found");
                    log::trace!(
                        "Exposing Parameter {} as {}, titled {}, with control {:?}",
                        res,
                        graph_field,
                        title,
                        control,
                    );
                    if let Some(param) =
                        graph.expose_parameter(res.clone(), graph_field, title, control.clone())
                    {
                        response.push(lang::Lang::GraphEvent(lang::GraphEvent::ParameterExposed(
                            res.clone().parameter_node().node_graph(),
                            param.clone(),
                        )));

                        Some(graph.complex_operator_stub())
                    } else {
                        None
                    }
                };
                if let Some(op_stub) = op_stub {
                    response.append(
                        &mut self
                            .update_complex_operators(&res.parameter_node().node_graph(), &op_stub),
                    );
                }
            }
            UserGraphEvent::ConcealParameter(graph_res, graph_field) => {
                let op_stub = {
                    let graph = self
                        .graphs
                        .get_mut(graph_res.path_str().unwrap())
                        .expect("Node Graph not found");
                    if let Some(param) = graph.conceal_parameter(graph_field) {
                        response.push(lang::Lang::GraphEvent(
                            lang::GraphEvent::ParameterConcealed(graph_res.clone(), param),
                        ));
                    }
                    graph.complex_operator_stub()
                };
                response.append(&mut self.update_complex_operators(graph_res, &op_stub));
            }
            UserGraphEvent::RetitleParameter(graph_res, graph_field, _, new_title) => {
                let graph = self
                    .graphs
                    .get_mut(graph_res.path_str().unwrap())
                    .expect("Node Graph not found");
                graph.retitle_parameter(graph_field, new_title);
            }
            UserGraphEvent::RefieldParameter(graph_res, graph_field, new_field) => {
                let graph = self
                    .graphs
                    .get_mut(graph_res.path_str().unwrap())
                    .expect("Node Graph not found");
                graph.refield_parameter(graph_field, new_field);
            }
            UserGraphEvent::Extract(ress) => {
                use itertools::Itertools;

                debug_assert!(!ress.is_empty());
                debug_assert!(ress.iter().map(|r| r.node_graph()).dedup().count() == 1);

                let graph_res = ress[0].node_graph();
                let mut new_graph = None;

                let name = (0..)
                    .map(|i| format!("unnamed.{}", i))
                    .find(|n| !self.graphs.contains_key(n))
                    .unwrap();

                if let Some(ManagedNodeCollection::NodeGraph(graph)) =
                    self.graphs.get_mut(graph_res.path_str().unwrap())
                {
                    match graph.extract(
                        &name,
                        self.parent_size,
                        ress.iter().map(|r| r.file().unwrap()),
                    ) {
                        Ok(x) => new_graph = Some(x),
                        Err(e) => log::error!("{}", e),
                    }
                }

                if let Some((g, mut evs)) = new_graph {
                    // Insert new graph
                    let sub_instructions = g.linearize(LinearizationMode::TopoSort);
                    let sub_graph_res = g.graph_resource();
                    self.graphs.insert(
                        sub_graph_res.path_str().unwrap().to_string(),
                        ManagedNodeCollection::NodeGraph(g),
                    );

                    response.push(lang::Lang::GraphEvent(lang::GraphEvent::GraphAdded(
                        sub_graph_res.clone(),
                    )));

                    // Rebuild parameter boxes for node added events before publishing
                    for ev in evs.iter_mut() {
                        if let Lang::GraphEvent(GraphEvent::NodeAdded(res, op, pbox, _, _)) = ev {
                            *pbox = self.element_param_box(&op, res)
                        }
                    }

                    response.append(&mut evs);

                    // Publish subgraph linearization
                    if let Some((instrs, last_use)) = sub_instructions {
                        response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                            sub_graph_res.clone(),
                            instrs,
                            last_use,
                        )));
                    }

                    // Relinearize the original graph and recompute
                    if let Some((instrs, last_use)) = self
                        .graphs
                        .get_mut(graph_res.path_str().unwrap())
                        .unwrap()
                        .linearize(LinearizationMode::TopoSort)
                    {
                        response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                            graph_res.clone(),
                            instrs,
                            last_use,
                        )));
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            graph_res,
                            Vec::new(),
                        )));
                    }
                }
            }
            UserGraphEvent::Inject(node, op) => {}
        };

        response
    }

    fn process_user_layers_event(&mut self, event: &lang::UserLayersEvent) -> Vec<lang::Lang> {
        let mut response = Vec::new();

        match event {
            UserLayersEvent::AddLayers => {
                let name = (0..)
                    .map(|i| format!("unnamed.{}", i))
                    .find(|n| !self.graphs.contains_key(n))
                    .unwrap();
                let ls = layers::LayerStack::new(&name);
                let outs = ls.output_resources();
                self.graphs
                    .insert(name.to_string(), ManagedNodeCollection::LayerStack(ls));
                response.push(lang::Lang::LayersEvent(lang::LayersEvent::LayersAdded(
                    Resource::graph(name.clone()),
                    self.parent_size,
                    outs,
                )));

                if let ManagedNodeCollection::LayerStack(ls) = self.graphs.get(&name).unwrap() {
                    for (_, (ty, node)) in ls.outputs() {
                        response.push(Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                            node.node_socket("data"),
                            ty,
                            false,
                            self.parent_size,
                        )));
                    }
                }
            }
            UserLayersEvent::DeleteLayers(res) => {
                if self.graphs.len() > 1 {
                    self.graphs.remove(res.path_str().unwrap());
                    response.push(Lang::LayersEvent(LayersEvent::LayersRemoved(res.clone())));
                }
            }
            UserLayersEvent::PushLayer(graph_res, ty, op) => {
                let op = self.complete_operator(op);

                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(graph_res.path_str().unwrap())
                {
                    let res =
                        ls.push_layer(layers::Layer::from(op.clone()), *ty, op.default_name());
                    log::debug!("Added {:?} layer {}", ty, res);

                    let lin = ls.linearize(LinearizationMode::FullTraversal);
                    let mut sockets = ls.layer_sockets(&res);
                    let mut blend_sockets = ls.blend_sockets(&res);
                    let pbox = self.element_param_box(&op, &res);
                    let size = op.size_request().unwrap_or(self.parent_size);

                    response.push(Lang::LayersEvent(LayersEvent::LayerPushed(
                        res,
                        *ty,
                        op.title().to_owned(),
                        op,
                        BlendMode::Mix,
                        1.0,
                        pbox,
                        size,
                    )));
                    response.extend(sockets.drain(0..).map(|(s, t, e)| {
                        Lang::GraphEvent(GraphEvent::OutputSocketAdded(s, t, e, size))
                    }));
                    response.extend(blend_sockets.drain(0..).map(|(s, t)| {
                        Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                            s,
                            t,
                            false,
                            self.parent_size,
                        ))
                    }));
                    if let Some((linearization, last_use)) = lin {
                        response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                            graph_res.to_owned(),
                            linearization,
                            last_use,
                        )))
                    }
                }
            }
            UserLayersEvent::PushMask(for_layer, op) => {
                let op = self.complete_operator(op);
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(for_layer.directory().unwrap())
                {
                    let mask = layers::Mask::from(op.clone());
                    if let Some(res) = ls.push_mask(mask, for_layer, op.default_name()) {
                        log::debug!("Added mask {}", res);

                        let lin = ls.linearize(LinearizationMode::FullTraversal);
                        let mut sockets = ls.mask_sockets(for_layer, &res);
                        let mut blend_sockets = ls.mask_blend_sockets(&res);
                        let pbox = self.element_param_box(&op, &res);

                        response.push(Lang::LayersEvent(LayersEvent::MaskPushed(
                            for_layer.to_owned(),
                            res,
                            op.title().to_owned(),
                            op,
                            BlendMode::Mix,
                            1.0,
                            pbox,
                            self.parent_size,
                        )));
                        response.extend(sockets.drain(0..).map(|(s, _, e)| {
                            Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                                s.clone(),
                                OperatorType::Monomorphic(ImageType::Grayscale),
                                e,
                                self.parent_size,
                            ))
                        }));
                        response.extend(blend_sockets.drain(0..).map(|(s, _)| {
                            Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                                s,
                                OperatorType::Monomorphic(ImageType::Grayscale),
                                false,
                                self.parent_size,
                            ))
                        }));
                        if let Some((linearization, last_use)) = lin {
                            response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                                for_layer.node_graph(),
                                linearization,
                                last_use,
                            )))
                        }
                    }
                }
            }
            UserLayersEvent::RemoveLayer(layer_res) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    if ls.remove_layer(layer_res).is_some() {
                        response.push(Lang::LayersEvent(LayersEvent::LayerRemoved(
                            layer_res.clone(),
                        )));
                    }

                    if let Some(linearize) = self.relinearize(&self.active_graph) {
                        response.push(linearize);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }
            }
            UserLayersEvent::RemoveMask(mask_res) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(mask_res.directory().unwrap())
                {
                    if ls.remove_mask(mask_res).is_some() {
                        response.push(Lang::LayersEvent(LayersEvent::LayerRemoved(
                            mask_res.clone(),
                        )));
                    }

                    if let Some(linearize) = self.relinearize(&self.active_graph) {
                        response.push(linearize);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }
            }
            UserLayersEvent::SetOutput(layer_res, channel, selected, enabled) => {
                let mut update_co = None;

                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    log::debug!(
                        "Set {} output for {} to {}, and enabled {}",
                        channel,
                        layer_res,
                        selected,
                        enabled
                    );

                    ls.set_output(layer_res, *channel, *selected);
                    ls.set_output_channel(layer_res, *channel, *enabled);
                    update_co = Some(ls.complex_operator_stub());

                    if let Some(linearize) = self.relinearize(&self.active_graph) {
                        response.push(linearize);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }

                if let Some(stub) = update_co {
                    response
                        .append(&mut self.update_complex_operators(&layer_res.node_graph(), &stub));
                }
            }
            UserLayersEvent::SetInput(socket_res, channel) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(socket_res.directory().unwrap())
                {
                    log::debug!("Set {} input to {}", socket_res, channel,);

                    response.extend(ls.set_input(socket_res, *channel).drain(0..).map(
                        |(socket, ty)| {
                            Lang::GraphEvent(GraphEvent::SocketMonomorphized(socket, ty))
                        },
                    ));
                    response.extend(
                        ls.type_sanitize_layer(&socket_res.socket_node())
                            .drain(0..)
                            .map(|chan| {
                                Lang::LayersEvent(LayersEvent::OutputUnset(
                                    socket_res.socket_node(),
                                    chan,
                                ))
                            }),
                    );

                    if let Some(linearize) = self.relinearize(&self.active_graph) {
                        response.push(linearize);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }
            }
            UserLayersEvent::SetOpacity(layer_res, _, opacity) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    log::debug!("Set layer opacity of {} to {}", layer_res, opacity);

                    ls.set_layer_opacity(layer_res, *opacity);

                    if let Some(linearize) = self.relinearize(&self.active_graph) {
                        response.push(linearize);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }
            }
            UserLayersEvent::SetBlendMode(layer_res, _, blend_mode) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    log::debug!("Set layer blend mode of {} to {:?}", layer_res, blend_mode);

                    ls.set_layer_blend_mode(layer_res, *blend_mode);

                    if let Some(linearize) = self.relinearize(&self.active_graph) {
                        response.push(linearize);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }
            }
            UserLayersEvent::SetTitle(layer_res, _, title) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    ls.set_title(layer_res, title);
                }
            }
            UserLayersEvent::SetEnabled(layer_res, _, enabled) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    log::debug!("Set layer enabled of {} to {}", layer_res, enabled);

                    ls.set_layer_enabled(layer_res, *enabled);

                    if let Some(linearize) = self.relinearize(&self.active_graph) {
                        response.push(linearize);
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                            Vec::new(),
                        )));
                    }
                }
            }
            UserLayersEvent::MoveUp(layer_res) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    if ls.move_up(layer_res) {
                        response.push(Lang::LayersEvent(LayersEvent::MovedUp(layer_res.clone())));

                        if let Some(linearize) = self.relinearize(&self.active_graph) {
                            response.push(linearize);
                            response.push(Lang::GraphEvent(GraphEvent::Recompute(
                                self.active_graph.clone(),
                                Vec::new(),
                            )));
                        }
                    }
                }
            }
            UserLayersEvent::MoveDown(layer_res) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    if ls.move_down(layer_res) {
                        response.push(Lang::LayersEvent(LayersEvent::MovedDown(layer_res.clone())));

                        if let Some(linearize) = self.relinearize(&self.active_graph) {
                            response.push(linearize);
                            response.push(Lang::GraphEvent(GraphEvent::Recompute(
                                self.active_graph.clone(),
                                Vec::new(),
                            )));
                        }
                    }
                }
            }
            UserLayersEvent::PositionLayer(layer_res, position) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get_mut(layer_res.directory().unwrap())
                {
                    dbg!(position);
                    ls.position_layer(layer_res, position);
                }
            }
            UserLayersEvent::Convert(graph_res) => {
                if let Some(ManagedNodeCollection::LayerStack(ls)) =
                    self.graphs.get(graph_res.path_str().unwrap())
                {
                    if let Some(graph) = ls.to_graph(self.parent_size) {
                        let mut evs = graph.rebuild_events(self.parent_size);
                        let linearization = graph.linearize(LinearizationMode::TopoSort);
                        let new_graph_res = graph.graph_resource();

                        self.graphs.insert(
                            new_graph_res.file().unwrap().to_owned(),
                            ManagedNodeCollection::NodeGraph(graph),
                        );

                        for ev in evs.iter_mut() {
                            if let Lang::GraphEvent(GraphEvent::NodeAdded(res, op, pbox, _, _)) = ev
                            {
                                *pbox = self.element_param_box(&op, res);
                            }
                        }

                        response.push(Lang::GraphEvent(GraphEvent::GraphAdded(
                            new_graph_res.clone(),
                        )));
                        response.extend(evs.drain(0..));

                        if let Some((instrs, last_use)) = linearization {
                            response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                                new_graph_res,
                                instrs,
                                last_use,
                            )))
                        }
                    }
                }
            }
        };

        response
    }

    fn process_user_io_event(&mut self, event: &lang::UserIOEvent) -> Option<Vec<lang::Lang>> {
        let mut response = Vec::new();

        match event {
            UserIOEvent::Quit => return None,
            UserIOEvent::SaveSurface(_) => {
                let data = self.serialize().ok()?;
                response.push(lang::Lang::GraphEvent(lang::GraphEvent::Serialized(data)));
            }
            UserIOEvent::NewSurface => {
                self.graphs.clear();
                self.graphs.insert(
                    "base".to_string(),
                    ManagedNodeCollection::NodeGraph(nodegraph::NodeGraph::new("base")),
                );
                response.push(Lang::GraphEvent(GraphEvent::Cleared));
                response.push(Lang::GraphEvent(GraphEvent::GraphAdded(Resource::graph(
                    "base",
                ))));
            }
            UserIOEvent::SetParentSize(size) => {
                log::trace!("Surface parent size changed to {}", size);
                self.parent_size = *size;
                for g in self.graphs.values_mut() {
                    response.append(&mut g.resize_all(self.parent_size));
                }

                // Recompute on size change
                response.push(Lang::SurfaceEvent(SurfaceEvent::ParentSizeSet(*size)));
                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                    self.active_graph.clone(),
                    Vec::new(),
                )));
            }
            UserIOEvent::NewExportSpec(new, keep_name) => {
                let mut new = new.clone();

                if let Some(out_ty) = self
                    .graphs
                    .get(new.node.node_graph().file().unwrap())
                    .and_then(|graph| graph.output_type(&new.node))
                {
                    if !keep_name {
                        new.name = out_ty.to_string();
                    }
                    self.export_specs.push(new.clone());
                    response.push(Lang::SurfaceEvent(SurfaceEvent::ExportSpecDeclared(new)));
                }
            }
            UserIOEvent::UpdateExportSpec(name, new) => {
                if let Some(idx) = self.export_specs.iter().position(|spec| &spec.name == name) {
                    let old = self.export_specs[idx].clone();
                    self.export_specs[idx] = new.clone();
                    response.push(Lang::SurfaceEvent(SurfaceEvent::ExportSpecUpdated(
                        old,
                        self.export_specs[idx].clone(),
                    )));
                }
            }
            UserIOEvent::RemoveExportSpec(name) => {
                if let Some(idx) = self.export_specs.iter().position(|spec| &spec.name == name) {
                    let spec = self.export_specs.remove(idx);
                    response.push(Lang::SurfaceEvent(SurfaceEvent::ExportSpecRemoved(spec)));
                }
            }
            UserIOEvent::SetImageColorSpace(_, _)
            | UserIOEvent::ReloadImageResource(..)
            | UserIOEvent::ReloadSvgResource(..) => {
                // Various IO changes should trigger a subsequent recompute
                // without incurring any other work in nodes.
                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                    self.active_graph.clone(),
                    Vec::new(),
                )));
            }
            UserIOEvent::RunExports(base) => {
                use itertools::Itertools;
                for (graph, export) in self
                    .export_specs
                    .iter()
                    .map(|spec| {
                        let mut path = base.clone();
                        path.set_file_name(format!(
                            "{}_{}.{}",
                            path.file_name().unwrap().to_str().unwrap(),
                            spec.name,
                            spec.format.file_extension(),
                        ));
                        (spec.node.node_graph(), (spec.clone(), path))
                    })
                    .into_group_map()
                    .drain()
                {
                    response.push(Lang::GraphEvent(GraphEvent::Recompute(graph, export)))
                }
            }
            _ => {}
        }

        Some(response)
    }

    /// Construct update events for complex operators after a change to a graph.
    fn update_complex_operators(
        &mut self,
        changed_graph: &lang::Resource<lang::Graph>,
        op_stub: &lang::ComplexOperator,
    ) -> Vec<lang::Lang> {
        use lang::*;

        let mut response = Vec::new();

        let pbox_prototype = self.operator_param_box(&Operator::ComplexOperator(op_stub.clone()));

        for graph in self.graphs.values_mut() {
            let (updated, mut socket_updates) =
                graph.update_complex_operators(self.parent_size, &changed_graph, &op_stub);

            if !updated.is_empty() {
                if let Some((instructions, last_use)) = graph.linearize(LinearizationMode::TopoSort)
                {
                    response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                        graph.graph_resource(),
                        instructions,
                        last_use,
                    )));
                }
            }

            for (node, params) in updated {
                // Update param boxes
                let mut pbox = pbox_prototype.clone();

                for param in pbox.parameters_mut() {
                    if let Some(subs) = params.get(&param.transmitter.0) {
                        param.control.set_value(subs.get_value());
                    }
                }

                let elbox = graph.element_param_box(&node);

                response.push(Lang::GraphEvent(GraphEvent::ComplexOperatorUpdated(
                    node.clone(),
                    op_stub.clone(),
                    elbox.merge(pbox.transmitters_into()),
                )));

                // Update output images
                response.extend(socket_updates.drain(0..).map(Lang::GraphEvent));
            }
        }

        response
    }

    /// Run the linearization procedure on a graph.
    fn relinearize(&self, graph: &lang::Resource<lang::Graph>) -> Option<lang::Lang> {
        self.graphs
            .get(graph.path_str().unwrap())?
            .linearize(LinearizationMode::TopoSort)
            .map(|(instructions, last_use)| {
                lang::Lang::GraphEvent(lang::GraphEvent::Relinearized(
                    graph.clone(),
                    instructions,
                    last_use,
                ))
            })
    }

    fn complete_operator(&self, op: &Operator) -> Operator {
        match op {
            lang::Operator::ComplexOperator(co) => {
                let co = self
                    .graphs
                    .get(co.graph.file().unwrap())
                    .unwrap()
                    .complex_operator_stub();
                lang::Operator::ComplexOperator(co)
            }
            lang::Operator::AtomicOperator(_) => op.clone(),
        }
    }
}

impl Default for NodeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the node manager thread.
///
/// Designed to exist once in the system.
pub fn start_nodes_thread(broker: &mut broker::Broker<lang::Lang>) -> thread::JoinHandle<()> {
    log::info!("Starting Node Manager");
    let (sender, receiver, disconnector) = broker.subscribe("nodes");

    thread::Builder::new()
        .name("nodes".to_string())
        .spawn(move || {
            let mut node_mgr = NodeManager::new();

            for event in receiver {
                match node_mgr.process_event(event) {
                    None => break,
                    Some(response) => {
                        for ev in response {
                            if sender.send(ev).is_none() {
                                log::error!("Node Manager lost connection to application bus!");
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
