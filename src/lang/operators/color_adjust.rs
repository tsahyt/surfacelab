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
pub enum ColorAdjustMode {
    HSV = 0,
    HSL = 1,
    HCL = 2,
}

impl ColorAdjustMode {
    fn has_saturation(self) -> bool {
        !matches!(self, Self::HCL)
    }

    fn has_value(self) -> bool {
        matches!(self, Self::HSV)
    }

    fn has_lightness(self) -> bool {
        !matches!(self, Self::HSV)
    }

    fn has_chroma(self) -> bool {
        matches!(self, Self::HCL)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct ColorAdjust {
    pub mode: ColorAdjustMode,
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub lightness: f32,
    pub chroma: f32,
    pub mix: f32,
}

impl Default for ColorAdjust {
    fn default() -> Self {
        Self {
            mode: ColorAdjustMode::HSV,
            hue: 0.5,
            saturation: 1.0,
            value: 1.0,
            lightness: 1.0,
            chroma: 1.0,
            mix: 1.0,
        }
    }
}

impl Socketed for ColorAdjust {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color_in".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color_out".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn default_name(&self) -> &str {
        "color_adjust"
    }

    fn title(&self) -> &str {
        "Color Adjust"
    }
}

impl Shader for ColorAdjust {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("color_adjust"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("color_in"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("color_out"),
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

impl OperatorParamBox for ColorAdjust {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("color_adjust".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "adjust-mode".to_string(),
                        transmitter: Field(ColorAdjust::MODE.to_string()),
                        control: Control::Enum {
                            selected: self.mode as usize,
                            variants: ColorAdjustMode::VARIANTS
                                .iter()
                                .map(|x| x.to_string())
                                .collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "mix".to_string(),
                        transmitter: Field(ColorAdjust::MIX.to_string()),
                        control: Control::Slider {
                            value: self.mix,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "hue".to_string(),
                        transmitter: Field(ColorAdjust::HUE.to_string()),
                        control: Control::Slider {
                            value: self.hue,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "saturation".to_string(),
                        transmitter: Field(ColorAdjust::SATURATION.to_string()),
                        control: Control::Slider {
                            value: self.saturation,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "adjust-mode",
                            |t: ColorAdjustMode| t.has_saturation(),
                        ),
                        presetable: true,
                    },
                    Parameter {
                        name: "chroma".to_string(),
                        transmitter: Field(ColorAdjust::CHROMA.to_string()),
                        control: Control::Slider {
                            value: self.chroma,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "adjust-mode",
                            |t: ColorAdjustMode| t.has_chroma(),
                        ),
                        presetable: true,
                    },
                    Parameter {
                        name: "value".to_string(),
                        transmitter: Field(ColorAdjust::VALUE.to_string()),
                        control: Control::Slider {
                            value: self.value,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "adjust-mode",
                            |t: ColorAdjustMode| t.has_value(),
                        ),
                        presetable: true,
                    },
                    Parameter {
                        name: "lightness".to_string(),
                        transmitter: Field(ColorAdjust::LIGHTNESS.to_string()),
                        control: Control::Slider {
                            value: self.lightness,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::on_parameter_enum(
                            "adjust-mode",
                            |t: ColorAdjustMode| t.has_lightness(),
                        ),
                        presetable: true,
                    },
                ],
            }],
        }
    }
}
