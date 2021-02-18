use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{
    OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader, Uniforms,
};
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes)]
pub struct TransformUniforms {
    pub transform: [[f32; 4]; 3],
    pub tiling: ParameterBool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Transform {
    pub translation: [f32; 2],
    pub scale: [f32; 2],
    pub shear: [f32; 2],
    pub rotation: f32,
    pub tiling: ParameterBool,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0., 0.],
            scale: [1., 1.],
            shear: [0., 0.],
            rotation: 0.,
            tiling: 1,
        }
    }
}

impl Socketed for Transform {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "in".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "out".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn default_name(&self) -> &str {
        "transform"
    }

    fn title(&self) -> &str {
        "Transform"
    }
}

/// Transform requires a matrix for its uniforms that is not stored in the
/// operator parameters but needs to be constructed on demand.
impl Uniforms for Transform {
    fn uniforms(&self) -> Cow<[u8]> {
        let similarity: nalgebra::Matrix3<f32> = nalgebra::Isometry2::new(
            nalgebra::Vector2::new(self.translation[0], self.translation[1]),
            self.rotation * std::f32::consts::PI / 180.,
        )
        .to_homogeneous();

        let mut shear_matrix = nalgebra::Matrix3::identity();
        shear_matrix[(0, 1)] = self.shear[0].tan();
        shear_matrix[(1, 0)] = self.shear[1].tan();

        let mut scale_matrix = nalgebra::Matrix3::identity();
        scale_matrix[(0, 0)] = self.scale[0];
        scale_matrix[(1, 1)] = self.scale[1];

        let transform = similarity * shear_matrix * scale_matrix;

        let uniforms = TransformUniforms {
            transform: [
                [transform[(0, 0)], transform[(0, 1)], transform[(0, 2)], 0.0],
                [transform[(1, 0)], transform[(1, 1)], transform[(1, 2)], 0.0],
                [transform[(2, 0)], transform[(2, 1)], transform[(2, 2)], 0.0],
            ],
            tiling: self.tiling,
        };

        Cow::Owned(uniforms.as_bytes().iter().copied().collect())
    }
}

impl Shader for Transform {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: shader!("transform"),
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
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("out"),
                },
            ],
        })
    }
}

impl OperatorParamBox for Transform {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "translation".to_string(),
                        transmitter: Field(Transform::TRANSLATION.to_string()),
                        control: Control::XYPad {
                            value: self.translation,
                            min: [-1., -1.],
                            max: [1., 1.],
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "scale".to_string(),
                        transmitter: Field(Transform::SCALE.to_string()),
                        control: Control::XYPad {
                            value: self.scale,
                            min: [0., 0.],
                            max: [2., 2.],
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "shear".to_string(),
                        transmitter: Field(Transform::SHEAR.to_string()),
                        control: Control::XYPad {
                            value: self.shear,
                            min: [-1., -1.],
                            max: [1., 1.],
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "rotation".to_string(),
                        transmitter: Field(Transform::ROTATION.to_string()),
                        control: Control::Slider {
                            value: self.rotation,
                            min: 0.,
                            max: 360.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                    Parameter {
                        name: "tiling".to_string(),
                        transmitter: Field(Transform::TILING.to_string()),
                        control: Control::Toggle {
                            def: self.tiling == 1,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                    },
                ],
            }],
        }
    }
}
