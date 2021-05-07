use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use num_enum::UnsafeFromPrimitive;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use strum_macros::*;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(u32)]
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
    UnsafeFromPrimitive,
)]
#[strum(serialize_all = "kebab_case")]
pub enum DistanceMetric {
    Euclidean = 0,
    Manhattan = 1,
    Chebyshev = 2,
    Minkowski = 3,
}

impl DistanceMetric {
    pub fn has_exponent(self) -> bool {
        matches!(self, Self::Minkowski)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Voronoi {
    metric: DistanceMetric,
    exponent: f32,
    scale: f32,
    octaves: f32,
    roughness: f32,
    randomness: f32,
}

impl Default for Voronoi {
    fn default() -> Self {
        Self {
            metric: DistanceMetric::Euclidean,
            exponent: 1.,
            scale: 3.0,
            octaves: 2.0,
            roughness: 0.5,
            randomness: 1.,
        }
    }
}

impl Socketed for Voronoi {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! { "noise".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "voronoi"
    }

    fn title(&self) -> &str {
        "Voronoi"
    }
}

impl Shader for Voronoi {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("voronoi"),
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

impl OperatorParamBox for Voronoi {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("voronoi".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "metric".to_string(),
                        transmitter: Field(Voronoi::METRIC.to_string()),
                        control: Control::Enum {
                            selected: self.metric as usize,
                            variants: DistanceMetric::VARIANTS
                                .iter()
                                .map(|x| x.to_string())
                                .collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "exponent".to_string(),
                        transmitter: Field(Voronoi::EXPONENT.to_string()),
                        control: Control::Slider {
                            value: self.scale,
                            min: 0.,
                            max: 16.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter("metric", |c| {
                            if let Control::Enum { selected, .. } = c {
                                unsafe { DistanceMetric::from_unchecked(*selected as u32) }
                                    .has_exponent()
                            } else {
                                false
                            }
                        }),
                        presetable: true,
                    },
                    Parameter {
                        name: "scale".to_string(),
                        transmitter: Field(Voronoi::SCALE.to_string()),
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
                        transmitter: Field(Voronoi::OCTAVES.to_string()),
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
                        transmitter: Field(Voronoi::ROUGHNESS.to_string()),
                        control: Control::Slider {
                            value: self.roughness,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "randomness".to_string(),
                        transmitter: Field(Voronoi::RANDOMNESS.to_string()),
                        control: Control::Slider {
                            value: self.randomness,
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
