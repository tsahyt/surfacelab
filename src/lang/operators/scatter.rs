use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Scatter {
    scale: i32,
    randomness: f32,
    random_rot: f32,
    random_scale: f32,
}

impl Default for Scatter {
    fn default() -> Self {
        Self {
            scale: 8,
            randomness: 0.5,
            random_rot: 1.,
            random_scale: 1.,
        }
    }
}

impl Socketed for Scatter {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "pattern".to_string() => (OperatorType::Polymorphic(0), false),
            "probability".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), true),
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
                    descriptor: OperatorDescriptorUse::InputImage("pattern"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::InputImage("probability"),
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 5,
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
                        name: "random-scale".to_string(),
                        transmitter: Field(Scatter::RANDOM_SCALE.to_string()),
                        control: Control::Slider {
                            value: self.random_scale,
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
