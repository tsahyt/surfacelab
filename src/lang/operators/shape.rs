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
pub enum ShapeType {
    Circle = 0,
    Box = 1,
    RegularNGon = 2,
    Ellipse = 3,
}

impl ShapeType {
    pub fn has_radius(self) -> bool {
        matches!(self, Self::Circle | Self::RegularNGon)
    }

    pub fn has_width(self) -> bool {
        matches!(self, Self::Box | Self::Ellipse)
    }

    pub fn has_height(self) -> bool {
        matches!(self, Self::Box | Self::Ellipse)
    }

    pub fn has_sides(self) -> bool {
        matches!(self, Self::RegularNGon)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Shape {
    pub translation: [f32; 2],
    pub rotation: f32,
    pub shape_type: ShapeType,
    pub radius: f32,
    pub width: f32,
    pub height: f32,
    pub sides: i32,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            translation: [0.; 2],
            rotation: 0.,
            shape_type: ShapeType::Circle,
            radius: 0.5,
            width: 0.3,
            height: 0.3,
            sides: 6,
        }
    }
}

impl Socketed for Shape {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "shape".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "shape"
    }

    fn title(&self) -> &str {
        "Shape"
    }
}

impl Shader for Shape {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("shape"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::OutputImage("shape"),
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

impl OperatorParamBox for Shape {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![
                ParamCategory {
                    name: "transform",
                    is_open: false,
                    parameters: vec![
                        Parameter {
                            name: "translation".to_string(),
                            transmitter: Field(Shape::TRANSLATION.to_string()),
                            control: Control::XYPad {
                                value: self.translation,
                                min: [-1., -1.],
                                max: [1., 1.],
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::default(),
                        },
                        Parameter {
                            name: "rotation".to_string(),
                            transmitter: Field(Shape::ROTATION.to_string()),
                            control: Control::Slider {
                                value: self.rotation,
                                min: 0.,
                                max: std::f32::consts::TAU,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::default(),
                        },
                    ],
                },
                ParamCategory {
                    name: "basic-parameters",
                    is_open: true,
                    parameters: vec![
                        Parameter {
                            name: "shape-type".to_string(),
                            transmitter: Field(Shape::SHAPE_TYPE.to_string()),
                            control: Control::Enum {
                                selected: self.shape_type as usize,
                                variants: ShapeType::VARIANTS
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect(),
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::default(),
                        },
                        Parameter {
                            name: "radius".to_string(),
                            transmitter: Field(Shape::RADIUS.to_string()),
                            control: Control::Slider {
                                value: self.radius,
                                min: 0.,
                                max: 1.,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::on_parameter("shape-type", |c| {
                                if let Control::Enum { selected, .. } = c {
                                    unsafe { ShapeType::from_unchecked(*selected as u32) }
                                        .has_radius()
                                } else {
                                    false
                                }
                            }),
                        },
                        Parameter {
                            name: "width".to_string(),
                            transmitter: Field(Shape::WIDTH.to_string()),
                            control: Control::Slider {
                                value: self.width,
                                min: 0.,
                                max: 1.,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::on_parameter("shape-type", |c| {
                                if let Control::Enum { selected, .. } = c {
                                    unsafe { ShapeType::from_unchecked(*selected as u32) }
                                        .has_width()
                                } else {
                                    false
                                }
                            }),
                        },
                        Parameter {
                            name: "height".to_string(),
                            transmitter: Field(Shape::HEIGHT.to_string()),
                            control: Control::Slider {
                                value: self.height,
                                min: 0.,
                                max: 1.,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::on_parameter("shape-type", |c| {
                                if let Control::Enum { selected, .. } = c {
                                    unsafe { ShapeType::from_unchecked(*selected as u32) }
                                        .has_height()
                                } else {
                                    false
                                }
                            }),
                        },
                        Parameter {
                            name: "sides".to_string(),
                            transmitter: Field(Shape::SIDES.to_string()),
                            control: Control::DiscreteSlider {
                                value: self.sides,
                                min: 1,
                                max: 32,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::on_parameter("shape-type", |c| {
                                if let Control::Enum { selected, .. } = c {
                                    unsafe { ShapeType::from_unchecked(*selected as u32) }
                                        .has_sides()
                                } else {
                                    false
                                }
                            }),
                        },
                    ],
                },
            ],
        }
    }
}
