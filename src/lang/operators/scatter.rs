use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use num_enum::TryFromPrimitive;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use strum_macros::*;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(u32)]
#[derive(
    AsBytes,
    Clone,
    Copy,
    Debug,
    EnumIter,
    EnumVariantNames,
    EnumString,
    Serialize,
    Deserialize,
    PartialEq,
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab_case")]
pub enum EdgeMode {
    Clamp = 0,
    Tile = 1,
    Solid = 2,
}

#[repr(u32)]
#[derive(
    AsBytes,
    Clone,
    Copy,
    Debug,
    EnumIter,
    EnumVariantNames,
    EnumString,
    Serialize,
    Deserialize,
    PartialEq,
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab_case")]
pub enum BlendMode {
    Add = 0,
    Max = 1,
    Min = 2,
}

impl BlendMode {
    pub fn has_adjust_levels(self) -> bool {
        matches!(self, Self::Add)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Scatter {
    edge_mode: EdgeMode,
    blend_mode: BlendMode,
    adjust_levels: ParameterBool,
    supersample: ParameterBool,
    scale: i32,
    size: f32,
    intensity: f32,
    density: f32,
    randomness: f32,
    random_rot: f32,
    random_size: f32,
    random_intensity: f32,
}

impl Default for Scatter {
    fn default() -> Self {
        Self {
            edge_mode: EdgeMode::Clamp,
            blend_mode: BlendMode::Max,
            adjust_levels: 1,
            supersample: 0,
            scale: 8,
            size: 1.,
            intensity: 1.,
            density: 1.,
            randomness: 0.5,
            random_rot: 1.,
            random_size: 0.,
            random_intensity: 1.,
        }
    }
}

impl Socketed for Scatter {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "image".to_string() => (OperatorType::Polymorphic(0), false),
            "probability".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), true),
            "size".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), true),
            "intensity".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), true),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "out".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn default_name(&self) -> &str {
        "scatter"
    }

    fn title(&self) -> &str {
        "Scatter"
    }
}

impl Shader for Scatter {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("scatter"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::Occupancy,
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("image"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::InputImage("probability"),
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::InputImage("size"),
                },
                OperatorDescriptor {
                    binding: 5,
                    descriptor: OperatorDescriptorUse::InputImage("intensity"),
                },
                OperatorDescriptor {
                    binding: 6,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 7,
                    descriptor: OperatorDescriptorUse::OutputImage("out"),
                },
            ],
            specialization: Specialization::default(),
            shape: OperatorShape::PerPixel {
                local_x: 8,
                local_y: 8,
            },
        })]
    }
}

impl OperatorParamBox for Scatter {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("rgb".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "edge-mode".to_string(),
                        transmitter: Field(Scatter::EDGE_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.edge_mode as usize,
                            variants: EdgeMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "blend-mode".to_string(),
                        transmitter: Field(Scatter::BLEND_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.blend_mode as usize,
                            variants: BlendMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "adjust-levels".to_string(),
                        transmitter: Field(Scatter::ADJUST_LEVELS.to_string()),
                        control: Control::Toggle {
                            def: self.adjust_levels == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "blend-mode",
                            |t: BlendMode| t.has_adjust_levels(),
                        ),
                        presetable: true,
                    },
                    Parameter {
                        name: "supersample".to_string(),
                        transmitter: Field(Scatter::SUPERSAMPLE.to_string()),
                        control: Control::Toggle {
                            def: self.supersample == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "scale".to_string(),
                        transmitter: Field(Scatter::SCALE.to_string()),
                        control: Control::DiscreteSlider {
                            value: self.scale,
                            min: 1,
                            max: 128,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "size".to_string(),
                        transmitter: Field(Scatter::SIZE.to_string()),
                        control: Control::Slider {
                            value: self.density,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "intensity".to_string(),
                        transmitter: Field(Scatter::INTENSITY.to_string()),
                        control: Control::Slider {
                            value: self.density,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "density".to_string(),
                        transmitter: Field(Scatter::DENSITY.to_string()),
                        control: Control::Slider {
                            value: self.density,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "randomness".to_string(),
                        transmitter: Field(Scatter::RANDOMNESS.to_string()),
                        control: Control::Slider {
                            value: self.randomness,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "random-rot".to_string(),
                        transmitter: Field(Scatter::RANDOM_ROT.to_string()),
                        control: Control::Slider {
                            value: self.random_rot,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "random-size".to_string(),
                        transmitter: Field(Scatter::RANDOM_SIZE.to_string()),
                        control: Control::Slider {
                            value: self.random_size,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "random-intensity".to_string(),
                        transmitter: Field(Scatter::RANDOM_INTENSITY.to_string()),
                        control: Control::Slider {
                            value: self.density,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                ],
            }],
        }
    }
}
