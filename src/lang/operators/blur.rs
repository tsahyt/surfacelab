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
    PartialEq,
)]
#[strum(serialize_all = "kebab-case")]
pub enum BlurQuality {
    LowQuality = 0,
    HighQuality = 1,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Blur {
    quality: BlurQuality,
    sigma: f32,
}

impl Default for Blur {
    fn default() -> Self {
        Self {
            quality: BlurQuality::HighQuality,
            sigma: 5.0,
        }
    }
}

impl Socketed for Blur {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "in".to_string() => (OperatorType::Polymorphic(0), false),
            "mask".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), true),
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
        descriptor: OperatorDescriptorUse::Occupancy,
    },
    OperatorDescriptor {
        binding: 2,
        descriptor: OperatorDescriptorUse::InputImage("in"),
    },
    OperatorDescriptor {
        binding: 3,
        descriptor: OperatorDescriptorUse::InputImage("mask"),
    },
    OperatorDescriptor {
        binding: 4,
        descriptor: OperatorDescriptorUse::Sampler,
    },
    OperatorDescriptor {
        binding: 5,
        descriptor: OperatorDescriptorUse::IntermediateImage("tmp1"),
    },
    OperatorDescriptor {
        binding: 6,
        descriptor: OperatorDescriptorUse::IntermediateImage("tmp2"),
    },
    OperatorDescriptor {
        binding: 7,
        descriptor: OperatorDescriptorUse::OutputImage("out"),
    },
];

impl Shader for Blur {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![
            OperatorPassDescription::SynchronizeImage(&[
                SynchronizeDescription::ToReadWrite("tmp1"),
                SynchronizeDescription::ToReadWrite("tmp2"),
            ]),
            OperatorPassDescription::RunShader(OperatorShader {
                spirv: shader!("blur"),
                descriptors: BLUR_DESCRIPTORS,
                specialization: gfx_hal::spec_const_list!(0u32),
                shape: OperatorShape::PerRowOrColumn { local_size: 64 },
            }),
            OperatorPassDescription::SynchronizeImage(&[
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

    fn intermediate_data(&self) -> HashMap<String, IntermediateDataDescription> {
        hashmap! {
            "tmp1".to_string() => IntermediateDataDescription::Image {
                size: FromSocketOr::FromSocket("out"),
                ty: FromSocketOr::FromSocket("out"),
            },
            "tmp2".to_string() => IntermediateDataDescription::Image {
                size: FromSocketOr::FromSocket("out"),
                ty: FromSocketOr::FromSocket("out"),
            },
        }
    }
}

impl OperatorParamBox for Blur {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("blur".to_string()),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![
                    Parameter {
                        name: "quality".to_string(),
                        transmitter: Field(Blur::QUALITY.to_string()),
                        control: Control::Enum {
                            selected: self.quality as usize,
                            variants: BlurQuality::VARIANTS
                                .iter()
                                .map(|x| x.to_string())
                                .collect(),
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                        presetable: true,
                    },
                    Parameter {
                        name: "sigma".to_string(),
                        transmitter: Field(Blur::SIGMA.to_string()),
                        control: Control::Slider {
                            value: self.sigma,
                            min: 1.,
                            max: 256.,
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
