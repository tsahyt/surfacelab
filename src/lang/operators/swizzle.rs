use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};
use crate::lang::ImageChannel;
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Swizzle {
    pub channel_r: ImageChannel,
    pub channel_g: ImageChannel,
    pub channel_b: ImageChannel,
}

impl Default for Swizzle {
    fn default() -> Self {
        Self {
            channel_r: ImageChannel::R,
            channel_g: ImageChannel::G,
            channel_b: ImageChannel::B,
        }
    }
}

impl Socketed for Swizzle {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color_in".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color_out".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn default_name(&self) -> &str {
        "swizzle"
    }

    fn title(&self) -> &str {
        "Swizzle"
    }
}

impl Shader for Swizzle {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: shader!("swizzle"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("color_in"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("color_out"),
                },
            ],
        })
    }
}

impl OperatorParamBox for Swizzle {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        let channels = vec!["R".to_string(), "G".to_string(), "B".to_string()];

        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "red-channel".to_string(),
                        transmitter: Field(Swizzle::CHANNEL_R.to_string()),
                        control: Control::Enum {
                            selected: self.channel_r as usize,
                            variants: channels.clone(),
                        },
                        expose_status: None,
                    },
                    Parameter {
                        name: "green-channel".to_string(),
                        transmitter: Field(Swizzle::CHANNEL_G.to_string()),
                        control: Control::Enum {
                            selected: self.channel_g as usize,
                            variants: channels.clone(),
                        },
                        expose_status: None,
                    },
                    Parameter {
                        name: "blue-channel".to_string(),
                        transmitter: Field(Swizzle::CHANNEL_B.to_string()),
                        control: Control::Enum {
                            selected: self.channel_b as usize,
                            variants: channels.clone(),
                        },
                        expose_status: None,
                    },
                ],
            }],
        }
    }
}
