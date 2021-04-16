use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use strum_macros::*;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(
    AsBytes,
    Clone,
    Copy,
    Debug,
    EnumIter,
    EnumVariantNames,
    EnumString,
    Serialize,
    Deserialize,
    PartialEq,
)]
#[strum(serialize_all = "kebab-case")]
pub enum AmbientOcclusionQuality {
    LowQuality = 0,
    MidQuality = 1,
    HighQuality = 2,
    UltraQuality = 3,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct AmbientOcclusion {
    pub quality: AmbientOcclusionQuality,
    pub jitter: ParameterBool,
    pub radius: f32,
    pub falloff: f32,
    pub depth: f32,
    pub albedo: f32,
}

impl Default for AmbientOcclusion {
    fn default() -> Self {
        Self {
            quality: AmbientOcclusionQuality::LowQuality,
            jitter: 1,
            radius: 0.01,
            falloff: 0.5,
            depth: 1.,
            albedo: 0.5,
        }
    }
}

impl Socketed for AmbientOcclusion {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "height".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "ao".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "ambient_occlusion"
    }

    fn title(&self) -> &str {
        "Ambient Occlusion"
    }
}

impl Shader for AmbientOcclusion {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("ambient_occlusion"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("height"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("ao"),
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

impl OperatorParamBox for AmbientOcclusion {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "quality".to_string(),
                        transmitter: Field(AmbientOcclusion::QUALITY.to_string()),
                        control: Control::Enum {
                            selected: self.quality as usize,
                            variants: AmbientOcclusionQuality::VARIANTS
                                .iter()
                                .map(|x| x.to_string())
                                .collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "jitter".to_string(),
                        transmitter: Field(AmbientOcclusion::JITTER.to_string()),
                        control: Control::Toggle {
                            def: self.jitter == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "radius".to_string(),
                        transmitter: Field(AmbientOcclusion::RADIUS.to_string()),
                        control: Control::Slider {
                            value: self.radius,
                            min: 0.,
                            max: 0.2,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "falloff".to_string(),
                        transmitter: Field(AmbientOcclusion::FALLOFF.to_string()),
                        control: Control::Slider {
                            value: self.falloff,
                            min: 0.,
                            max: 1.0,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "depth".to_string(),
                        transmitter: Field(AmbientOcclusion::DEPTH.to_string()),
                        control: Control::Slider {
                            value: self.depth,
                            min: 0.,
                            max: 4.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "albedo".to_string(),
                        transmitter: Field(AmbientOcclusion::ALBEDO.to_string()),
                        control: Control::Slider {
                            value: self.albedo,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                ],
            }],
        }
    }
}
