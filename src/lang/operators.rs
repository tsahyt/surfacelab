use super::parameters::*;
use zerocopy::AsBytes;
use serde_big_array::*;
use strum_macros::*;
use surfacelab_derive::*;
use serde_derive::{Deserialize, Serialize};

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
