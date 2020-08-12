use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};
use crate::ui::param_box::*;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use strum_macros::*;
use surfacelab_derive::*;
use zerocopy::AsBytes;

use std::cell::RefCell;
use std::rc::Rc;

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
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct Blend {
    pub blend_mode: BlendMode,
    pub mix: f32,
    pub clamp_output: ParameterBool,
}

impl Default for Blend {
    fn default() -> Self {
        Self {
            blend_mode: BlendMode::Mix,
            mix: 0.5,
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
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../../shaders/blend.spv"),
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
        })
    }
}

impl OperatorParamBox for Blend {
    fn param_box(&self, res: Rc<RefCell<crate::lang::Resource>>) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[
                    Parameter {
                        name: "Blend Mode",
                        transmitter: Field(Blend::BLEND_MODE),
                        control: Control::Enum {
                            selected: self.blend_mode as usize,
                            variants: BlendMode::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        available: true,
                    },
                    Parameter {
                        name: "Clamp",
                        transmitter: Field(Blend::CLAMP_OUTPUT),
                        control: Control::Toggle {
                            def: self.clamp_output == 1,
                        },
                        available: true,
                    },
                    Parameter {
                        name: "Mix",
                        transmitter: Field(Blend::MIX),
                        control: Control::Slider {
                            value: self.mix,
                            min: 0.,
                            max: 1.,
                        },
                        available: true,
                    },
                ],
            }],
        })
    }
}
