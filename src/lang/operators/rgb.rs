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
pub struct Rgb {
    pub rgb: [f32; 3],
}

impl Default for Rgb {
    fn default() -> Self {
        Self {
            rgb: [0.5, 0.7, 0.3],
        }
    }
}

impl Socketed for Rgb {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn default_name(&self) -> &str {
        "rgb"
    }

    fn title(&self) -> &str {
        "RGB Color"
    }
}

impl Shader for Rgb {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("rgb"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
                },
            ],
        })]
    }
}

impl OperatorParamBox for Rgb {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![Parameter {
                    name: "color".to_string(),
                    transmitter: Field(Rgb::RGB.to_string()),
                    control: Control::RgbColor { value: self.rgb },
                    expose_status: Some(ExposeStatus::Unexposed),
                }],
            }],
        }
    }
}
