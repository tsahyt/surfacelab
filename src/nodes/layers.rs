use crate::lang::*;
use enumset::EnumSet;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FillLayer {
    fill: Fill,
    blend_options: LayerBlendOptions,
}

/// A type encoding a function from material channels to sockets.
type ChannelMap = HashMap<MaterialChannel, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Fill {
    /// A fill layer using static images for each material channel
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

/// A layer is either a fill layer or an FX layer. Each layer has a name, such
/// that it can be referenced via a Resource. The resource type for a layer is
/// `Resource<Node>`.
pub enum Layer {
    FillLayer(String, FillLayer),
    FxLayer(String, FxLayer),
}

pub struct LayerStack {
    name: String,
    layers: Vec<Layer>,
    resources: HashMap<String, usize>,
}

impl LayerStack {
    pub fn new(name: &str) -> Self {
        LayerStack {
            name: name.to_owned(),
            layers: Vec::new(),
            resources: HashMap::new(),
        }
    }

    pub fn layer_resource(&self, layer: &str) -> Resource<Node> {
        Resource::node(&format!("{}/{}", self.name, layer), None)
    }

    fn blend_resource(&self, layer: &str) -> Resource<Node> {
        Resource::node(&format!("{}/{}.blend", self.name, layer), None)
    }

    /// Linearize this layer stack into a vector of instructions to be
    /// interpreted by the compute backend. Analogous to the similarly named
    /// function in the NodeGraph.
    pub fn linearize(&self) -> (Linearization, LastUses) {
        let mut linearization = Vec::new();
        let mut last_use = Vec::new();

        let mut last_socket: HashMap<MaterialChannel, Resource<Socket>> = HashMap::new();

        for layer in self.layers.iter() {
            match layer {
                Layer::FillLayer(
                    name,
                    FillLayer {
                        blend_options,
                        fill: Fill::Material(mat),
                    },
                ) => {
                    for (channel, img) in mat.iter() {
                        // We can skip execution entirely if the material channel is disabled
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        let resource = self.layer_resource(name);
                        linearization.push(Instruction::Execute(
                            resource.clone(),
                            AtomicOperator::Image(img.clone()),
                        ));

                        if let Some(background) = last_socket.get(channel) {
                            let blend_res = self.blend_resource(name);

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

                            last_socket.insert(*channel, blend_res.node_socket("color"));
                        } else {
                            last_socket.insert(*channel, resource.node_socket("data"));
                        }
                    }
                }
                Layer::FillLayer(
                    name,
                    FillLayer {
                        blend_options,
                        fill:
                            Fill::Operator {
                                operator,
                                output_sockets,
                            },
                    },
                ) => {
                    // Skip execution if no channels are blended.
                    if blend_options.channels.is_empty() {
                        continue;
                    }

                    let resource = self.layer_resource(name);
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
                            let blend_res = self.blend_resource(name);

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

                            last_socket.insert(*channel, blend_res.node_socket("color"));
                        } else {
                            last_socket.insert(*channel, resource.node_socket(socket));
                        }
                    }
                }
                Layer::FxLayer(
                    name,
                    FxLayer {
                        operator,
                        input_sockets,
                        output_sockets,
                        blend_options,
                    },
                ) => {
                    let resource = self.layer_resource(name);
                    match operator {
                        Operator::AtomicOperator(aop) => {
                            // Move inputs
                            for (channel, socket) in input_sockets.iter() {
                                linearization.push(Instruction::Move(
                                    last_socket
                                        .get(channel)
                                        .expect("Missing layer underneath FX")
                                        .clone(),
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
                                linearization.push(Instruction::Copy(
                                    last_socket
                                        .get(channel)
                                        .expect("Missing layer underneath FX")
                                        .clone(),
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

                        let blend_res = self.blend_resource(name);

                        linearization.push(Instruction::Move(
                            last_socket
                                .get(channel)
                                .expect("Missing layer underneath FX")
                                .clone(),
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

        (linearization, last_use)
    }

    fn push(&mut self, layer: Layer, resource: Resource<Node>) {
        self.layers.push(layer);
        self.resources
            .insert(resource.file().unwrap().to_owned(), self.layers.len() - 1);
    }

    pub fn push_fill(&mut self, layer: FillLayer, resource: Resource<Node>) {
        let layer = Layer::FillLayer(resource.file().unwrap().to_owned(), layer);
        self.push(layer, resource);
    }

    pub fn push_fx(&mut self, layer: FxLayer, resource: Resource<Node>) {
        let layer = Layer::FxLayer(resource.file().unwrap().to_owned(), layer);
        self.push(layer, resource);
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

    pub fn parameter_change(&mut self, resource: &Resource<Param>, data: &[u8]) {
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
    }
}
