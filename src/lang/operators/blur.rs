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
pub struct Blur {
    sigma: f32,
}

impl Default for Blur {
    fn default() -> Self {
        Self { sigma: 5.0 }
    }
}

impl Socketed for Blur {
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
        "blur"
    }

    fn title(&self) -> &str {
        "Blur"
    }
}

const BLUR_DESCRIPTORS: &'static [OperatorDescriptor] = &[
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
        descriptor: OperatorDescriptorUse::IntermediateImage("tmp1"),
    },
    OperatorDescriptor {
        binding: 4,
        descriptor: OperatorDescriptorUse::IntermediateImage("tmp2"),
    },
    OperatorDescriptor {
        binding: 5,
        descriptor: OperatorDescriptorUse::OutputImage("out"),
    },
];

impl Shader for Blur {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![
            OperatorPassDescription::Synchronize(&[
                SynchronizeDescription::ToReadWrite("tmp1"),
                SynchronizeDescription::ToReadWrite("tmp2"),
            ]),
            OperatorPassDescription::RunShader(OperatorShader {
                spirv: shader!("blur"),
                descriptors: BLUR_DESCRIPTORS,
                specialization: gfx_hal::spec_const_list!(0u32),
                shape: OperatorShape::PerRowOrColumn { local_size: 64 },
            }),
            OperatorPassDescription::Synchronize(&[
                SynchronizeDescription::ToReadWrite("tmp1"),
                SynchronizeDescription::ToReadWrite("tmp2"),
            ]),
            OperatorPassDescription::RunShader(OperatorShader {
                spirv: shader!("blur"),
                descriptors: BLUR_DESCRIPTORS,
                specialization: gfx_hal::spec_const_list!(1u32),
                shape: OperatorShape::PerRowOrColumn { local_size: 64 },
            }),
        ]
    }

    fn intermediate_data(&self) -> HashMap<String, ImageType> {
        hashmap! {
            "tmp1".to_string() => ImageType::Rgb,
            "tmp2".to_string() => ImageType::Rgb,
        }
    }
}

impl OperatorParamBox for Blur {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![Parameter {
                    name: "sigma".to_string(),
                    transmitter: Field(Blur::SIGMA.to_string()),
                    control: Control::Slider {
                        value: self.sigma,
                        min: 1.,
                        max: 256.,
                    },
                    expose_status: Some(ExposeStatus::Unexposed),
                }],
            }],
        }
    }
}
