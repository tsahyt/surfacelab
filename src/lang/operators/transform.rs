use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransformMatrix([[f32; 4]; 3]);

impl TransformMatrix {
    pub fn new(translation: [f32; 2], rotation: f32, scale: f32) -> Self {
        let transform: nalgebra::Matrix3<f32> = nalgebra::Similarity2::new(
            nalgebra::Vector2::new(translation[0], translation[1]),
            rotation,
            scale,
        )
        .to_homogeneous();

        Self([
            [transform[(0, 0)], transform[(1, 0)], transform[(2, 0)], 0.0],
            [transform[(0, 1)], transform[(1, 1)], transform[(2, 1)], 0.0],
            [transform[(0, 2)], transform[(1, 2)], transform[(2, 2)], 0.0],
        ])
    }
}

impl Default for TransformMatrix {
    fn default() -> Self {
        Self::new([0., 0.], 0., 1.)
    }
}

impl ParameterField for TransformMatrix {
    fn from_data(data: &[u8]) -> Self {
        Self(<[[f32; 4]; 3]>::from_data(data))
    }

    fn to_data(&self) -> Vec<u8> {
        self.0.to_data()
    }

    fn data_length() -> usize {
        <[[f32; 4]; 3]>::data_length()
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Transform {
    pub transform: TransformMatrix,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            transform: TransformMatrix::default(),
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
                parameters: vec![],
            }],
        }
    }
}
