use super::super::parameters::*;
use super::super::socketed::*;
use crate::{compute::shaders::*, lang::resource as r};

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap};
use surfacelab_derive::*;

#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Svg {
    pub resource: Option<r::Resource<r::Svg>>,
}

impl Default for Svg {
    fn default() -> Self {
        Self { resource: None }
    }
}

impl Socketed for Svg {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "image".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &'static str {
        "svg"
    }

    fn title(&self) -> &'static str {
        "SVG"
    }

    fn external_data(&self) -> bool {
        true
    }

    // Svg operators are special in sizing and are handled by the compute
    // component. The size requests instructs the node manager to size the node.
    // The compute manager will pick the appropriate size on upload.
    fn size_request(&self) -> Option<u32> {
        Some(1)
    }
}

/// Svg is special and doesn't have uniforms. Therefore the output is empty
impl Uniforms for Svg {
    fn uniforms(&self) -> Cow<[u8]> {
        Cow::Borrowed(&[])
    }
}

impl Shader for Svg {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![]
    }
}

impl OperatorParamBox for Svg {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("svg".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![Parameter {
                    name: "svg-resource".to_string(),
                    transmitter: Field(Self::RESOURCE.to_string()),
                    control: Control::SvgResource {
                        selected: self.resource.clone(),
                    },
                    expose_status: Some(ExposeStatus::Unexposed),
                    visibility: VisibilityFunction::default(),
                    presetable: false,
                }],
            }],
        }
    }
}
