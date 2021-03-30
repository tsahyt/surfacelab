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
pub enum WarpMode {
    Push = 0,
    Pull = 1,
    Directional = 2,
}

impl WarpMode {
    fn has_angle(&self) -> bool {
        matches!(self, Self::Directional)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Warp {
    pub mode: WarpMode,
    pub intensity: f32,
    pub angle: f32,
}

impl Default for Warp {
    fn default() -> Self {
        Self {
            mode: WarpMode::Push,
            intensity: 1.,
            angle: 0.,
        }
    }
}

impl Socketed for Warp {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "in".to_string() => OperatorType::Polymorphic(0),
            "intensity".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
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
                    descriptor: OperatorDescriptorUse::InputImage("in"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("intensity"),
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

impl OperatorParamBox for Warp {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "warp-mode".to_string(),
                        transmitter: Field(Warp::MODE.to_string()),
                        control: Control::Enum {
                            selected: self.mode as usize,
                            variants: WarpMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
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
                        visibility: VisibilityFunction::on_parameter("warp-mode", |c| {
                            if let Control::Enum { selected, .. } = c {
                                unsafe { WarpMode::from_unchecked(*selected as u32) }.has_angle()
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
