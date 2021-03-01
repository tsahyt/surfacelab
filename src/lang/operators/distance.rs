use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Distance {
    threshold: f32,
}

impl Default for Distance {
    fn default() -> Self {
        Self { threshold: 0.5 }
    }
}

impl Socketed for Distance {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "in".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "out".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &str {
        "distance"
    }

    fn title(&self) -> &str {
        "Distance"
    }
}

const DISTANCE_DESCRIPTORS: &'static [OperatorDescriptor] = &[
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
        descriptor: OperatorDescriptorUse::IntermediateBuffer("tmp"),
    },
    OperatorDescriptor {
        binding: 4,
        descriptor: OperatorDescriptorUse::IntermediateBuffer("s"),
    },
    OperatorDescriptor {
        binding: 5,
        descriptor: OperatorDescriptorUse::IntermediateBuffer("t"),
    },
    OperatorDescriptor {
        binding: 6,
        descriptor: OperatorDescriptorUse::OutputImage("out"),
    },
];

impl Shader for Distance {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![
            OperatorPassDescription::Synchronize(&[SynchronizeDescription::ToReadWrite("tmp")]),
            OperatorPassDescription::RunShader(OperatorShader {
                spirv: shader!("distance"),
                descriptors: DISTANCE_DESCRIPTORS,
                specialization: gfx_hal::spec_const_list!(0u32),
                shape: OperatorShape::PerRowOrColumn { local_size: 64 },
            }),
            OperatorPassDescription::Synchronize(&[
                SynchronizeDescription::ToRead("tmp"),
                SynchronizeDescription::ToReadWrite("s"),
                SynchronizeDescription::ToReadWrite("t"),
            ]),
            OperatorPassDescription::RunShader(OperatorShader {
                spirv: shader!("distance"),
                descriptors: DISTANCE_DESCRIPTORS,
                specialization: gfx_hal::spec_const_list!(1u32),
                shape: OperatorShape::PerRowOrColumn { local_size: 64 },
            }),
        ]
    }

    fn intermediate_data(&self) -> HashMap<String, IntermediateDataDescription> {
        hashmap! {
            "tmp".to_string() => IntermediateDataDescription::Buffer {
                dim: BufferDim::Square(FromSocketOr::FromSocket("out")),
                element_width: 4,
            },
            "s".to_string() => IntermediateDataDescription::Buffer {
                dim: BufferDim::Square(FromSocketOr::FromSocket("out")),
                element_width: 4,
            },
            "t".to_string() => IntermediateDataDescription::Buffer {
                dim: BufferDim::Square(FromSocketOr::FromSocket("out")),
                element_width: 4,
            },
        }
    }
}

impl OperatorParamBox for Distance {
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
