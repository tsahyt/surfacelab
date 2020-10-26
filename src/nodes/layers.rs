use crate::lang::*;
use enumset::EnumSet;
use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::IntoEnumIterator;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillLayer {
    fill: Fill,
    blend_options: LayerBlendOptions,
}

impl FillLayer {
    pub fn from_operator(op: &Operator) -> Self {
        FillLayer {
            fill: Fill::Operator {
                operator: op.clone(),
                output_sockets: HashMap::new(),
            },
            blend_options: LayerBlendOptions::default(),
        }
    }
}

/// A type encoding a function from material channels to sockets.
type ChannelMap = HashMap<MaterialChannel, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Fill {
    /// A fill layer using static images for each material channel
    // TODO: Consider replacing Material fill layer with a complex operator for materials and standard fill
    Material(HashMap<MaterialChannel, Image>),

    /// A fill layer using an operator, with its output sockets mapped to
    /// material channels. The operator must not have any inputs! It can be
    /// complex or atomic. The requirement to not have inputs means that most
    /// atomic operators are not usable, outside of noises etc., and the
    /// operator is most likely complex.
    Operator {
        operator: Operator,
        output_sockets: ChannelMap,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// An FX layer is a layer that uses the material underneath it as input.
/// Therefore FX layers *cannot* be placed at the bottom of a layer stack.
pub struct FxLayer {
    operator: Operator,
    input_sockets: ChannelMap,
    output_sockets: ChannelMap,
    blend_options: LayerBlendOptions,
}

impl FxLayer {
    pub fn from_operator(op: &Operator) -> Self {
        FxLayer {
            operator: op.clone(),
            input_sockets: HashMap::new(),
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
    factor: f32,
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
            mix: self.factor,
            clamp_output: 1,
        }
    }
}

impl Default for LayerBlendOptions {
    fn default() -> Self {
        LayerBlendOptions {
            mask: MaskStack(Vec::new()),
            factor: 1.0,
            channels: EnumSet::all(),
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

    fn output_resource(&self, channel: MaterialChannel) -> Resource<Node> {
        Resource::node(
            &format!("{}/output.{}", self.name, channel.short_name(),),
            None,
        )
    }

    pub fn all_resources(&self) -> Vec<Resource<Node>> {
        self.layers
            .iter()
            .map(|layer| match layer {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        fill: Fill::Material(_),
                        blend_options,
                        ..
                    },
                ) => blend_options
                    .channels
                    .iter()
                    .map(|channel| self.blend_resource(layer, channel))
                    .chain(
                        MaterialChannel::iter()
                            .map(|channel| self.material_layer_resource(layer, channel)),
                    )
                    .collect::<Vec<_>>(),
                Layer::FillLayer(
                    _,
                    FillLayer {
                        fill: Fill::Operator { .. },
                        blend_options,
                        ..
                    },
                ) => blend_options
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
        hashmap! {
            "albedo".to_string() =>
                (OperatorType::Monomorphic(ImageType::Rgb),
                 self.output_resource(MaterialChannel::Albedo)),
            "roughness".to_string() =>
                (OperatorType::Monomorphic(ImageType::Grayscale),
                 self.output_resource(MaterialChannel::Roughness)),
            "normal".to_string() =>
                (OperatorType::Monomorphic(ImageType::Rgb),
                 self.output_resource(MaterialChannel::Normal)),
            "displacement".to_string() =>
                (OperatorType::Monomorphic(ImageType::Grayscale),
                 self.output_resource(MaterialChannel::Displacement)),
            "metallic".to_string() =>
                (OperatorType::Monomorphic(ImageType::Grayscale),
                 self.output_resource(MaterialChannel::Metallic)),
        }
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
    fn linearize(&self, _mode: super::LinearizationMode) -> Option<(Linearization, LastUses)> {
        let mut linearization = Vec::new();
        let mut last_use: HashMap<Resource<Node>, usize> = HashMap::new();
        let mut step = 0;

        let mut last_socket: HashMap<MaterialChannel, Resource<Socket>> = HashMap::new();

        for layer in self.layers.iter() {
            match layer {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        blend_options,
                        fill: Fill::Material(mat),
                    },
                ) => {
                    // Skip if disabled
                    if !blend_options.enabled {
                        continue;
                    }

                    for (channel, img) in mat.iter() {
                        // We can skip execution entirely if the material channel is disabled
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        step += 1;

                        let resource = self.material_layer_resource(layer, *channel);
                        linearization.push(Instruction::Execute(
                            resource.clone(),
                            AtomicOperator::Image(img.clone()),
                        ));

                        if let Some(background) = last_socket.get(channel) {
                            step += 1;

                            let blend_res = self.blend_resource(layer, *channel);

                            linearization.push(Instruction::Move(
                                background.clone(),
                                blend_res.node_socket("background"),
                            ));
                            linearization.push(Instruction::Move(
                                resource.node_socket("data"),
                                blend_res.node_socket("foreground"),
                            ));

                            linearization.push(Instruction::Execute(
                                blend_res.clone(),
                                AtomicOperator::Blend(blend_options.blend_operator()),
                            ));

                            last_use.insert(background.socket_node(), step);
                            last_socket.insert(*channel, blend_res.node_socket("color"));
                        } else {
                            last_socket.insert(*channel, resource.node_socket("data"));
                        }
                    }
                }
                Layer::FillLayer(
                    _,
                    FillLayer {
                        blend_options,
                        fill:
                            Fill::Operator {
                                operator,
                                output_sockets,
                            },
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

                    for (channel, socket) in output_sockets.iter() {
                        // Skip blending if channel is not selected
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        if let Some(background) = last_socket.get(channel) {
                            step += 1;

                            let blend_res = self.blend_resource(layer, *channel);

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

                            last_use.insert(background.socket_node(), step);
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
                            for (channel, socket) in input_sockets.iter() {
                                let input_resource = last_socket
                                    .get(channel)
                                    .expect("Missing layer underneath FX")
                                    .clone();
                                last_use.insert(input_resource.socket_node(), step);

                                linearization.push(Instruction::Move(
                                    input_resource,
                                    resource.node_socket(socket),
                                ));
                            }
                            linearization.push(Instruction::Execute(resource.clone(), aop.clone()));
                        }
                        Operator::ComplexOperator(cop) => {
                            // Copy inputs to internal sockets
                            for (channel, socket) in input_sockets.iter() {
                                let input =
                                    cop.inputs.get(socket).expect("Missing internal socket");
                                let input_resource = last_socket
                                    .get(channel)
                                    .expect("Missing layer underneath FX")
                                    .clone();
                                last_use.insert(input_resource.socket_node(), step);

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

                    for (channel, socket) in output_sockets.iter() {
                        // Skip blending if channel is not selected
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        step += 1;

                        let blend_res = self.blend_resource(layer, *channel);
                        let background = last_socket
                            .get(channel)
                            .expect("Missing layer underneath FX")
                            .clone();

                        last_use.insert(background.socket_node(), step);

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

                        last_socket.insert(*channel, blend_res.node_socket("color"));
                    }
                }
            }
        }

        // Finally process the (virtual) output operators
        for channel in MaterialChannel::iter() {
            step += 1;

            let output = self.output_resource(channel);
            if let Some(socket) = last_socket.get(&channel).cloned() {
                last_use.insert(socket.socket_node(), step);

                linearization.push(Instruction::Move(socket, output.node_socket("data")));
                linearization.push(Instruction::Execute(
                    output,
                    AtomicOperator::Output(Output {
                        output_type: channel.to_output_type(),
                    }),
                ));
            }
        }

        Some((linearization, last_use.drain().collect()))
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
                        fill: Fill::Operator { operator, .. },
                        ..
                    },
                ) => operator.set_parameter(field, data),
                Layer::FillLayer(
                    _,
                    FillLayer {
                        fill: Fill::Material(mat),
                        ..
                    },
                ) => {
                    let file = resource.file().unwrap();
                    match &file[file.len() - 3..] {
                        "col" => mat
                            .get_mut(&MaterialChannel::Albedo)
                            .unwrap()
                            .set_parameter(field, data),
                        "dsp" => mat
                            .get_mut(&MaterialChannel::Displacement)
                            .unwrap()
                            .set_parameter(field, data),
                        "nor" => mat
                            .get_mut(&MaterialChannel::Normal)
                            .unwrap()
                            .set_parameter(field, data),
                        "rgh" => mat
                            .get_mut(&MaterialChannel::Roughness)
                            .unwrap()
                            .set_parameter(field, data),
                        "met" => mat
                            .get_mut(&MaterialChannel::Metallic)
                            .unwrap()
                            .set_parameter(field, data),
                        _ => panic!("Invalid material resource"),
                    }
                }
                Layer::FxLayer(_, layer) => layer.operator.set_parameter(field, data),
            }
        }

        Ok(None)
    }

    fn update_complex_operators(
        &mut self,
        graph: &Resource<Graph>,
        new: &ComplexOperator,
    ) -> Vec<(Resource<Node>, HashMap<String, ParamSubstitution>)> {
        let mut updated = Vec::new();

        for layer in self.layers.iter_mut() {
            let complex = match layer {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        fill:
                            Fill::Operator {
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

        updated
            .drain(0..)
            .map(|(l, p)| (self.layer_resource(&l), p))
            .collect()
    }

    fn resize_all(&mut self, parent_size: u32) -> Vec<Lang> {
        self.all_resources()
            .drain(0..)
            .map(|res| Lang::GraphEvent(GraphEvent::NodeResized(res, parent_size)))
            .collect()
    }

    fn rebuild_events(&self, parent_size: u32) -> Vec<Lang> {
        todo!()
    }
}
