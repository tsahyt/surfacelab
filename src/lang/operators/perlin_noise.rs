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

    fn default_name(&self) -> &'static str {
        "perlin_noise"
    }

    fn title(&self) -> &'static str {
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
    fn param_box(&self, res: Rc<RefCell<crate::lang::Resource>>) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[
                    Parameter {
                        name: "Scale",
                        transmitter: Field(PerlinNoise::SCALE),
                        control: Control::Slider {
                            value: self.scale,
                            min: 0.,
                            max: 16.,
                        },
                        available: true,
                    },
                    Parameter {
                        name: "Octaves",
                        transmitter: Field(PerlinNoise::OCTAVES),
                        control: Control::Slider {
                            value: self.octaves as _,
                            min: 0.,
                            max: 16.,
                        },
                        available: true,
                    },
                    Parameter {
                        name: "Attenuation",
                        transmitter: Field(PerlinNoise::ATTENUATION),
                        control: Control::Slider {
                            value: self.attenuation,
                            min: 0.,
                            max: 4.,
                        },
                        available: true,
                    },
                ],
            }],
        })
    }
}
