use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use num_enum::TryFromPrimitive;
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
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab_case")]
pub enum SelectMode {
    Threshold = 0,
    Band = 1,
}

impl SelectMode {
    pub fn has_bandwidth(self) -> bool {
        matches!(self, Self::Band)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Select {
    select_mode: SelectMode,
    smooth: ParameterBool,
    invert: ParameterBool,
    threshold: f32,
    bandwidth: f32,
    color: [f32; 3],
}

impl Default for Select {
    fn default() -> Self {
        Self {
            select_mode: SelectMode::Threshold,
            smooth: 0,
            invert: 0,
            threshold: 0.5,
            bandwidth: 0.,
            color: [0.5; 3],
        }
    }
}

impl Socketed for Select {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "in".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "out".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "select"
    }

    fn title(&self) -> &str {
        "Select"
    }
}

impl Shader for Select {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("select"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::Occupancy,
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("in"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::OutputImage("out"),
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

impl OperatorParamBox for Select {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("select".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "select-mode".to_string(),
                        transmitter: Field(Select::SELECT_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.select_mode as usize,
                            variants: SelectMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "smooth-edge".to_string(),
                        transmitter: Field(Select::SMOOTH.to_string()),
                        control: Control::Toggle {
                            def: self.smooth == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "invert".to_string(),
                        transmitter: Field(Select::INVERT.to_string()),
                        control: Control::Toggle {
                            def: self.invert == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "threshold".to_string(),
                        transmitter: Field(Select::THRESHOLD.to_string()),
                        control: Control::Slider {
                            value: self.threshold,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_type_variable(0, |t| {
                            matches!(t, ImageType::Grayscale)
                        }),
                        presetable: true,
                    },
                    Parameter {
                        name: "bandwidth".to_string(),
                        transmitter: Field(Select::BANDWIDTH.to_string()),
                        control: Control::Slider {
                            value: self.bandwidth,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "select-mode",
                            |t: SelectMode| t.has_bandwidth(),
                        ),
                        presetable: true,
                    },
                    Parameter {
                        name: "color".to_string(),
                        transmitter: Field(Select::COLOR.to_string()),
                        control: Control::RgbColor { value: self.color },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_type_variable(0, |t| {
                            matches!(t, ImageType::Rgb)
                        }),
                        presetable: true,
                    },
                ],
            }],
        }
    }
}
