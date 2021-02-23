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
pub struct Hsv {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub mix: f32,
}

impl Default for Hsv {
    fn default() -> Self {
        Self {
            hue: 0.5,
            saturation: 1.0,
            value: 1.0,
            mix: 1.0,
        }
    }
}

impl Socketed for Hsv {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color_in".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color_out".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn default_name(&self) -> &str {
        "hsv"
    }

    fn title(&self) -> &str {
        "HSV"
    }
}

impl Shader for Hsv {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("hsv"),
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
        })]
    }
}

impl OperatorParamBox for Hsv {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "hue".to_string(),
                        transmitter: Field(Hsv::HUE.to_string()),
                        control: Control::Slider {
                            value: self.hue,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "saturation".to_string(),
                        transmitter: Field(Hsv::SATURATION.to_string()),
                        control: Control::Slider {
                            value: self.saturation,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "value".to_string(),
                        transmitter: Field(Hsv::VALUE.to_string()),
                        control: Control::Slider {
                            value: self.value,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "mix".to_string(),
                        transmitter: Field(Hsv::MIX.to_string()),
                        control: Control::Slider {
                            value: self.mix,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                ],
            }],
        }
    }
}
