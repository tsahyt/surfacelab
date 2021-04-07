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
pub enum DistanceMetric {
    Euclidean = 0,
    Manhattan = 1,
    Chebyshev = 2,
}

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
pub enum DistanceBorderMode {
    Closed = 0,
    Open = 1,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Distance {
    metric: DistanceMetric,
    border_mode: DistanceBorderMode,
    clamp: ParameterBool,
    expand: ParameterBool,
    threshold: f32,
    extent: f32,
}

impl Default for Distance {
    fn default() -> Self {
        Self {
            metric: DistanceMetric::Euclidean,
            border_mode: DistanceBorderMode::Closed,
            clamp: 0,
            expand: 0,
            threshold: 0.5,
            extent: 3.0,
        }
    }
}

impl Socketed for Distance {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "in".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "out".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "distance"
    }

    fn title(&self) -> &str {
        "Distance"
    }
}

const DISTANCE_DESCRIPTORS: &'static [OperatorDescriptor] = &[
    OperatorDescriptor {
        binding: 0,
        descriptor: OperatorDescriptorUse::Uniforms,
    },
    OperatorDescriptor {
        binding: 1,
        descriptor: OperatorDescriptorUse::InputImage("in"),
    },
    OperatorDescriptor {
        binding: 2,
        descriptor: OperatorDescriptorUse::Sampler,
    },
    OperatorDescriptor {
        binding: 3,
        descriptor: OperatorDescriptorUse::IntermediateImage("g"),
    },
    OperatorDescriptor {
        binding: 4,
        descriptor: OperatorDescriptorUse::IntermediateBuffer("s"),
    },
    OperatorDescriptor {
        binding: 5,
        descriptor: OperatorDescriptorUse::IntermediateBuffer("t"),
    },
    OperatorDescriptor {
        binding: 6,
        descriptor: OperatorDescriptorUse::OutputImage("out"),
    },
];

impl Shader for Distance {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![
            OperatorPassDescription::SynchronizeImage(&[SynchronizeDescription::ToReadWrite("g")]),
            OperatorPassDescription::RunShader(OperatorShader {
                spirv: shader!("distance"),
                descriptors: DISTANCE_DESCRIPTORS,
                specialization: gfx_hal::spec_const_list!(0u32),
                shape: OperatorShape::PerRowOrColumn { local_size: 64 },
            }),
            OperatorPassDescription::SynchronizeImage(&[SynchronizeDescription::ToRead("g")]),
            OperatorPassDescription::SynchronizeBuffer(&[
                SynchronizeDescription::ToReadWrite("s"),
                SynchronizeDescription::ToReadWrite("t"),
            ]),
            OperatorPassDescription::RunShader(OperatorShader {
                spirv: shader!("distance"),
                descriptors: DISTANCE_DESCRIPTORS,
                specialization: gfx_hal::spec_const_list!(1u32),
                shape: OperatorShape::PerRowOrColumn { local_size: 64 },
            }),
        ]
    }

    fn intermediate_data(&self) -> HashMap<String, IntermediateDataDescription> {
        hashmap! {
            "g".to_string() => IntermediateDataDescription::Image {
                size: FromSocketOr::FromSocket("out"),
                ty: FromSocketOr::FromSocket("out"),
            },
            "s".to_string() => IntermediateDataDescription::Buffer {
                dim: BufferDim::Square(FromSocketOr::FromSocket("out")),
                element_width: std::mem::size_of::<i32>(),
            },
            "t".to_string() => IntermediateDataDescription::Buffer {
                dim: BufferDim::Square(FromSocketOr::FromSocket("out")),
                element_width: std::mem::size_of::<i32>(),
            },
        }
    }
}

impl OperatorParamBox for Distance {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                parameters: vec![
                    Parameter {
                        name: "metric".to_string(),
                        transmitter: Field(Distance::METRIC.to_string()),
                        control: Control::Enum {
                            selected: self.metric as usize,
                            variants: DistanceMetric::VARIANTS
                                .iter()
                                .map(|x| x.to_string())
                                .collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "border-mode".to_string(),
                        transmitter: Field(Distance::BORDER_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.border_mode as usize,
                            variants: DistanceBorderMode::VARIANTS
                                .iter()
                                .map(|x| x.to_string())
                                .collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "clamp".to_string(),
                        transmitter: Field(Distance::CLAMP.to_string()),
                        control: Control::Toggle {
                            def: self.clamp == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "expand".to_string(),
                        transmitter: Field(Distance::EXPAND.to_string()),
                        control: Control::Toggle {
                            def: self.expand == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "threshold".to_string(),
                        transmitter: Field(Distance::THRESHOLD.to_string()),
                        control: Control::Slider {
                            value: self.threshold,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "extent".to_string(),
                        transmitter: Field(Distance::EXTENT.to_string()),
                        control: Control::Slider {
                            value: self.extent,
                            min: 0.,
                            max: 10.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                ],
            }],
        }
    }
}
