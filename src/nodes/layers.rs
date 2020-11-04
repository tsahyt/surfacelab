use crate::lang::*;
use enumset::EnumSet;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::iter::FromIterator;
use strum::IntoEnumIterator;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillLayer {
    title: String,
    fill: Fill,
    blend_options: LayerBlendOptions,
}

impl FillLayer {
    pub fn from_operator(op: &Operator) -> Self {
        FillLayer {
            title: op.title().to_owned(),
            fill: Fill {
                operator: op.clone(),
                output_sockets: HashMap::new(),
            },
            blend_options: LayerBlendOptions::default(),
        }
    }
}

/// A type encoding a function from material channels to sockets.
type ChannelMap = HashMap<MaterialChannel, String>;

/// A type encoding a function from sockets to material channels.
type InputMap = HashMap<String, MaterialChannel>;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// A fill layer using an operator, with its output sockets mapped to
/// material channels. The operator must not have any inputs! It can be
/// complex or atomic. The requirement to not have inputs means that most
/// atomic operators are not usable, outside of noises etc., and the
/// operator is most likely complex.
pub struct Fill {
    operator: Operator,
    output_sockets: ChannelMap,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// An FX layer is a layer that uses the material underneath it as input.
/// Therefore FX layers *cannot* be placed at the bottom of a layer stack.
pub struct FxLayer {
    title: String,
    operator: Operator,
    input_sockets: InputMap,
    output_sockets: ChannelMap,
    blend_options: LayerBlendOptions,
}

impl FxLayer {
    pub fn from_operator(op: &Operator) -> Self {
        FxLayer {
            title: op.title().to_owned(),
            operator: op.clone(),
            input_sockets: HashMap::from_iter(
                op.inputs()
                    .drain()
                    .map(|(k, _)| (k, MaterialChannel::Displacement)),
            ),
            output_sockets: HashMap::new(),
            blend_options: LayerBlendOptions::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mask {
    operator: Operator,
    factor: f32,
    blend_mode: BlendMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaskStack(Vec<Mask>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerBlendOptions {
    mask: MaskStack,
    opacity: f32,
    channels: EnumSet<MaterialChannel>,
    blend_mode: BlendMode,
    enabled: bool,
}

impl LayerBlendOptions {
    /// Create a blend operator from the given blend options. The output will
    /// *always* be clamped!
    pub fn blend_operator(&self) -> Blend {
        Blend {
            blend_mode: self.blend_mode,
            mix: self.opacity,
            sharpness: 16.0,
            clamp_output: 1,
        }
    }
}

impl Default for LayerBlendOptions {
    fn default() -> Self {
        LayerBlendOptions {
            mask: MaskStack(Vec::new()),
            opacity: 1.0,
            channels: EnumSet::empty(),
            blend_mode: BlendMode::Mix,
            enabled: true,
        }
    }
}

/// A layer is either a fill layer or an FX layer. Each layer has a name, such
/// that it can be referenced via a Resource. The resource type for a layer is
/// `Resource<Node>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Layer {
    FillLayer(String, FillLayer),
    FxLayer(String, FxLayer),
}

impl Layer {
    pub fn name(&self) -> &str {
        match self {
            Layer::FillLayer(s, _) => s,
            Layer::FxLayer(s, _) => s,
        }
    }

    pub fn layer_type(&self) -> LayerType {
        match self {
            Layer::FillLayer(_, _) => LayerType::Fill,
            Layer::FxLayer(_, _) => LayerType::Fx,
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.opacity = opacity;
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.opacity = opacity;
            }
        }
    }

    pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.blend_mode = blend_mode;
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.blend_mode = blend_mode;
            }
        }
    }

    pub fn get_output_channels(&self) -> EnumSet<MaterialChannel> {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => blend_options.channels,
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => blend_options.channels,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Layer::FillLayer(_, l) => &l.title,
            Layer::FxLayer(_, l) => &l.title,
        }
    }

    pub fn set_title(&mut self, title: &str) {
        match self {
            Layer::FillLayer(_, l) => {
                l.title = title.to_owned();
            }
            Layer::FxLayer(_, l) => {
                l.title = title.to_owned();
            }
        }
    }

    pub fn operator(&self) -> &Operator {
        match self {
            Layer::FillLayer(_, l) => &l.fill.operator,
            Layer::FxLayer(_, l) => &l.operator,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerStack {
    name: String,
    layers: Vec<Layer>,
    resources: HashMap<String, usize>,
    parameters: HashMap<String, GraphParameter>,
}

impl LayerStack {
    pub fn new(name: &str) -> Self {
        LayerStack {
            name: name.to_owned(),
            layers: Vec::new(),
            resources: HashMap::new(),
            parameters: HashMap::new(),
        }
    }

    fn next_free_name(&self, base_name: &str) -> String {
        let mut resource = String::new();

        for i in 1.. {
            let name = format!("{}.{}", base_name, i);

            if !self.resources.contains_key(&name) {
                resource = name;
                break;
            }
        }

        resource
    }

    pub fn layer_resource(&self, layer: &Layer) -> Resource<Node> {
        Resource::node(&format!("{}/{}", self.name, layer.name()), None)
    }

    pub fn material_layer_resource(
        &self,
        layer: &Layer,
        channel: MaterialChannel,
    ) -> Resource<Node> {
        Resource::node(
            &format!("{}/{}.{}", self.name, layer.name(), channel.short_name()),
            None,
        )
    }

    pub fn blend_resource(&self, layer: &Layer, channel: MaterialChannel) -> Resource<Node> {
        Resource::node(
            &format!(
                "{}/{}.blend.{}",
                self.name,
                layer.name(),
                channel.short_name()
            ),
            None,
        )
    }

    fn push(&mut self, layer: Layer, resource: &Resource<Node>) {
        self.layers.push(layer);
        self.resources
            .insert(resource.file().unwrap().to_owned(), self.layers.len() - 1);
    }

    pub fn push_fill(&mut self, layer: FillLayer, base_name: &str) -> Resource<Node> {
        let resource = Resource::node(
            &format!("{}/{}", self.name, self.next_free_name(base_name)),
            None,
        );
        let layer = Layer::FillLayer(resource.file().unwrap().to_owned(), layer);
        self.push(layer, &resource);
        resource
    }

    pub fn push_fx(&mut self, layer: FxLayer, base_name: &str) -> Resource<Node> {
        let resource = Resource::node(
            &format!("{}/{}", self.name, self.next_free_name(base_name)),
            None,
        );
        let layer = Layer::FxLayer(resource.file().unwrap().to_owned(), layer);
        self.push(layer, &resource);
        resource
    }

    pub fn remove(&mut self, resource: &Resource<Node>) {
        if let Some(index) = self.resources.remove(resource.file().unwrap()) {
            self.layers.remove(index);

            for idx in self.resources.values_mut().filter(|i| **i >= index) {
                *idx -= 1;
            }
        }
    }

    pub fn reset(&mut self) {
        self.layers.clear();
        self.resources.clear();
    }

    pub fn output_resource(&self, channel: MaterialChannel) -> Resource<Node> {
        Resource::node(
            &format!("{}/output.{}", self.name, channel.short_name(),),
            None,
        )
    }

    pub fn all_resources(&self) -> Vec<Resource<Node>> {
        self.layers
            .iter()
            .map(|layer| match layer {
                Layer::FillLayer(_, FillLayer { blend_options, .. }) => blend_options
                    .channels
                    .iter()
                    .map(|channel| self.blend_resource(layer, channel))
                    .chain(std::iter::once(self.layer_resource(layer)))
                    .collect::<Vec<_>>(),
                Layer::FxLayer(_, FxLayer { blend_options, .. }) => blend_options
                    .channels
                    .iter()
                    .map(|channel| self.blend_resource(layer, channel))
                    .chain(std::iter::once(self.layer_resource(layer)))
                    .collect::<Vec<_>>(),
            })
            .flatten()
            .collect()
    }

    /// Return all output sockets of the given layer
    pub fn layer_sockets(
        &self,
        layer: &Resource<Node>,
    ) -> Vec<(Resource<Socket>, OperatorType, bool)> {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            match &self.layers[*idx] {
                Layer::FillLayer(_, fill) => fill
                    .fill
                    .operator
                    .outputs()
                    .iter()
                    .map(|(s, t)| (layer.node_socket(s), *t, fill.fill.operator.external_data()))
                    .collect(),
                Layer::FxLayer(_, fx) => fx
                    .operator
                    .outputs()
                    .iter()
                    .map(|(s, t)| (layer.node_socket(s), *t, fx.operator.external_data()))
                    .collect(),
            }
        } else {
            Vec::new()
        }
    }

    /// Return all blend sockets of the given layer
    pub fn blend_sockets(&self, layer: &Resource<Node>) -> Vec<(Resource<Socket>, OperatorType)> {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            let layer = &self.layers[*idx];
            MaterialChannel::iter()
                .map(|channel| {
                    (
                        self.blend_resource(layer, channel).node_socket("color"),
                        OperatorType::Monomorphic(channel.to_image_type()),
                    )
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn set_title(&mut self, layer: &Resource<Node>, title: &str) {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            self.layers[*idx].set_title(title);
        }
    }

    pub fn set_output(
        &mut self,
        layer: &Resource<Node>,
        channel: MaterialChannel,
        socket_index: usize,
    ) {
        use itertools::Itertools;

        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            match &mut self.layers[*idx] {
                Layer::FillLayer(_, fill) => {
                    let socket = fill
                        .fill
                        .operator
                        .outputs()
                        .keys()
                        .sorted()
                        .nth(socket_index)
                        .cloned()
                        .unwrap();
                    fill.fill.output_sockets.insert(channel, socket);
                }
                Layer::FxLayer(_, fx) => {
                    let socket = fx
                        .operator
                        .outputs()
                        .keys()
                        .sorted()
                        .nth(socket_index)
                        .cloned()
                        .unwrap();
                    fx.output_sockets.insert(channel, socket);
                }
            }
        }
    }

    pub fn set_input(&mut self, layer_socket: &Resource<Socket>, channel: MaterialChannel) {
        if let Some(idx) = self.resources.get(layer_socket.file().unwrap()) {
            if let Layer::FxLayer(_, fx) = &mut self.layers[*idx] {
                fx.input_sockets
                    .insert(layer_socket.fragment().unwrap().to_owned(), channel);
            }
        }
    }

    pub fn set_output_channel(
        &mut self,
        layer: &Resource<Node>,
        channel: MaterialChannel,
        visibility: bool,
    ) {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            match &mut self.layers[*idx] {
                Layer::FillLayer(_, fill) => {
                    if visibility {
                        fill.blend_options.channels.insert(channel);
                    } else {
                        fill.blend_options.channels.remove(channel);
                    }
                }
                Layer::FxLayer(_, fx) => {
                    if visibility {
                        fx.blend_options.channels.insert(channel);
                    } else {
                        fx.blend_options.channels.remove(channel);
                    }
                }
            }
        }
    }

    pub fn set_layer_opacity(&mut self, layer: &Resource<Node>, opacity: f32) {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            self.layers[*idx].set_opacity(opacity);
        }
    }

    pub fn set_layer_blend_mode(&mut self, layer: &Resource<Node>, blend_mode: BlendMode) {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            self.layers[*idx].set_blend_mode(blend_mode)
        }
    }
}

impl super::ExposedParameters for LayerStack {
    fn exposed_parameters(&self) -> &HashMap<String, GraphParameter> {
        &self.parameters
    }

    fn exposed_parameters_mut(&mut self) -> &mut HashMap<String, GraphParameter> {
        &mut self.parameters
    }
}

impl super::NodeCollection for LayerStack {
    /// Layer stacks do not have inputs, so this always returns an empty HashMap.
    fn inputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)> {
        HashMap::new()
    }

    /// Layer stacks always have the same set of outputs, one per possible material channel.
    fn outputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)> {
        let channels = self
            .layers
            .iter()
            .map(|l| l.get_output_channels())
            .fold(EnumSet::empty(), |z, c| z.union(c));

        HashMap::from_iter(channels.iter().map(|channel| {
            (
                channel.short_name().to_string(),
                (
                    OperatorType::Monomorphic(channel.to_image_type()),
                    self.output_resource(channel),
                ),
            )
        }))
    }

    fn graph_resource(&self) -> Resource<Graph> {
        Resource::graph(self.name.clone(), None)
    }

    fn rename(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Linearize this layer stack into a vector of instructions to be
    /// interpreted by the compute backend. Analogous to the similarly named
    /// function in the NodeGraph.
    ///
    /// The linearization mode is ignored for layer stacks.
    fn linearize(&self, _mode: super::LinearizationMode) -> Option<(Linearization, UsePoints)> {
        let mut linearization = Vec::new();
        let mut use_points: HashMap<Resource<Node>, UsePoint> = HashMap::new();
        let mut step = 0;

        let mut last_socket: HashMap<MaterialChannel, Resource<Socket>> = HashMap::new();

        for layer in self.layers.iter() {
            match layer {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        blend_options,
                        fill:
                            Fill {
                                operator,
                                output_sockets,
                            },
                        ..
                    },
                ) => {
                    // Skip execution if no channels are blended or the layer is disabled.
                    if blend_options.channels.is_empty() || !blend_options.enabled {
                        continue;
                    }

                    step += 1;

                    let resource = self.layer_resource(layer);
                    match operator {
                        Operator::AtomicOperator(aop) => {
                            linearization.push(Instruction::Execute(resource.clone(), aop.clone()));
                        }
                        Operator::ComplexOperator(cop) => {
                            // Inputs can be skipped vs the nodegraph
                            // linearization, since fill layers must not have
                            // inputs.
                            linearization.push(Instruction::Call(resource.clone(), cop.clone()));
                            for (out_socket, (_, output)) in cop.outputs.iter() {
                                linearization.push(Instruction::Copy(
                                    output.node_socket("data"),
                                    resource.node_socket(out_socket),
                                ))
                            }
                        }
                    }

                    use_points
                        .entry(resource.clone())
                        .and_modify(|e| e.creation = step)
                        .or_insert(UsePoint {
                            last: usize::MAX,
                            creation: step,
                        });

                    for (channel, socket) in output_sockets.iter() {
                        // Skip blending if channel is not selected
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        if let Some(background) = last_socket.get(channel) {
                            step += 1;

                            let blend_res = self.blend_resource(layer, *channel);

                            use_points
                                .entry(resource.clone())
                                .and_modify(|e| e.last = step)
                                .or_insert(UsePoint {
                                    last: step,
                                    creation: usize::MIN,
                                });
                            use_points
                                .entry(background.socket_node())
                                .and_modify(|e| e.last = step)
                                .or_insert(UsePoint {
                                    last: step,
                                    creation: usize::MIN,
                                });

                            linearization.push(Instruction::Move(
                                background.clone(),
                                blend_res.node_socket("background"),
                            ));
                            linearization.push(Instruction::Move(
                                resource.node_socket(socket),
                                blend_res.node_socket("foreground"),
                            ));
                            linearization.push(Instruction::Execute(
                                blend_res.clone(),
                                AtomicOperator::Blend(blend_options.blend_operator()),
                            ));

                            use_points
                                .entry(blend_res.clone())
                                .and_modify(|e| e.creation = step)
                                .or_insert(UsePoint {
                                    last: usize::MAX,
                                    creation: step,
                                });

                            last_socket.insert(*channel, blend_res.node_socket("color"));
                        } else {
                            last_socket.insert(*channel, resource.node_socket(socket));
                        }
                    }
                }
                Layer::FxLayer(
                    _,
                    FxLayer {
                        operator,
                        input_sockets,
                        output_sockets,
                        blend_options,
                        ..
                    },
                ) => {
                    // Skip if disabled
                    if !blend_options.enabled {
                        continue;
                    }

                    step += 1;

                    let resource = self.layer_resource(layer);

                    match operator {
                        Operator::AtomicOperator(aop) => {
                            // Move inputs
                            for (socket, channel) in input_sockets.iter() {
                                let input_resource = last_socket
                                    .get(channel)
                                    .expect("Missing layer underneath FX")
                                    .clone();
                                use_points
                                    .entry(input_resource.socket_node())
                                    .and_modify(|e| e.last = step)
                                    .or_insert(UsePoint {
                                        last: step,
                                        creation: usize::MIN,
                                    });

                                linearization.push(Instruction::Move(
                                    input_resource,
                                    resource.node_socket(socket),
                                ));
                            }
                            linearization.push(Instruction::Execute(resource.clone(), aop.clone()));
                        }
                        Operator::ComplexOperator(cop) => {
                            // Copy inputs to internal sockets
                            for (socket, channel) in input_sockets.iter() {
                                let input =
                                    cop.inputs.get(socket).expect("Missing internal socket");
                                let input_resource = last_socket
                                    .get(channel)
                                    .expect("Missing layer underneath FX")
                                    .clone();
                                use_points
                                    .entry(input_resource.socket_node())
                                    .and_modify(|e| e.last = step)
                                    .or_insert(UsePoint {
                                        last: step,
                                        creation: usize::MIN,
                                    });

                                linearization.push(Instruction::Copy(
                                    input_resource,
                                    input.1.node_socket("data"),
                                ));
                            }

                            // Call complex operator execution
                            linearization.push(Instruction::Call(resource.clone(), cop.clone()));

                            // Copy back outputs
                            for (out_socket, (_, output)) in cop.outputs.iter() {
                                linearization.push(Instruction::Copy(
                                    output.node_socket("data"),
                                    resource.node_socket(out_socket),
                                ))
                            }
                        }
                    }

                    use_points
                        .entry(resource.clone())
                        .and_modify(|e| e.creation = step)
                        .or_insert(UsePoint {
                            last: usize::MAX,
                            creation: step,
                        });

                    for (channel, socket) in output_sockets.iter() {
                        // Skip blending if channel is not selected
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        let blend_res = self.blend_resource(layer, *channel);

                        if let Some(background) = last_socket.get(channel).cloned() {
                            step += 1;

                            use_points
                                .entry(background.socket_node())
                                .and_modify(|e| e.last = step)
                                .or_insert(UsePoint {
                                    last: step,
                                    creation: usize::MIN,
                                });
                            use_points
                                .entry(resource.clone())
                                .and_modify(|e| e.last = step)
                                .or_insert(UsePoint {
                                    last: step,
                                    creation: usize::MIN,
                                });

                            linearization.push(Instruction::Move(
                                background,
                                blend_res.node_socket("background"),
                            ));
                            linearization.push(Instruction::Move(
                                resource.node_socket(socket),
                                blend_res.node_socket("foreground"),
                            ));
                            linearization.push(Instruction::Execute(
                                blend_res.clone(),
                                AtomicOperator::Blend(blend_options.blend_operator()),
                            ));

                            use_points
                                .entry(blend_res.clone())
                                .and_modify(|e| e.creation = step)
                                .or_insert(UsePoint {
                                    last: usize::MAX,
                                    creation: step,
                                });

                            last_socket.insert(*channel, blend_res.node_socket("color"));
                        } else {
                            last_socket.insert(*channel, resource.node_socket(socket));
                        }
                    }
                }
            }
        }

        // Finally process the (virtual) output operators
        for channel in MaterialChannel::iter() {
            step += 1;

            let output = self.output_resource(channel);
            if let Some(socket) = last_socket.get(&channel).cloned() {
                use_points
                    .entry(socket.socket_node())
                    .and_modify(|e| e.last = step)
                    .or_insert(UsePoint {
                        last: step,
                        creation: usize::MIN,
                    });

                linearization.push(Instruction::Move(socket, output.node_socket("data")));
                linearization.push(Instruction::Execute(
                    output,
                    AtomicOperator::Output(Output {
                        output_type: channel.to_output_type(),
                    }),
                ));
            }
        }

        Some((linearization, use_points.drain().collect()))
    }

    fn parameter_change(
        &mut self,
        resource: &Resource<Param>,
        data: &[u8],
    ) -> Result<Option<Lang>, String> {
        let field = resource.fragment().unwrap();

        if let Some(idx) = self
            .resources
            .get(resource.parameter_node().file().unwrap())
        {
            match &mut self.layers[*idx] {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        fill: Fill { operator, .. },
                        ..
                    },
                ) => operator.set_parameter(field, data),
                Layer::FxLayer(_, layer) => layer.operator.set_parameter(field, data),
            }
        }

        Ok(None)
    }

    fn update_complex_operators(
        &mut self,
        parent_size: u32,
        graph: &Resource<Graph>,
        new: &ComplexOperator,
    ) -> (Vec<super::ComplexOperatorUpdate>, Vec<GraphEvent>) {
        let mut updated = Vec::new();

        for layer in self.layers.iter_mut() {
            let complex = match layer {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        fill:
                            Fill {
                                operator: Operator::ComplexOperator(co),
                                ..
                            },
                        ..
                    },
                ) if &co.graph == graph => co,
                Layer::FxLayer(
                    _,
                    FxLayer {
                        operator: Operator::ComplexOperator(co),
                        ..
                    },
                ) if &co.graph == graph => co,
                _ => continue,
            };

            complex.graph = new.graph.clone();
            complex.title = new.title.clone();
            complex.inputs = new.inputs.clone();
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
            updated.push((layer.to_owned(), params));
            // TODO: find a way to avoid this clone
        }

        (
            updated
                .drain(0..)
                .map(|(l, p)| (self.layer_resource(&l), p))
                .collect(),
            vec![],
        )
    }

    fn resize_all(&mut self, parent_size: u32) -> Vec<Lang> {
        self.all_resources()
            .drain(0..)
            .map(|res| Lang::GraphEvent(GraphEvent::NodeResized(res, parent_size)))
            .collect()
    }

    fn rebuild_events(&self, parent_size: u32) -> Vec<Lang> {
        self.layers
            .iter()
            .map(|layer| {
                let mut evs = Vec::new();

                let res = self.layer_resource(layer);

                let mut sockets = self.layer_sockets(&res);
                let mut blend_sockets = self.blend_sockets(&res);

                evs.push(Lang::LayersEvent(LayersEvent::LayerPushed(
                    res,
                    layer.layer_type(),
                    layer.title().to_owned(),
                    layer.operator().clone(),
                    ParamBoxDescription::empty(),
                    parent_size,
                )));
                evs.extend(sockets.drain(0..).map(|(s, t, e)| {
                    Lang::GraphEvent(GraphEvent::OutputSocketAdded(s, t, e, parent_size))
                }));
                evs.extend(blend_sockets.drain(0..).map(|(s, t)| {
                    Lang::GraphEvent(GraphEvent::OutputSocketAdded(s, t, false, parent_size))
                }));

                evs
            })
            .flatten()
            .collect()
    }

    fn element_param_box(&self, element: &Resource<Node>) -> ParamBoxDescription<MessageWriters> {
        if let Some(idx) = self.resources.get(element.file().unwrap()) {
            match &self.layers[*idx] {
                Layer::FillLayer(_, l) => ParamBoxDescription::fill_layer_parameters(
                    &l.fill.operator,
                    &l.fill.output_sockets,
                )
                .map_transmitters(|t| t.clone().into()),
                Layer::FxLayer(_, l) => ParamBoxDescription::fx_layer_parameters(&l.operator)
                    .map_transmitters(|t| t.clone().into()),
            }
        } else {
            ParamBoxDescription::empty()
        }
    }
}
