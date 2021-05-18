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
pub enum ShapeType {
    Circle = 0,
    Box = 1,
    RegularNGon = 2,
    RegularStar = 3,
    Ellipse = 4,
}

impl ShapeType {
    pub fn has_radius(self) -> bool {
        matches!(self, Self::Circle | Self::RegularNGon | Self::RegularStar)
    }

    pub fn has_width(self) -> bool {
        matches!(self, Self::Box | Self::Ellipse)
    }

    pub fn has_height(self) -> bool {
        matches!(self, Self::Box | Self::Ellipse)
    }

    pub fn has_sides(self) -> bool {
        matches!(self, Self::RegularNGon | Self::RegularStar)
    }

    pub fn has_angle_factor(self) -> bool {
        matches!(self, Self::RegularStar)
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Shape {
    pub translation: [f32; 2],
    pub rotation: f32,
    pub mirror_x: ParameterBool,
    pub mirror_y: ParameterBool,
    pub shape_type: ShapeType,
    pub shell: ParameterBool,
    pub radius: f32,
    pub width: f32,
    pub height: f32,
    pub angle_factor: f32,
    pub sides: i32,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            translation: [0.; 2],
            rotation: 0.,
            mirror_x: 0,
            mirror_y: 0,
            shape_type: ShapeType::Circle,
            shell: 0,
            radius: 0.3,
            width: 0.4,
            height: 0.2,
            angle_factor: 0.5,
            sides: 6,
        }
    }
}

impl Socketed for Shape {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
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
            preset_tag: Some("shape".to_string()),
            categories: vec![
                ParamCategory {
                    name: "transform",
                    is_open: false,
                    visibility: VisibilityFunction::default(),
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
                            presetable: true,
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
                            presetable: true,
                        },
                        Parameter {
                            name: "mirror-x".to_string(),
                            transmitter: Field(Shape::MIRROR_X.to_string()),
                            control: Control::Toggle {
                                def: self.mirror_x == 1,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::default(),
                            presetable: true,
                        },
                        Parameter {
                            name: "mirror-y".to_string(),
                            transmitter: Field(Shape::MIRROR_Y.to_string()),
                            control: Control::Toggle {
                                def: self.mirror_y == 1,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::default(),
                            presetable: true,
                        },
                    ],
                },
                ParamCategory {
                    name: "basic-parameters",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
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
                            presetable: true,
                        },
                        Parameter {
                            name: "shell".to_string(),
                            transmitter: Field(Shape::SHELL.to_string()),
                            control: Control::Toggle {
                                def: self.shell == 1,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::default(),
                            presetable: true,
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
                            visibility: VisibilityFunction::on_parameter_enum(
                                "shape-type",
                                |t: ShapeType| t.has_radius(),
                            ),
                            presetable: true,
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
                            visibility: VisibilityFunction::on_parameter_enum(
                                "shape-type",
                                |t: ShapeType| t.has_width(),
                            ),
                            presetable: true,
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
                            visibility: VisibilityFunction::on_parameter_enum(
                                "shape-type",
                                |t: ShapeType| t.has_height(),
                            ),
                            presetable: true,
                        },
                        Parameter {
                            name: "sides".to_string(),
                            transmitter: Field(Shape::SIDES.to_string()),
                            control: Control::DiscreteSlider {
                                value: self.sides,
                                min: 2,
                                max: 32,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::on_parameter_enum(
                                "shape-type",
                                |t: ShapeType| t.has_sides(),
                            ),
                            presetable: true,
                        },
                        Parameter {
                            name: "angle-factor".to_string(),
                            transmitter: Field(Shape::ANGLE_FACTOR.to_string()),
                            control: Control::Slider {
                                value: self.angle_factor,
                                min: 0.,
                                max: 1.,
                            },
                            expose_status: Some(ExposeStatus::Unexposed),
                            visibility: VisibilityFunction::on_parameter_enum(
                                "shape-type",
                                |t: ShapeType| t.has_angle_factor(),
                            ),
                            presetable: true,
                        },
                    ],
                },
            ],
        }
    }
}
