use super::parameters::*;
use super::socketed::*;
use crate::compute::shaders::{OperatorDescriptor, OperatorDescriptorUse, OperatorShader, Shader};
use crate::ui::param_box::*;

use maplit::hashmap;
use serde_big_array::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::VariantNames;
use strum_macros::*;
use surfacelab_derive::*;
use zerocopy::AsBytes;

big_array! { BigArray; }

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
    ParameterField,
)]
pub enum BlendMode {
    Mix,
    Multiply,
    Add,
    Subtract,
    Screen,
    Overlay,
    Darken,
    Lighten,
    SmoothDarken,
    SmoothLighten,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct Blend {
    pub blend_mode: BlendMode,
    pub mix: f32,
    pub clamp_output: ParameterBool,
}

impl Default for Blend {
    fn default() -> Self {
        Self {
            blend_mode: BlendMode::Mix,
            mix: 0.5,
            clamp_output: 0,
        }
    }
}

impl Socketed for Blend {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "background".to_string() => OperatorType::Polymorphic(0),
            "foreground".to_string() => OperatorType::Polymorphic(0)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Polymorphic(0),
        }
    }

    fn default_name(&self) -> &'static str {
        "blend"
    }

    fn title(&self) -> &'static str {
        "Blend"
    }
}

impl Shader for Blend {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../shaders/blend.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("background"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("foreground"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
                },
            ],
        })
    }
}

impl OperatorParamBox for Blend {
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[
                    Parameter {
                        name: "Blend Mode",
                        transmitter: Field(Blend::BLEND_MODE),
                        control: Control::Enum {
                            selected: self.blend_mode as usize,
                            variants: BlendMode::VARIANTS,
                        },
                    },
                    Parameter {
                        name: "Clamp",
                        transmitter: Field(Blend::CLAMP_OUTPUT),
                        control: Control::Toggle {
                            def: self.clamp_output == 1,
                        },
                    },
                    Parameter {
                        name: "Mix",
                        transmitter: Field(Blend::MIX),
                        control: Control::Slider {
                            value: self.mix,
                            min: 0.,
                            max: 1.,
                        },
                    },
                ],
            }],
        })
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct PerlinNoise {
    pub scale: f32,
    pub octaves: u32,
    pub attenuation: f32,
}

impl Default for PerlinNoise {
    fn default() -> Self {
        Self {
            scale: 3.0,
            octaves: 2,
            attenuation: 2.0,
        }
    }
}

impl Socketed for PerlinNoise {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! { "noise".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &'static str {
        "perlin_noise"
    }

    fn title(&self) -> &'static str {
        "Perlin Noise"
    }
}

impl Shader for PerlinNoise {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../shaders/perlin.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::OutputImage("noise"),
                },
            ],
        })
    }
}

impl OperatorParamBox for PerlinNoise {
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[
                    Parameter {
                        name: "Scale",
                        transmitter: Field(PerlinNoise::SCALE),
                        control: Control::Slider {
                            value: self.scale,
                            min: 0.,
                            max: 16.,
                        },
                    },
                    Parameter {
                        name: "Octaves",
                        transmitter: Field(PerlinNoise::OCTAVES),
                        control: Control::DiscreteSlider {
                            value: self.octaves as _,
                            min: 0,
                            max: 24,
                        },
                    },
                    Parameter {
                        name: "Attenuation",
                        transmitter: Field(PerlinNoise::ATTENUATION),
                        control: Control::Slider {
                            value: self.attenuation,
                            min: 0.,
                            max: 4.,
                        },
                    },
                ],
            }],
        })
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct Rgb {
    pub rgb: [f32; 3],
}

impl Default for Rgb {
    fn default() -> Self {
        Self {
            rgb: [0.5, 0.7, 0.3],
        }
    }
}

impl Socketed for Rgb {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn default_name(&self) -> &'static str {
        "rgb"
    }

    fn title(&self) -> &'static str {
        "RGB Color"
    }
}

impl Shader for Rgb {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../shaders/rgb.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
                },
            ],
        })
    }
}

impl OperatorParamBox for Rgb {
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Color",
                    transmitter: Field(Rgb::RGB),
                    control: Control::RgbColor { value: self.rgb },
                }],
            }],
        })
    }
}

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
    ParameterField,
)]
pub enum GrayscaleMode {
    Luminance,
    Average,
    Desaturate,
    MaxDecompose,
    MinDecompose,
    RedOnly,
    GreenOnly,
    BlueOnly,
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct Grayscale {
    pub mode: GrayscaleMode,
}

impl Default for Grayscale {
    fn default() -> Self {
        Self {
            mode: GrayscaleMode::Luminance,
        }
    }
}

impl Socketed for Grayscale {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "value".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn default_name(&self) -> &'static str {
        "grayscale"
    }

    fn title(&self) -> &'static str {
        "Grayscale"
    }
}

impl Shader for Grayscale {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../shaders/grayscale.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("color"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("value"),
                },
            ],
        })
    }
}

impl OperatorParamBox for Grayscale {
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Conversion Mode",
                    transmitter: Field(Grayscale::MODE),
                    control: Control::Enum {
                        selected: self.mode as usize,
                        variants: GrayscaleMode::VARIANTS,
                    },
                }],
            }],
        })
    }
}

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
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
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

    fn default_name(&self) -> &'static str {
        "ramp"
    }

    fn title(&self) -> &'static str {
        "Ramp"
    }
}

impl Shader for Ramp {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../shaders/ramp.spv"),
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
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Gradient",
                    transmitter: Field(Ramp::RAMP),
                    control: Control::Ramp {
                        steps: self.get_steps(),
                    },
                }],
            }],
        })
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct NormalMap {
    pub strength: f32,
}

impl Default for NormalMap {
    fn default() -> Self {
        Self { strength: 1.0 }
    }
}

impl Socketed for NormalMap {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "height".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "normal".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &'static str {
        "normal_map"
    }

    fn title(&self) -> &'static str {
        "Normal Map"
    }
}

impl Shader for NormalMap {
    fn operator_shader(&self) -> Option<OperatorShader> {
        Some(OperatorShader {
            spirv: include_bytes!("../../shaders/normal.spv"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::Uniforms,
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("height"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("normal"),
                },
            ],
        })
    }
}

impl OperatorParamBox for NormalMap {
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Strength",
                    transmitter: Field(NormalMap::STRENGTH),
                    control: Control::Slider {
                        value: self.strength,
                        min: 0.,
                        max: 2.,
                    },
                }],
            }],
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parameters)]
pub struct Image {
    pub path: std::path::PathBuf,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::new(),
        }
    }
}

impl Socketed for Image {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "image".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
        }
    }

    fn default_name(&self) -> &'static str {
        "image"
    }

    fn title(&self) -> &'static str {
        "Image"
    }
}

impl Shader for Image {
    fn operator_shader(&self) -> Option<OperatorShader> {
        None
    }
}

impl OperatorParamBox for Image {
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Image Path",
                    transmitter: Field("image_path"), // TODO: consts
                    control: Control::File {
                        selected: Some(self.path.to_owned()), // TODO: probably not necessary to clone the path
                    },
                }],
            }],
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parameters)]
pub struct Output {
    pub output_type: super::OutputType,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            output_type: super::OutputType::default(),
        }
    }
}

impl Socketed for Output {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "data".to_string() => match self.output_type {
                super::OutputType::Albedo => OperatorType::Monomorphic(ImageType::Rgb),
                super::OutputType::Roughness => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Normal => OperatorType::Monomorphic(ImageType::Rgb),
                super::OutputType::Displacement => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Metallic => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Value => OperatorType::Monomorphic(ImageType::Grayscale),
                super::OutputType::Rgb => OperatorType::Monomorphic(ImageType::Rgb),
        }
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {}
    }

    fn default_name(&self) -> &'static str {
        "output"
    }

    fn title(&self) -> &'static str {
        "Output"
    }
}

impl Shader for Output {
    fn operator_shader(&self) -> Option<OperatorShader> {
        None
    }
}

impl OperatorParamBox for Output {
    fn param_box(&self, res: &super::Resource) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: self.title(),
            resource: res.clone(),
            categories: &[ParamCategory {
                name: "Basic Parameters",
                parameters: &[Parameter {
                    name: "Output Type",
                    transmitter: Field("output_type"), // TODO: consts
                    control: Control::Enum {
                        selected: self.output_type as usize,
                        variants: super::OutputType::VARIANTS,
                    },
                }],
            }],
        })
    }
}
