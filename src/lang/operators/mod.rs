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

pub mod ambient_occlusion;
pub mod blend;
pub mod blur;
pub mod checker;
pub mod color_adjust;
pub mod distance;
pub mod grayscale;
pub mod noise_spread;
pub mod normal_blend;
pub mod normal_map;
pub mod perlin_noise;
pub mod ramp;
pub mod range;
pub mod rgb;
pub mod shape;
pub mod split_merge;
pub mod swizzle;
pub mod threshold;
pub mod transform;
pub mod value;
pub mod warp;

pub use ambient_occlusion::*;
pub use blend::*;
pub use blur::*;
pub use checker::*;
pub use color_adjust::*;
pub use distance::*;
pub use grayscale::*;
pub use noise_spread::*;
pub use normal_blend::*;
pub use normal_map::*;
pub use perlin_noise::*;
pub use ramp::*;
pub use range::*;
pub use rgb::*;
pub use shape::*;
pub use split_merge::*;
pub use swizzle::*;
pub use threshold::*;
pub use transform::*;
pub use value::*;
pub use warp::*;

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

    // Image operators are special in sizing and are handled by the compute
    // component. The size requests instructs the node manager to size the node.
    // The compute manager will pick the appropriate size on upload.
    fn size_request(&self) -> Option<u32> {
        Some(1)
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
                is_open: true,
                visibility: VisibilityFunction::default(),
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
#[strum(serialize_all = "kebab_case")]
pub enum OutputType {
    Albedo,
    Roughness,
    Normal,
    Displacement,
    Metallic,
    AmbientOcclusion,
    Value,
    Rgb,
}

impl From<ImageType> for OutputType {
    fn from(t: ImageType) -> Self {
        match t {
            ImageType::Grayscale => OutputType::Value,
            ImageType::Rgb => OutputType::Rgb,
        }
    }
}

impl From<OperatorType> for OutputType {
    fn from(source: OperatorType) -> Self {
        match source {
            OperatorType::Monomorphic(i) => Self::from(i),
            OperatorType::Polymorphic(_) => OutputType::Value,
        }
    }
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
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "data".to_string() => OperatorType::Monomorphic(self.output_type.into())
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
                is_open: true,
                visibility: VisibilityFunction::default(),
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
    pub input_type: ImageType,
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
                is_open: true,
                visibility: VisibilityFunction::default(),
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
