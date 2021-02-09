use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use strum_macros::*;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
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
    ParameterField,
    PartialEq,
)]
pub enum GrayscaleMode {
    Luminance,
    Average,
    Desaturate,
    MaxDecompose,
    MinDecompose,
    RedOnly,
    GreenOnly,
    BlueOnly,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Grayscale {
    pub mode: GrayscaleMode,
}

impl Default for Grayscale {
    fn default() -> Self {
        Self {
            mode: GrayscaleMode::Luminance,
        }
    }
}

impl Socketed for Grayscale {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "value".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "grayscale"
    }

    fn title(&self) -> &str {
        "Grayscale"
    }
}

impl Shader for Grayscale {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: shader!("grayscale"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("color"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("value"),
                },
            ],
        })
    }
}

impl OperatorParamBox for Grayscale {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![Parameter {
                    name: "grayscale-conversion-mode".to_string(),
                    transmitter: Field(Grayscale::MODE.to_string()),
                    control: Control::Enum {
                        selected: self.mode as usize,
                        variants: GrayscaleMode::VARIANTS
                            .iter()
                            .map(|x| x.to_string())
                            .collect(),
                    },
                    expose_status: Some(ExposeStatus::Unexposed),
                }],
            }],
        }
    }
}
