use super::parameters::*;
use super::socketed::*;
use crate::{
    compute::shaders::*,
    lang::{Img, Resource},
};

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use strum::VariantNames;
use strum_macros::*;
use surfacelab_derive::*;

pub mod blend;
pub mod blur;
pub mod checker;
pub mod color_adjust;
pub mod distance;
pub mod grayscale;
pub mod normal_map;
pub mod perlin_noise;
pub mod ramp;
pub mod range;
pub mod rgb;
pub mod split_merge;
pub mod swizzle;
pub mod transform;

pub use blend::*;
pub use blur::*;
pub use checker::*;
pub use color_adjust::*;
pub use distance::*;
pub use grayscale::*;
pub use normal_map::*;
pub use perlin_noise::*;
pub use ramp::*;
pub use range::*;
pub use rgb::*;
pub use split_merge::*;
pub use swizzle::*;
pub use transform::*;

/// Image operator to include external images into a node graph
#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Image {
    pub resource: Option<Resource<Img>>,
}

impl Default for Image {
    fn default() -> Self {
        Self { resource: None }
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

/// Image is special and doesn't have uniforms. Therefore the output is empty
impl Uniforms for Image {
    fn uniforms(&self) -> Cow<[u8]> {
        Cow::Borrowed(&[])
    }
}

impl Shader for Image {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![]
    }
}

impl OperatorParamBox for Image {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![Parameter {
                    name: "image-resource".to_string(),
                    transmitter: Field(Self::RESOURCE.to_string()),
                    control: Control::ImageResource {
                        selected: self.resource.clone(),
                    },
                    expose_status: Some(ExposeStatus::Unexposed),
                    visibility: VisibilityFunction::default(),
                }],
            }],
        }
    }
}

/// Types of outputs. Possible values include PBR channels as well as
/// generalized formats.
#[repr(C)]
#[derive(
    PartialEq, Clone, Copy, Debug, EnumIter, EnumVariantNames, EnumString, Serialize, Deserialize,
)]
pub enum OutputType {
    Albedo,
    Roughness,
    Normal,
    Displacement,
    Metallic,
    Value,
    Rgb,
}

impl Default for OutputType {
    fn default() -> Self {
        OutputType::Value
    }
}

/// Output nodes for node graphs are used to gather results. For complex
/// operators they represent the output sockets. Otherwise they are passed to
/// the renderer.
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

/// Output is special and doesn't have uniforms. Therefore the output is empty
impl Uniforms for Output {
    fn uniforms(&self) -> Cow<[u8]> {
        Cow::Borrowed(&[])
    }
}

impl Shader for Output {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![]
    }
}

impl OperatorParamBox for Output {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![Parameter {
                    name: "output-type".to_string(),
                    transmitter: Field(Self::OUTPUT_TYPE.to_string()),
                    control: Control::Enum {
                        selected: self.output_type as usize,
                        variants: super::OutputType::VARIANTS
                            .iter()
                            .map(|x| x.to_string())
                            .collect(),
                    },
                    expose_status: None,
                    visibility: VisibilityFunction::default(),
                }],
            }],
        }
    }
}

/// Input nodes are used to gather inputs for complex operators.
#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Input {
    input_type: ImageType,
}

impl Default for Input {
    /// By default an input is grayscale
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

/// Input is special and doesn't have uniforms. Therefore the output is empty
impl Uniforms for Input {
    fn uniforms(&self) -> Cow<[u8]> {
        Cow::Borrowed(&[])
    }
}

impl Shader for Input {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![]
    }
}

impl OperatorParamBox for Input {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![Parameter {
                    name: "input-type".to_string(),
                    transmitter: Field(Self::INPUT_TYPE.to_string()),
                    control: Control::Enum {
                        selected: self.input_type as usize,
                        variants: super::ImageType::VARIANTS
                            .iter()
                            .map(|x| x.to_string())
                            .collect(),
                    },
                    expose_status: None,
                    visibility: VisibilityFunction::default(),
                }],
            }],
        }
    }
}
