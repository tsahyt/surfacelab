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
pub struct Checker {
    pub tiling: u32,
    pub rotated: ParameterBool,
    pub inverted: ParameterBool,
}

impl Default for Checker {
    fn default() -> Self {
        Self {
            tiling: 2,
            rotated: 0,
            inverted: 0,
        }
    }
}

impl Socketed for Checker {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "pattern".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "checker"
    }

    fn title(&self) -> &str {
        "Checker"
    }
}

impl Shader for Checker {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("checker"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::OutputImage("pattern"),
                },
            ],
            specialization: Specialization::default(),
        })]
    }
}

impl OperatorParamBox for Checker {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "tiling".to_string(),
                        transmitter: Field(Checker::TILING.to_string()),
                        control: Control::DiscreteSlider {
                            value: self.tiling as i32,
                            min: 1,
                            max: 32,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "rotated".to_string(),
                        transmitter: Field(Checker::ROTATED.to_string()),
                        control: Control::Toggle {
                            def: self.rotated == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "inverted".to_string(),
                        transmitter: Field(Checker::INVERTED.to_string()),
                        control: Control::Toggle {
                            def: self.rotated == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                ],
            }],
        }
    }
}
