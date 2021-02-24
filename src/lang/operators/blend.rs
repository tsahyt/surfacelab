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
    ParameterField,
    PartialEq,
)]
pub enum BlendMode {
    Mix,
    Multiply,
    Add,
    Subtract,
    Screen,
    Overlay,
    Darken,
    Lighten,
    SmoothDarken,
    SmoothLighten,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Blend {
    pub blend_mode: BlendMode,
    pub mix: f32,
    pub sharpness: f32,
    pub clamp_output: ParameterBool,
}

impl Default for Blend {
    fn default() -> Self {
        Self {
            blend_mode: BlendMode::Mix,
            mix: 0.5,
            sharpness: 16.0,
            clamp_output: 0,
        }
    }
}

impl Socketed for Blend {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "background".to_string() => OperatorType::Polymorphic(0),
            "foreground".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Polymorphic(0),
        }
    }

    fn default_name(&self) -> &str {
        "blend"
    }

    fn title(&self) -> &str {
        "Blend"
    }
}

impl Shader for Blend {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("blend"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("background"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("foreground"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
                },
            ],
            specialization: Specialization::default(),
        })]
    }
}

impl OperatorParamBox for Blend {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "blend-mode".to_string(),
                        transmitter: Field(Blend::BLEND_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.blend_mode as usize,
                            variants: BlendMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "clamp".to_string(),
                        transmitter: Field(Blend::CLAMP_OUTPUT.to_string()),
                        control: Control::Toggle {
                            def: self.clamp_output == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "mix".to_string(),
                        transmitter: Field(Blend::MIX.to_string()),
                        control: Control::Slider {
                            value: self.mix,
                            min: 0.,
                            max: 1.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "sharpness".to_string(),
                        transmitter: Field(Blend::SHARPNESS.to_string()),
                        control: Control::Slider {
                            value: self.sharpness,
                            min: 1.,
                            max: 64.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                ],
            }],
        }
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct BlendMasked {
    pub blend_mode: BlendMode,
    pub sharpness: f32,
    pub clamp_output: ParameterBool,
}

impl Default for BlendMasked {
    fn default() -> Self {
        Self {
            blend_mode: BlendMode::Mix,
            sharpness: 16.0,
            clamp_output: 0,
        }
    }
}

impl Socketed for BlendMasked {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "mask".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
            "background".to_string() => OperatorType::Polymorphic(0),
            "foreground".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Polymorphic(0),
        }
    }

    fn default_name(&self) -> &str {
        "blendmasked"
    }

    fn title(&self) -> &str {
        "Blend Masked"
    }
}

impl Shader for BlendMasked {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("blend_masked"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("mask"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("background"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::InputImage("foreground"),
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 5,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
                },
            ],
            specialization: Specialization::default(),
        })]
    }
}

impl OperatorParamBox for BlendMasked {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "blend-mode".to_string(),
                        transmitter: Field(BlendMasked::BLEND_MODE.to_string()),
                        control: Control::Enum {
                            selected: self.blend_mode as usize,
                            variants: BlendMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "clamp".to_string(),
                        transmitter: Field(BlendMasked::CLAMP_OUTPUT.to_string()),
                        control: Control::Toggle {
                            def: self.clamp_output == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "sharpness".to_string(),
                        transmitter: Field(BlendMasked::SHARPNESS.to_string()),
                        control: Control::Slider {
                            value: self.sharpness,
                            min: 1.,
                            max: 64.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                ],
            }],
        }
    }
}
