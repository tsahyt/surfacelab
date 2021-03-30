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
pub struct NoiseSpread {
    pub distance: f32,
    pub samples: i32,
}

impl Default for NoiseSpread {
    fn default() -> Self {
        Self {
            distance: 1.0,
            samples: 1,
        }
    }
}

impl Socketed for NoiseSpread {
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
        "noise_spread"
    }

    fn title(&self) -> &str {
        "Noise Spread"
    }
}

impl Shader for NoiseSpread {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("noise_spread"),
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
            specialization: Specialization::default(),
            shape: OperatorShape::PerPixel {
                local_x: 8,
                local_y: 8,
            },
        })]
    }
}

impl OperatorParamBox for NoiseSpread {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "basic-parameters",
                parameters: vec![
                    Parameter {
                        name: "spread-distance".to_string(),
                        transmitter: Field(NoiseSpread::DISTANCE.to_string()),
                        control: Control::Slider {
                            value: self.distance,
                            min: 0.,
                            max: 2.,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                    Parameter {
                        name: "spread-samples".to_string(),
                        transmitter: Field(NoiseSpread::SAMPLES.to_string()),
                        control: Control::DiscreteSlider {
                            value: self.samples,
                            min: 1,
                            max: 32,
                        },
                        expose_status: Some(ExposeStatus::Unexposed),
                        visibility: VisibilityFunction::default(),
                    },
                ],
            }],
        }
    }
}
