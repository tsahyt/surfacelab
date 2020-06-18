use super::parameters::*;
use super::socketed::*;
use crate::compute::shaders::{OperatorShader, Shader};
use crate::ui::param_box::*;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use surfacelab_derive::*;

use std::rc::Rc;
use std::cell::RefCell;

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

#[derive(Clone, Debug, Serialize, Deserialize, Parameters)]
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
    fn param_box(&self, res: Rc<RefCell<super::Resource>>) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Image Path",
                    transmitter: Field(Self::PATH),
                    control: Control::File {
                        selected: Some(self.path.to_owned()),
                    },
                }],
            }],
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parameters)]
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
}

impl Shader for Output {
    fn operator_shader(&self) -> Option<OperatorShader> {
        None
    }
}

impl OperatorParamBox for Output {
    fn param_box(&self, res: Rc<RefCell<super::Resource>>) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Output Type",
                    transmitter: Field(Self::OUTPUT_TYPE),
                    control: Control::Enum {
                        selected: self.output_type as usize,
                        variants: super::OutputType::VARIANTS,
                    },
                }],
            }],
        })
    }
}
