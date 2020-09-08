use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct PerlinNoise {
    pub scale: f32,
    pub octaves: f32,
    pub attenuation: f32,
}

impl Default for PerlinNoise {
    fn default() -> Self {
        Self {
            scale: 3.0,
            octaves: 2.0,
            attenuation: 2.0,
        }
    }
}

impl Socketed for PerlinNoise {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! { "noise".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "perlin_noise"
    }

    fn title(&self) -> &str {
        "Perlin Noise"
    }
}

impl Shader for PerlinNoise {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../../shaders/perlin.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::OutputImage("noise"),
                },
            ],
        })
    }
}

impl OperatorParamBox for PerlinNoise {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "Basic Parameters",
                parameters: vec![
                    Parameter {
                        name: "Scale".to_string(),
                        transmitter: Field(PerlinNoise::SCALE.to_string()),
                        control: Control::Slider {
                            value: self.scale,
                            min: 0.,
                            max: 16.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "Octaves".to_string(),
                        transmitter: Field(PerlinNoise::OCTAVES.to_string()),
                        control: Control::Slider {
                            value: self.octaves as _,
                            min: 0.,
                            max: 16.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "Attenuation".to_string(),
                        transmitter: Field(PerlinNoise::ATTENUATION.to_string()),
                        control: Control::Slider {
                            value: self.attenuation,
                            min: 0.,
                            max: 4.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                ],
            }],
        }
    }
}
