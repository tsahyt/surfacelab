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
pub struct NormalBlend {
    pub mix: f32,
}

impl Default for NormalBlend {
    fn default() -> Self {
        Self { mix: 1.0 }
    }
}

impl Socketed for NormalBlend {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "base".to_string() => (OperatorType::Monomorphic(ImageType::Rgb), false),
            "detail".to_string() => (OperatorType::Monomorphic(ImageType::Rgb), false),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "normal".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &str {
        "normal_blend"
    }

    fn title(&self) -> &str {
        "Normal Blend"
    }
}

impl Shader for NormalBlend {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("normal_blend"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("base"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("detail"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::OutputImage("normal"),
                },
            ],
            specialization: Specialization::default(),
            shape: OperatorShape::PerPixel {
                local_x: 8,
                local_y: 8,
            },
        })]
    }
}

impl OperatorParamBox for NormalBlend {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("normal_blend".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![Parameter {
                    name: "mix".to_string(),
                    transmitter: Field(NormalBlend::MIX.to_string()),
                    control: Control::Slider {
                        value: self.mix,
                        min: 0.,
                        max: 1.,
                    },
                    expose_status: Some(ExposeStatus::Unexposed),
                    visibility: VisibilityFunction::default(),
                    presetable: true,
                }],
            }],
        }
    }
}
