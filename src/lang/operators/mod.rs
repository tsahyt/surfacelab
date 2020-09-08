use super::parameters::*;
use super::socketed::*;
use crate::compute::shaders::{OperatorShader, Shader};

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use surfacelab_derive::*;

pub mod blend;
pub mod grayscale;
pub mod normal_map;
pub mod perlin_noise;
pub mod ramp;
pub mod rgb;

pub use blend::*;
pub use grayscale::*;
pub use normal_map::*;
pub use perlin_noise::*;
pub use ramp::*;
pub use rgb::*;

#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Image {
    pub path: std::path::PathBuf,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::new(),
        }
    }
}

impl Socketed for Image {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "image".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &'static str {
        "image"
    }

    fn title(&self) -> &'static str {
        "Image"
    }
}

impl Shader for Image {
    fn operator_shader(&self) -> Option<OperatorShader> {
        None
    }
}

impl OperatorParamBox for Image {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "Basic Parameters",
                parameters: vec![Parameter {
                    name: "Image Path".to_string(),
                    transmitter: Field(Self::PATH.to_string()),
                    control: Control::File {
                        selected: if self.path.file_name().is_none() {
                            None
                        } else {
                            Some(self.path.to_owned())
                        },
                    },
                    exposable: true,
                }],
            }],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Output {
    pub output_type: super::OutputType,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            output_type: super::OutputType::default(),
        }
    }
}

impl Socketed for Output {
    // TODO: unify output types with monomorphization, to solve coloring and thumbnail situation
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "data".to_string() => match self.output_type {
                super::OutputType::Albedo => OperatorType::Monomorphic(ImageType::Rgb),
                super::OutputType::Roughness => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Normal => OperatorType::Monomorphic(ImageType::Rgb),
                super::OutputType::Displacement => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Metallic => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Value => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Rgb => OperatorType::Monomorphic(ImageType::Rgb),
        }
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn default_name(&self) -> &'static str {
        "output"
    }

    fn title(&self) -> &'static str {
        "Output"
    }

    fn is_output(&self) -> bool {
        true
    }
}

impl Shader for Output {
    fn operator_shader(&self) -> Option<OperatorShader> {
        None
    }
}

impl OperatorParamBox for Output {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "Basic Parameters",
                parameters: vec![Parameter {
                    name: "Output Type".to_string(),
                    transmitter: Field(Self::OUTPUT_TYPE.to_string()),
                    control: Control::Enum {
                        selected: self.output_type as usize,
                        variants: super::OutputType::VARIANTS
                            .iter()
                            .map(|x| x.to_string())
                            .collect(),
                    },
                    exposable: false,
                }],
            }],
        }
    }
}
