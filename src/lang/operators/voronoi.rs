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
pub enum Method {
    F1 = 0,
    DistanceToEdge = 1,
    SmoothF1 = 2,
}

impl Method {
    pub fn has_metric(self) -> bool {
        !matches!(self, Self::DistanceToEdge)
    }

    pub fn has_smoothness(self) -> bool {
        matches!(self, Self::SmoothF1)
    }
}

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
pub enum Dimensions {
    TwoD = 0,
    ThreeD = 1,
}

impl Dimensions {
    pub fn has_z(self) -> bool {
        matches!(self, Self::ThreeD)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Voronoi {
    dimensions: Dimensions,
    method: Method,
    metric: DistanceMetric,
    z: f32,
    exponent: f32,
    scale: i32,
    octaves: f32,
    roughness: f32,
    randomness: f32,
    smoothness: f32,
}

impl Default for Voronoi {
    fn default() -> Self {
        Self {
            dimensions: Dimensions::TwoD,
            method: Method::F1,
            metric: DistanceMetric::Euclidean,
            z: 0.,
            exponent: 1.,
            scale: 16,
            octaves: 0.0,
            roughness: 0.5,
            randomness: 1.,
            smoothness: 0.25,
        }
    }
}

impl Socketed for Voronoi {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "distance".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
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
                    descriptor: OperatorDescriptorUse::OutputImage("distance"),
                },
                OperatorDescriptor {
                    binding: 2,
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
                        name: "dimensions".to_string(),
                        transmitter: Field(Voronoi::DIMENSIONS.to_string()),
                        control: Control::Enum {
                            selected: self.dimensions as usize,
                            variants: Dimensions::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "method".to_string(),
                        transmitter: Field(Voronoi::METHOD.to_string()),
                        control: Control::Enum {
                            selected: self.method as usize,
                            variants: Method::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
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
                        visibility: VisibilityFunction::on_parameter("method", |c| {
                            if let Control::Enum { selected, .. } = c {
                                unsafe { Method::from_unchecked(*selected as u32) }.has_metric()
                            } else {
                                false
                            }
                        }),
                        presetable: true,
                    },
                    Parameter {
                        name: "z".to_string(),
                        transmitter: Field(Voronoi::Z.to_string()),
                        control: Control::Slider {
                            value: self.z,
                            min: 0.,
                            max: 16.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter("dimensions", |c| {
                            if let Control::Enum { selected, .. } = c {
                                unsafe { Dimensions::from_unchecked(*selected as u32) }.has_z()
                            } else {
                                false
                            }
                        }),
                        presetable: true,
                    },
                    Parameter {
                        name: "exponent".to_string(),
                        transmitter: Field(Voronoi::EXPONENT.to_string()),
                        control: Control::Slider {
                            value: self.exponent,
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
                        control: Control::DiscreteSlider {
                            value: self.scale,
                            min: 1,
                            max: 256,
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
                    Parameter {
                        name: "smoothness".to_string(),
                        transmitter: Field(Voronoi::SMOOTHNESS.to_string()),
                        control: Control::Slider {
                            value: self.smoothness,
                            min: 0.,
                            max: 0.5,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter("method", |c| {
                            if let Control::Enum { selected, .. } = c {
                                unsafe { Method::from_unchecked(*selected as u32) }.has_smoothness()
                            } else {
                                false
                            }
                        }),
                        presetable: true,
                    },
                ],
            }],
        }
    }
}
