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
    PartialEq,
)]
#[strum(serialize_all = "kebab_case")]
pub enum CoordinateSpace {
    Cartesian = 0,
    Polar = 1,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct CoordinateTransform {
    pub from_space: CoordinateSpace,
    pub to_space: CoordinateSpace,
    pub supersample: ParameterBool,
}

impl Default for CoordinateTransform {
    fn default() -> Self {
        Self {
            from_space: CoordinateSpace::Cartesian,
            to_space: CoordinateSpace::Polar,
            supersample: 0,
        }
    }
}

impl Socketed for CoordinateTransform {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "input".to_string() => (OperatorType::Polymorphic(0), false),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "output".to_string() => OperatorType::Polymorphic(0),
        }
    }

    fn default_name(&self) -> &str {
        "coordinate_transform"
    }

    fn title(&self) -> &str {
        "Coordinate Transform"
    }
}

impl Shader for CoordinateTransform {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("coordinate_transform"),
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
                    descriptor: OperatorDescriptorUse::OutputImage("output"),
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

impl OperatorParamBox for CoordinateTransform {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("range".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "from-space".to_string(),
                        transmitter: Field(CoordinateTransform::FROM_SPACE.to_string()),
                        control: Control::Enum {
                            selected: self.from_space as usize,
                            variants: CoordinateSpace::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "to-space".to_string(),
                        transmitter: Field(CoordinateTransform::TO_SPACE.to_string()),
                        control: Control::Enum {
                            selected: self.to_space as usize,
                            variants: CoordinateSpace::VARIANTS.iter().map(|x| x.to_string()).collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "supersample".to_string(),
                        transmitter: Field(CoordinateTransform::SUPERSAMPLE.to_string()),
                        control: Control::Toggle {
                            def: self.supersample == 1,
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
