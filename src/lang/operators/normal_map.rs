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
pub struct NormalMap {
    pub strength: f32,
}

impl Default for NormalMap {
    fn default() -> Self {
        Self { strength: 1.0 }
    }
}

impl Socketed for NormalMap {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "height".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "normal".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &str {
        "normal_map"
    }

    fn title(&self) -> &str {
        "Normal Map"
    }
}

impl Shader for NormalMap {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../../shaders/normal.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("height"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("normal"),
                },
            ],
        })
    }
}

impl OperatorParamBox for NormalMap {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "Basic Parameters",
                parameters: vec![Parameter {
                    name: "Strength".to_string(),
                    transmitter: Field(NormalMap::STRENGTH.to_string()),
                    control: Control::Slider {
                        value: self.strength,
                        min: 0.,
                        max: 2.,
                    },
                    exposable: true,
                }],
            }],
        }
    }
}
