use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};

use maplit::hashmap;
use serde_big_array::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use zerocopy::AsBytes;

big_array! { BigArray; }

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Serialize, Deserialize)]
pub struct Ramp {
    #[serde(with = "BigArray")]
    ramp_data: [[f32; 4]; 64],
    ramp_size: u32,
    ramp_min: f32,
    ramp_max: f32,
}

impl Ramp {
    pub const RAMP: &'static str = "ramp";

    pub fn get_steps(&self) -> Vec<[f32; 4]> {
        (0..self.ramp_size)
            .map(|i| self.ramp_data[i as usize])
            .collect()
    }
}

impl PartialEq for Ramp {
    fn eq(&self, other: &Self) -> bool {
        let mut ramps_equal = true;
        for i in 0..64 {
            ramps_equal &= self.ramp_data[i] == other.ramp_data[i]
        }

        ramps_equal
            && self.ramp_size == other.ramp_size
            && self.ramp_min == other.ramp_min
            && self.ramp_max == other.ramp_max
    }
}

impl std::fmt::Debug for Ramp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RampParameters")
            .field("ramp_size", &self.ramp_size)
            .field("ramp_data", &[()])
            .field("ramp_min", &self.ramp_min)
            .field("ramp_max", &self.ramp_max)
            .finish()
    }
}

impl Default for Ramp {
    fn default() -> Self {
        Self {
            ramp_data: {
                let mut arr = [[0.0; 4]; 64];
                arr[1] = [1., 1., 1., 1.];
                arr
            },
            ramp_size: 2,
            ramp_min: 0.,
            ramp_max: 1.,
        }
    }
}

/// RampParameters has a manual Parameters implementation since the GPU side
/// representation and the broker representation differ.
impl Parameters for Ramp {
    fn set_parameter(&mut self, field: &str, data: &[u8]) {
        match field {
            Self::RAMP => {
                let mut ramp: Vec<[f32; 4]> = data
                    .chunks(std::mem::size_of::<[f32; 4]>())
                    .map(|chunk| {
                        let fields: Vec<f32> = chunk
                            .chunks(4)
                            .map(|z| {
                                let mut arr: [u8; 4] = Default::default();
                                arr.copy_from_slice(z);
                                f32::from_be_bytes(arr)
                            })
                            .collect();
                        [fields[0], fields[1], fields[2], fields[3]]
                    })
                    .collect();

                // vector needs to be sorted because the shader assumes sortedness!
                ramp.sort_by(|a, b| a[3].partial_cmp(&b[3]).unwrap_or(std::cmp::Ordering::Equal));

                // obtain extra information for shader
                self.ramp_size = ramp.len() as u32;
                self.ramp_min = ramp
                    .iter()
                    .map(|x| x[3])
                    .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(0.0);
                self.ramp_max = ramp
                    .iter()
                    .map(|x| x[3])
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap_or(1.0);

                // resize before copying, this is required by copy_from_slice
                ramp.resize_with(64, || [0.0; 4]);
                self.ramp_data.copy_from_slice(&ramp);
            }
            _ => panic!("Unknown field {}", field),
        }
    }
}

impl Socketed for Ramp {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "factor".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &str {
        "ramp"
    }

    fn title(&self) -> &str {
        "Ramp"
    }
}

impl Shader for Ramp {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../../shaders/ramp.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("factor"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
                },
            ],
        })
    }
}

impl OperatorParamBox for Ramp {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            categories: vec![ParamCategory {
                name: "Basic Parameters",
                parameters: vec![Parameter {
                    name: "Gradient".to_string(),
                    transmitter: Field(Ramp::RAMP.to_string()),
                    control: Control::Ramp {
                        steps: self.get_steps(),
                    },
                    expose_status: Some(ExposeStatus::Unexposed),
                }],
            }],
        }
    }
}
