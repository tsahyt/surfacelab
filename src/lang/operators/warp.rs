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
pub enum WarpMode {
    Push = 0,
    Pull = 1,
    Directional = 2,
    SlopeBlur = 3,
    SlopeBlurInv = 4,
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
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab_case")]
pub enum BlendMode {
    Mix = 0,
    Min = 1,
    Max = 2,
}

impl WarpMode {
    fn has_angle(&self) -> bool {
        matches!(self, Self::Directional)
    }

    fn has_iterations(&self) -> bool {
        matches!(self, Self::SlopeBlur | Self::SlopeBlurInv)
    }

    fn has_blend_mode(&self) -> bool {
        matches!(self, Self::SlopeBlur | Self::SlopeBlurInv)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Warp {
    pub mode: WarpMode,
    pub blend_mode: BlendMode,
    pub intensity: f32,
    pub angle: f32,
    pub iterations: i32,
}

impl Default for Warp {
    fn default() -> Self {
        Self {
            mode: WarpMode::Push,
            blend_mode: BlendMode::Mix,
            intensity: 1.,
            angle: 0.,
            iterations: 32,
        }
    }
}

impl Socketed for Warp {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "in".to_string() => (OperatorType::Polymorphic(0), false),
            "intensity".to_string() => (OperatorType::Polymorphic(1), false)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "out".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn default_name(&self) -> &str {
        "warp"
    }

    fn title(&self) -> &str {
        "Warp"
    }
}

impl Shader for Warp {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("warp"),
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
                    descriptor: OperatorDescriptorUse::InputImage("intensity"),
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 5,
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

impl OperatorParamBox for Warp {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("warp".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "warp-mode".to_string(),
                        transmitter: Field(Warp::MODE.to_string()),
                        control: Control::Enum {
                            selected: self.mode as usize,
                            variants: WarpMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_type_variable(1, |t| {
                            matches!(t, ImageType::Grayscale)
                        }),
                        presetable: true,
                    },
                    Parameter {
                        name: "blend-mode".to_string(),
                        transmitter: Field(Warp::BLEND_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.blend_mode as usize,
                            variants: BlendMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "warp-mode",
                            |t: WarpMode| t.has_blend_mode(),
                        ),
                        presetable: true,
                    },
                    Parameter {
                        name: "intensity".to_string(),
                        transmitter: Field(Warp::INTENSITY.to_string()),
                        control: Control::Slider {
                            value: self.intensity,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "angle".to_string(),
                        transmitter: Field(Warp::ANGLE.to_string()),
                        control: Control::Slider {
                            value: self.angle,
                            min: 0.,
                            max: std::f32::consts::TAU,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "warp-mode",
                            |t: WarpMode| t.has_angle(),
                        ),
                        presetable: true,
                    },
                    Parameter {
                        name: "iterations".to_string(),
                        transmitter: Field(Warp::ITERATIONS.to_string()),
                        control: Control::DiscreteSlider {
                            value: self.iterations,
                            min: 1,
                            max: 128,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "warp-mode",
                            |t: WarpMode| t.has_iterations(),
                        ),
                        presetable: true,
                    },
                ],
            }],
        }
    }
}
