use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

use std::cell::RefCell;
use std::rc::Rc;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
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
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../../shaders/rgb.spv"),
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
        })
    }
}

impl OperatorParamBox for Rgb {
    fn param_box_description(
        &self,
        res: Rc<RefCell<crate::lang::Resource>>,
    ) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: vec![ParamCategory {
                name: "Basic Parameters",
                parameters: vec![Parameter {
                    name: "Color",
                    transmitter: Field(Rgb::RGB),
                    control: Control::RgbColor { value: self.rgb },
                    available: true,
                }],
            }],
        }
    }
}
