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
pub struct PerlinNoise {
    pub scale: f32,
    pub octaves: f32,
    pub roughness: f32,
}

impl Default for PerlinNoise {
    fn default() -> Self {
        Self {
            scale: 3.0,
            octaves: 2.0,
            roughness: 0.5,
        }
    }
}

impl Socketed for PerlinNoise {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! { "noise".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "perlin_noise"
    }

    fn title(&self) -> &str {
        "Perlin Noise"
    }
}

impl Shader for PerlinNoise {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("perlin"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::OutputImage("noise"),
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

impl OperatorParamBox for PerlinNoise {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("perlin_noise".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "scale".to_string(),
                        transmitter: Field(PerlinNoise::SCALE.to_string()),
                        control: Control::Slider {
                            value: self.scale,
                            min: 0.,
                            max: 16.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "octaves".to_string(),
                        transmitter: Field(PerlinNoise::OCTAVES.to_string()),
                        control: Control::Slider {
                            value: self.octaves as _,
                            min: 0.,
                            max: 16.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "roughness".to_string(),
                        transmitter: Field(PerlinNoise::ROUGHNESS.to_string()),
                        control: Control::Slider {
                            value: self.roughness,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                ],
            }],
        }
    }
}
