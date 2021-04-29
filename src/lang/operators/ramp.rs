use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryInto;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes)]
pub struct RampUniforms {
    ramp_data: [[f32; 4]; 64],
    ramp_size: u32,
    ramp_min: f32,
    ramp_max: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Ramp {
    ramp: Vec<[f32; 4]>,
}

impl Default for Ramp {
    fn default() -> Self {
        Self {
            ramp: vec![[0.0; 4], [1.0; 4]],
        }
    }
}

impl Uniforms for Ramp {
    fn uniforms(&self) -> Cow<[u8]> {
        let mut ramp = self.ramp.clone();
        ramp.sort_by(|a, b| a[3].partial_cmp(&b[3]).unwrap_or(std::cmp::Ordering::Equal));
        ramp.resize_with(64, || [0.0; 4]);

        let uniforms = RampUniforms {
            ramp_data: ramp.try_into().unwrap(),
            ramp_size: self.ramp.len() as u32,
            ramp_min: self
                .ramp
                .iter()
                .map(|x| x[3])
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0),
            ramp_max: self
                .ramp
                .iter()
                .map(|x| x[3])
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(1.0),
        };

        Cow::Owned(uniforms.as_bytes().iter().copied().collect())
    }
}

impl Socketed for Ramp {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "factor".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &str {
        "ramp"
    }

    fn title(&self) -> &str {
        "Ramp"
    }
}

impl Shader for Ramp {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("ramp"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("factor"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
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

impl OperatorParamBox for Ramp {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("ramp".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![Parameter {
                    name: "gradient".to_string(),
                    transmitter: Field(Ramp::RAMP.to_string()),
                    control: Control::Ramp {
                        steps: self.ramp.clone(),
                    },
                    expose_status: Some(ExposeStatus::Unexposed),
                    visibility: VisibilityFunction::default(),
                    presetable: true,
                }],
            }],
        }
    }
}
