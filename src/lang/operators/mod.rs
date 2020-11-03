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
pub mod range;
pub mod rgb;

pub use blend::*;
pub use grayscale::*;
pub use normal_map::*;
pub use perlin_noise::*;
pub use ramp::*;
pub use range::*;
pub use rgb::*;

#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Image {
    pub path: std::path::PathBuf,
    pub color_space: crate::lang::ColorSpace,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::new(),
            color_space: crate::lang::ColorSpace::Srgb,
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

    fn external_data(&self) -> bool {
        true
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
                parameters: vec![
                    Parameter {
                        name: "Image Path".to_string(),
                        transmitter: Field(Self::PATH.to_string()),
                        control: Control::File {
                            selected: if self.path.file_name().is_none() {
                                None
                            } else {
                                Some(self.path.to_owned())
                            },
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "Color Space".to_string(),
                        transmitter: Field(Self::COLOR_SPACE.to_string()),
                        control: Control::Enum {
                            selected: self.color_space as usize,
                            variants: crate::lang::ColorSpace::VARIANTS
                                .iter()
                                .map(|x| x.to_string())
                                .collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                ],
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
                    expose_status: None,
                }],
            }],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Input {
    input_type: ImageType,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            input_type: ImageType::Grayscale,
        }
    }
}

impl Socketed for Input {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "data".to_string() => OperatorType::Monomorphic(self.input_type)
        }
    }

    fn default_name(&self) -> &str {
        "input"
    }

    fn title(&self) -> &str {
        "Input"
    }

    /// Inputs have external data, since their output sockets take all data from
    /// a copy operation
    fn external_data(&self) -> bool {
        true
    }

    fn is_input(&self) -> bool {
        true
    }
}

impl Shader for Input {
    fn operator_shader(&self) -> Option<OperatorShader> {
        None
    }
}

impl OperatorParamBox for Input {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "Basic Parameters",
                parameters: vec![Parameter {
                    name: "Input Type".to_string(),
                    transmitter: Field(Self::INPUT_TYPE.to_string()),
                    control: Control::Enum {
                        selected: self.input_type as usize,
                        variants: super::ImageType::VARIANTS
                            .iter()
                            .map(|x| x.to_string())
                            .collect(),
                    },
                    expose_status: None,
                }],
            }],
        }
    }
}
