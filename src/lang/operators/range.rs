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
    ParameterField,
    PartialEq,
)]
pub enum RangeMode {
    Linear = 0,
    SmoothStep = 1,
    SmootherStep = 2,
    Stepped = 3,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Range {
    pub range_mode: RangeMode,
    pub from_min: f32,
    pub from_max: f32,
    pub to_min: f32,
    pub to_max: f32,
    pub steps: i32,
    pub clamp_output: ParameterBool,
}

impl Default for Range {
    fn default() -> Self {
        Self {
            range_mode: RangeMode::Linear,
            from_min: 0.0,
            from_max: 1.0,
            to_min: 0.0,
            to_max: 1.0,
            steps: 4,
            clamp_output: 1,
        }
    }
}

impl Socketed for Range {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "input".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "value".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
        }
    }

    fn default_name(&self) -> &str {
        "range"
    }

    fn title(&self) -> &str {
        "Range"
    }
}

impl Shader for Range {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("range"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("input"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("value"),
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

impl OperatorParamBox for Range {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "range-mode".to_string(),
                        transmitter: Field(Range::RANGE_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.range_mode as usize,
                            variants: RangeMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "clamp".to_string(),
                        transmitter: Field(Range::CLAMP_OUTPUT.to_string()),
                        control: Control::Toggle {
                            def: self.clamp_output == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "from-min".to_string(),
                        transmitter: Field(Range::FROM_MIN.to_string()),
                        control: Control::Slider {
                            value: self.from_min,
                            min: -1.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "from-max".to_string(),
                        transmitter: Field(Range::FROM_MAX.to_string()),
                        control: Control::Slider {
                            value: self.from_max,
                            min: -1.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "to-min".to_string(),
                        transmitter: Field(Range::TO_MIN.to_string()),
                        control: Control::Slider {
                            value: self.to_min,
                            min: -1.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "to-max".to_string(),
                        transmitter: Field(Range::TO_MAX.to_string()),
                        control: Control::Slider {
                            value: self.to_max,
                            min: -1.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "steps".to_string(),
                        transmitter: Field(Range::STEPS.to_string()),
                        control: Control::DiscreteSlider {
                            value: self.steps,
                            min: 1,
                            max: 32,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter("range-mode", |c| {
                            if let Control::Enum { selected, .. } = c {
                                *selected == RangeMode::Stepped as usize
                            } else {
                                false
                            }
                        }),
                    },
                ],
            }],
        }
    }
}
