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
/// graphs.
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
    fn conceal_parameter(&mut self, graph_field: &str) {
        self.exposed_parameters_mut().remove(graph_field);
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
            box_title: title,
            categories: vec![ParamCategory {
                name: "Exposed Parameters",
                parameters: self
                    .exposed_parameters()
                    .iter()
                    .map(|(k, v)| Parameter {
                        name: v.title.clone(),
                        transmitter: Field(k.clone()),
                        control: v.control.clone(),
                        expose_status: Some(ExposeStatus::Unexposed),
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LinearizationMode {
    TopoSort,
    FullTraversal,
}

/// General functions of a node graph
#[enum_dispatch]
trait NodeCollection {
    /// Obtain the inputs, i.e. set of input nodes, in the node graph
    fn inputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)>;

    /// Obtain the outputs, i.e. set of output nodes, in the node graph
    fn outputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)>;

    fn graph_resource(&self) -> Resource<Graph>;
    fn rename(&mut self, name: &str);

    /// Linearize this node graph into a vector of instructions that can be
    /// interpreted by the compute backend.
    fn linearize(&self, mode: LinearizationMode) -> Option<(Linearization, LastUses)>;

    /// Change a parameter in a resource in this node collection.
    fn parameter_change(
        &mut self,
        resource: &Resource<Param>,
        data: &[u8],
    ) -> Result<Option<Lang>, String>;

    /// Update all the complex operators matching a call to the old graph.
    /// Returns a vector of all node resources that have been updated.
    fn update_complex_operators(
        &mut self,
        graph: &Resource<Graph>,
        new: &ComplexOperator,
    ) -> Vec<(Resource<Node>, HashMap<String, ParamSubstitution>)>;

    /// Resize all the nodes in the collection with the new parent size.
    fn resize_all(&mut self, parent_size: u32) -> Vec<Lang>;

    /// Rebuild all events that create this collection. Note that parameter boxes
    /// will be left empty, since not all information is available to build them
    /// in the case of complex operators.
    fn rebuild_events(&self, parent_size: u32) -> Vec<Lang>;
}

#[enum_dispatch(ExposedParameters, NodeCollection)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeGraph {
    NodeGraph(nodegraph::NodeGraph),
    LayerStack(layers::LayerStack),
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::NodeGraph(nodegraph::NodeGraph::new("base"))
    }
}

struct NodeManager {
    parent_size: u32,
    export_specs: HashMap<String, lang::ExportSpec>,
    graphs: HashMap<String, NodeGraph>,
    active_graph: lang::Resource<lang::Graph>,
}

// FIXME: Changing output socket type after connection has already been made does not propagate type changes into preceeding polymorphic nodes!
impl NodeManager {
    pub fn new() -> Self {
        NodeManager {
            parent_size: 1024,
            export_specs: HashMap::new(),
            graphs: hashmap! { "base".to_string() => NodeGraph::default() },
            active_graph: lang::Resource::graph("base", None),
        }
    }

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

    pub fn process_event(&mut self, event: Arc<lang::Lang>) -> Option<Vec<lang::Lang>> {
        use crate::lang::*;
        let mut response = vec![];

        match &*event {
            Lang::UserNodeEvent(event) => match event {
                UserNodeEvent::NewNode(graph, op, pos) => {
                    let graph_name = graph.path().to_str().unwrap();
                    let op = match op {
                        lang::Operator::ComplexOperator(co) => {
                            let co = self
                                .graphs
                                .get(co.graph.file().unwrap())
                                .unwrap()
                                .complex_operator_stub();
                            lang::Operator::ComplexOperator(co)
                        }
                        lang::Operator::AtomicOperator(_) => op.clone(),
                    };

                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph_name) {
                        let (node_id, size) = graph.new_node(&op, self.parent_size);
                        let resource = Resource::node(
                            [graph_name, &node_id]
                                .iter()
                                .collect::<std::path::PathBuf>(),
                            None,
                        );
                        response.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                            resource.clone(),
                            op.clone(),
                            self.operator_param_box(&op),
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
                    }
                }
                UserNodeEvent::RemoveNode(res) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();

                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                        match graph.remove_node(node) {
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
                                response
                                    .push(Lang::GraphEvent(GraphEvent::NodeRemoved(res.clone())));
                                if let Some(ty) = ty {
                                    response.push(Lang::GraphEvent(GraphEvent::OutputRemoved(
                                        res.clone(),
                                        ty,
                                    )))
                                }
                            }
                            Err(e) => log::error!("{}", e),
                        }
                    }
                }
                UserNodeEvent::ConnectSockets(from, to) => {
                    let from_node = from.file().unwrap();
                    let from_socket = from.fragment().unwrap();
                    let to_node = to.file().unwrap();
                    let to_socket = to.fragment().unwrap();
                    let graph = from.directory().unwrap();

                    debug_assert_eq!(graph, to.directory().unwrap());

                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                        match graph.connect_sockets(from_node, from_socket, to_node, to_socket) {
                            Ok(mut res) => {
                                response.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                                    from.clone(),
                                    to.clone(),
                                )));
                                response.append(&mut res);

                                if let Some(instrs) = graph
                                    .linearize(LinearizationMode::TopoSort)
                                    .map(|(instructions, last_use)| {
                                        lang::Lang::GraphEvent(lang::GraphEvent::Relinearized(
                                            graph.graph_resource(),
                                            instructions,
                                            last_use,
                                        ))
                                    })
                                {
                                    response.push(instrs);
                                }
                            }
                            Err(e) => log::error!("{}", e),
                        }
                    }

                    response.push(Lang::GraphEvent(GraphEvent::Recompute(
                        self.active_graph.clone(),
                    )));
                }
                UserNodeEvent::DisconnectSinkSocket(sink) => {
                    let node = sink.file().unwrap();
                    let socket = sink.fragment().unwrap();
                    let graph = sink.directory().unwrap();

                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                        match graph.disconnect_sink_socket(node, socket) {
                            Ok(mut r) => response.append(&mut r),
                            Err(e) => log::error!("Error while disconnecting sink {}", e),
                        }
                    }
                }
                UserNodeEvent::ParameterChange(res, data) => {
                    if let Some(graph) = self.graphs.get_mut(res.directory().unwrap()) {
                        if let Some(side_effect) = graph.parameter_change(res, data).unwrap() {
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
                        }
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                        )));
                    }
                }
                UserNodeEvent::PositionNode(res, (x, y)) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();
                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                        graph.position_node(node, *x, *y);
                    }
                }
                UserNodeEvent::RenameNode(from, to) => {
                    let from_node = from.file().unwrap();
                    let to_node = to.file().unwrap();
                    let graph = from.directory().unwrap();

                    debug_assert_eq!(graph, to.directory().unwrap());

                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                        if let Some(r) = graph.rename_node(from_node, to_node) {
                            response.push(r);
                        }
                    }
                }
                UserNodeEvent::OutputSizeChange(res, size) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();

                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                        if let Some(r) =
                            graph.resize_node(node, Some(*size), None, self.parent_size)
                        {
                            response.push(r);
                        };
                    }
                }
                UserNodeEvent::OutputSizeAbsolute(res, abs) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();

                    if let Some(NodeGraph::NodeGraph(graph)) = self.graphs.get_mut(graph) {
                        if let Some(r) = graph.resize_node(node, None, Some(*abs), self.parent_size)
                        {
                            response.push(r);
                        };
                    }
                }
            },
            Lang::UserGraphEvent(event) => match event {
                UserGraphEvent::AddGraph => {
                    let name = (0..)
                        .map(|i| format!("unnamed.{}", i))
                        .find(|n| !self.graphs.contains_key(n))
                        .unwrap();
                    self.graphs.insert(
                        name.to_string(),
                        NodeGraph::NodeGraph(nodegraph::NodeGraph::new(&name)),
                    );
                    response.push(lang::Lang::GraphEvent(lang::GraphEvent::GraphAdded(
                        Resource::graph(name, None),
                    )));
                }
                UserGraphEvent::ChangeGraph(res) => {
                    if let Some(instrs) = self.relinearize(&self.active_graph) {
                        response.push(instrs);
                    }
                    self.active_graph = res.clone();
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
                            response.push(lang::Lang::GraphEvent(
                                lang::GraphEvent::ParameterExposed(
                                    res.clone().parameter_node().node_graph(),
                                    param.clone(),
                                ),
                            ));

                            Some(graph.complex_operator_stub())
                        } else {
                            None
                        }
                    };
                    if let Some(op_stub) = op_stub {
                        response.append(&mut self.update_complex_operators(
                            &res.parameter_node().node_graph(),
                            &op_stub,
                        ));
                    }
                }
                UserGraphEvent::ConcealParameter(graph_res, graph_field) => {
                    let op_stub = {
                        let graph = self
                            .graphs
                            .get_mut(graph_res.path_str().unwrap())
                            .expect("Node Graph not found");
                        graph.conceal_parameter(graph_field);
                        response.push(lang::Lang::GraphEvent(
                            lang::GraphEvent::ParameterConcealed(
                                graph_res.clone(),
                                graph_field.clone(),
                            ),
                        ));
                        graph.complex_operator_stub()
                    };
                    response.append(&mut self.update_complex_operators(graph_res, &op_stub));
                }
                UserGraphEvent::RetitleParameter(graph_res, graph_field, new_title) => {
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
            },
            Lang::UserLayersEvent(event) => match event {
                UserLayersEvent::AddLayers => {
                    let name = (0..)
                        .map(|i| format!("unnamed.{}", i))
                        .find(|n| !self.graphs.contains_key(n))
                        .unwrap();
                    self.graphs.insert(
                        name.to_string(),
                        NodeGraph::LayerStack(layers::LayerStack::new(&name)),
                    );
                    response.push(lang::Lang::LayersEvent(lang::LayersEvent::LayersAdded(
                        Resource::graph(name.clone(), None),
                        self.parent_size,
                    )));

                    if let NodeGraph::LayerStack(ls) = self.graphs.get(&name).unwrap() {
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
                UserLayersEvent::PushLayer(graph_res, ty, op) => {
                    if let Some(NodeGraph::LayerStack(ls)) =
                        self.graphs.get_mut(graph_res.path_str().unwrap())
                    {
                        let res = match ty {
                            LayerType::Fill => ls
                                .push_fill(layers::FillLayer::from_operator(op), op.default_name()),
                            LayerType::Fx => {
                                ls.push_fx(layers::FxLayer::from_operator(op), op.default_name())
                            }
                        };
                        log::debug!("Added {:?} layer {}", ty, res);
                        let lin = ls.linearize(LinearizationMode::FullTraversal);
                        let mut sockets = ls.layer_sockets(&res);
                        let mut blend_sockets = ls.blend_sockets(&res);
                        response.push(Lang::LayersEvent(LayersEvent::LayerPushed(
                            res,
                            *ty,
                            op.clone(),
                            self.operator_param_box(op),
                            self.parent_size,
                        )));
                        response.extend(sockets.drain(0..).map(|(s, t, e)| {
                            Lang::GraphEvent(GraphEvent::OutputSocketAdded(
                                s,
                                t,
                                e,
                                self.parent_size,
                            ))
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
                UserLayersEvent::SetOutput(layer_res, channel, selected, enabled) => {
                    if let Some(NodeGraph::LayerStack(ls)) =
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

                        if let Some(linearize) = self.relinearize(&self.active_graph) {
                            response.push(linearize);
                        }
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                        )));
                    }
                }
                UserLayersEvent::SetInput(socket_res, channel) => {}
                UserLayersEvent::SetOpacity(layer_res, opacity) => {
                    if let Some(NodeGraph::LayerStack(ls)) =
                        self.graphs.get_mut(layer_res.directory().unwrap())
                    {
                        log::debug!("Set layer opacity of {} to {}", layer_res, opacity);

                        ls.set_layer_opacity(layer_res, *opacity);

                        if let Some(linearize) = self.relinearize(&self.active_graph) {
                            response.push(linearize);
                        }
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                        )));
                    }
                }
                UserLayersEvent::SetBlendMode(layer_res, blend_mode) => {
                    if let Some(NodeGraph::LayerStack(ls)) =
                        self.graphs.get_mut(layer_res.directory().unwrap())
                    {
                        log::debug!("Set layer blend mode of {} to {:?}", layer_res, blend_mode);

                        ls.set_layer_blend_mode(layer_res, *blend_mode);

                        if let Some(linearize) = self.relinearize(&self.active_graph) {
                            response.push(linearize);
                        }
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                        )));
                    }
                }
            },
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::UserIOEvent(UserIOEvent::OpenSurface(path)) => {
                match self.open_surface(path) {
                    Ok(mut evs) => {
                        response.push(Lang::GraphEvent(GraphEvent::Cleared));
                        response.append(&mut evs);

                        // Automatically recompute on load
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                        )));
                    }
                    Err(e) => log::error!("{}", e),
                }
            }
            Lang::UserIOEvent(UserIOEvent::SaveSurface(path)) => {
                if let Err(e) = self.save_surface(path) {
                    log::error!("{}", e)
                }
            }
            Lang::UserIOEvent(UserIOEvent::NewSurface) => {
                self.graphs.clear();
                self.graphs.insert(
                    "base".to_string(),
                    NodeGraph::NodeGraph(nodegraph::NodeGraph::new("base")),
                );
                response.push(Lang::GraphEvent(GraphEvent::Cleared));
                response.push(Lang::GraphEvent(GraphEvent::GraphAdded(Resource::graph(
                    "base", None,
                ))));
            }
            Lang::UserIOEvent(UserIOEvent::SetParentSize(size)) => {
                self.parent_size = *size;
                for g in self.graphs.values_mut() {
                    response.append(&mut g.resize_all(self.parent_size));
                }

                // Recompute on size change
                if let Some(linearize) = self.relinearize(&self.active_graph) {
                    response.push(linearize);
                }
                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                    self.active_graph.clone(),
                )));
            }
            Lang::UserIOEvent(UserIOEvent::DeclareExport(name, spec)) => {
                self.export_specs.insert(name.clone(), spec.clone());
            }
            Lang::UserIOEvent(UserIOEvent::RenameExport(from, to)) => {
                if let Some(spec) = self.export_specs.remove(from) {
                    self.export_specs.insert(to.clone(), spec);
                }
            }
            Lang::UserIOEvent(UserIOEvent::RunExports(base)) => {
                for (name, spec) in self.export_specs.iter() {
                    let mut path = base.clone();
                    path.set_file_name(format!(
                        "{}_{}.png",
                        path.file_name().unwrap().to_str().unwrap(),
                        name
                    ));
                    log::debug!("Dispatching export to {:#?}", path);
                    response.push(Lang::SurfaceEvent(SurfaceEvent::ExportImage(
                        spec.clone(),
                        self.parent_size,
                        path,
                    )));
                }
            }
            _ => {}
        }

        Some(response)
    }

    fn update_complex_operators(
        &mut self,
        changed_graph: &lang::Resource<lang::Graph>,
        op_stub: &lang::ComplexOperator,
    ) -> Vec<lang::Lang> {
        use lang::*;

        let mut response = Vec::new();

        let pbox_prototype = self.operator_param_box(&Operator::ComplexOperator(op_stub.clone()));

        for graph in self.graphs.values_mut() {
            let updated = graph.update_complex_operators(&changed_graph, &op_stub);

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
                let mut pbox = pbox_prototype.clone();
                for param in pbox.parameters_mut() {
                    if let Some(subs) = params.get(&param.transmitter.0) {
                        param.control.set_value(subs.get_value());
                    }
                }

                response.push(Lang::GraphEvent(GraphEvent::ComplexOperatorUpdated(
                    node.clone(),
                    op_stub.clone(),
                    pbox,
                )))
            }
        }

        response
    }

    fn relinearize(&self, graph: &lang::Resource<lang::Graph>) -> Option<lang::Lang> {
        self.graphs
            .get(graph.path_str().unwrap())
            .unwrap()
            .linearize(LinearizationMode::TopoSort)
            .map(|(instructions, last_use)| {
                lang::Lang::GraphEvent(lang::GraphEvent::Relinearized(
                    graph.clone(),
                    instructions,
                    last_use,
                ))
            })
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
