use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};
use crate::ui::param_box::*;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

use std::cell::RefCell;
use std::rc::Rc;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
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

    fn default_name(&self) -> &'static str {
        "normal_map"
    }

    fn title(&self) -> &'static str {
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
    fn param_box(&self, res: Rc<RefCell<crate::lang::Resource>>) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Strength",
                    transmitter: Field(NormalMap::STRENGTH),
                    control: Control::Slider {
                        value: self.strength,
                        min: 0.,
                        max: 2.,
                    },
                    available: true,
                }],
            }],
        })
    }
}