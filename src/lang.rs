use maplit::hashmap;
use serde_big_array::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::*;
use strum_macros::*;
use zerocopy::AsBytes;

use surfacelab_derive::*;

big_array! { BigArray; }

pub trait Parameters {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]);
}

type ParameterBool = u32;

pub trait ParameterField {
    fn from_data(data: &[u8]) -> Self;
    fn to_data(&self) -> Vec<u8>;
}

impl ParameterField for f32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        f32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for u32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        u32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for [f32; 3] {
    fn from_data(data: &[u8]) -> Self {
        let cols: Vec<f32> = data
            .chunks(4)
            .map(|z| {
                let mut arr: [u8; 4] = Default::default();
                arr.copy_from_slice(z);
                f32::from_be_bytes(arr)
            })
            .collect();
        [cols[0], cols[1], cols[2]]
    }

    fn to_data(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self[0] as f32).to_be_bytes());
        buf.extend_from_slice(&(self[1] as f32).to_be_bytes());
        buf.extend_from_slice(&(self[2] as f32).to_be_bytes());
        buf.extend_from_slice(&(1.0 as f32).to_be_bytes());
        buf
    }
}

impl ParameterField for PathBuf {
    fn from_data(data: &[u8]) -> Self {
        let path_str = unsafe { std::str::from_utf8_unchecked(&data) };
        Path::new(path_str).to_path_buf()
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_str().unwrap().as_bytes().to_vec()
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
pub struct BlendParameters {
    pub blend_mode: BlendMode,
    pub mix: f32,
    pub clamp_output: ParameterBool,
}

impl Default for BlendParameters {
    fn default() -> Self {
        BlendParameters {
            blend_mode: BlendMode::Mix,
            mix: 0.5,
            clamp_output: 0,
        }
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct PerlinNoiseParameters {
    pub scale: f32,
    pub octaves: u32,
    pub attenuation: f32,
}

impl Default for PerlinNoiseParameters {
    fn default() -> Self {
        PerlinNoiseParameters {
            scale: 3.0,
            octaves: 2,
            attenuation: 2.0,
        }
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters)]
pub struct RgbParameters {
    pub rgb: [f32; 3],
}

impl Default for RgbParameters {
    fn default() -> Self {
        RgbParameters {
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
pub struct GrayscaleParameters {
    pub mode: GrayscaleMode,
}

impl Default for GrayscaleParameters {
    fn default() -> Self {
        GrayscaleParameters {
            mode: GrayscaleMode::Luminance,
        }
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Serialize, Deserialize)]
pub struct RampParameters {
    #[serde(with = "BigArray")]
    ramp_data: [[f32; 4]; 64],
    ramp_size: u32,
    ramp_min: f32,
    ramp_max: f32,
}

impl RampParameters {
    pub const RAMP: &'static str = "ramp";

    pub fn get_steps(&self) -> Vec<[f32; 4]> {
        (0..self.ramp_size)
            .map(|i| self.ramp_data[i as usize])
            .collect()
    }
}

impl std::fmt::Debug for RampParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RampParameters")
            .field("ramp_size", &self.ramp_size)
            .field("ramp_data", &[()])
            .field("ramp_min", &self.ramp_min)
            .field("ramp_max", &self.ramp_max)
            .finish()
    }
}

impl Default for RampParameters {
    fn default() -> Self {
        RampParameters {
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
impl Parameters for RampParameters {
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
pub struct NormalMapParameters {
    pub strength: f32,
}

impl Default for NormalMapParameters {
    fn default() -> Self {
        Self { strength: 1.0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operator {
    Blend(BlendParameters),
    PerlinNoise(PerlinNoiseParameters),
    Rgb(RgbParameters),
    Grayscale(GrayscaleParameters),
    Ramp(RampParameters),
    NormalMap(NormalMapParameters),
    Image { path: std::path::PathBuf },
    Output { output_type: OutputType },
}

impl Operator {
    /// Returns whether an operator can use external data.
    pub fn external_data(&self) -> bool {
        match self {
            Operator::Image { .. } => true,
            _ => false,
        }
    }

    pub fn inputs(&self) -> HashMap<String, OperatorType> {
        match self {
            Self::Blend(..) => hashmap! {
                "background".to_string() => OperatorType::Polymorphic(0),
                "foreground".to_string() => OperatorType::Polymorphic(0)
            },
            Self::PerlinNoise(..) => HashMap::new(),
            Self::Rgb(..) => HashMap::new(),
            Self::Grayscale(..) => hashmap! {
                "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb),
            },
            Self::Ramp(..) => hashmap! {
                "factor".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
            },
            Self::NormalMap(..) => hashmap! {
                "height".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
            },
            Self::Image { .. } => HashMap::new(),
            Self::Output { output_type } => hashmap! {
                "data".to_string() => match output_type {
                    OutputType::Albedo => OperatorType::Monomorphic(ImageType::Rgb),
                    OutputType::Roughness => OperatorType::Monomorphic(ImageType::Grayscale),
                    OutputType::Normal => OperatorType::Monomorphic(ImageType::Rgb),
                    OutputType::Displacement => OperatorType::Monomorphic(ImageType::Grayscale),
                    OutputType::Metallic => OperatorType::Monomorphic(ImageType::Grayscale),
                    OutputType::Value => OperatorType::Monomorphic(ImageType::Grayscale),
                    OutputType::Rgb => OperatorType::Monomorphic(ImageType::Rgb),
                }
            },
        }
    }

    pub fn outputs(&self) -> HashMap<String, OperatorType> {
        match self {
            Self::Blend(..) => hashmap! {
                "color".to_string() => OperatorType::Polymorphic(0),
            },
            Self::Rgb(..) => hashmap! {
                "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
            },
            Self::PerlinNoise(..) => {
                hashmap! { "noise".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
                }
            }
            Self::Grayscale(..) => hashmap! {
                "value".to_string() => OperatorType::Monomorphic(ImageType::Grayscale)
            },
            Self::Ramp(..) => hashmap! {
                "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
            },
            Self::NormalMap(..) => hashmap! {
                "normal".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
            },
            Self::Image { .. } => {
                hashmap! { "image".to_string() => OperatorType::Monomorphic(ImageType::Rgb) }
            }
            Self::Output { .. } => HashMap::new(),
        }
    }

    pub fn default_name<'a>(&'a self) -> &'static str {
        match self {
            Self::Blend(..) => "blend",
            Self::PerlinNoise(..) => "perlin_noise",
            Self::Rgb(..) => "rgb",
            Self::Grayscale(..) => "grayscale",
            Self::Ramp(..) => "ramp",
            Self::NormalMap(..) => "normal_map",
            Self::Image { .. } => "image",
            Self::Output { .. } => "output",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Blend(..) => "Blend",
            Self::PerlinNoise(..) => "Perlin Noise",
            Self::Rgb(..) => "RGB Color",
            Self::Grayscale(..) => "Grayscale",
            Self::Ramp(..) => "Ramp",
            Self::NormalMap(..) => "Normal Map",
            Self::Image { .. } => "Image",
            Self::Output { .. } => "Output",
        }
    }

    pub fn all_default() -> Vec<Self> {
        vec![
            Self::Blend(BlendParameters::default()),
            Self::PerlinNoise(PerlinNoiseParameters::default()),
            Self::Rgb(RgbParameters::default()),
            Self::Grayscale(GrayscaleParameters::default()),
            Self::Ramp(RampParameters::default()),
            Self::NormalMap(NormalMapParameters::default()),
            Self::Image {
                path: PathBuf::new(),
            },
            Self::Output {
                output_type: OutputType::default(),
            },
        ]
    }

    pub fn is_output(&self) -> bool {
        match self {
            Self::Output { .. } => true,
            _ => false,
        }
    }

    pub fn sockets_by_type_variable(&self, var: TypeVariable) -> Vec<String> {
        self.inputs()
            .iter()
            .chain(self.outputs().iter())
            .filter(|(_, t)| **t == OperatorType::Polymorphic(var))
            .map(|x| x.0.to_owned())
            .collect()
    }
}

impl Parameters for Operator {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
        match self {
            Self::Blend(p) => p.set_parameter(field, data),
            Self::PerlinNoise(p) => p.set_parameter(field, data),
            Self::Rgb(p) => p.set_parameter(field, data),
            Self::Grayscale(p) => p.set_parameter(field, data),
            Self::Ramp(p) => p.set_parameter(field, data),
            Self::NormalMap(p) => p.set_parameter(field, data),

            Self::Image { path } => {
                *path = PathBuf::from_data(data);
            }

            Self::Output { output_type } => *output_type = OutputType::from_data(data),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Instruction {
    Execute(Resource, Operator),
    Move(Resource, Resource),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ImageType {
    Grayscale,
    Rgb,
}

impl Default for ImageType {
    fn default() -> Self {
        ImageType::Grayscale
    }
}

impl ImageType {
    pub fn gpu_bytes_per_pixel(self) -> u8 {
        match self {
            Self::Rgb => 8,
            Self::Grayscale => 4,
        }
    }
}

#[repr(C)]
#[derive(
    PartialEq,
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
pub enum OutputType {
    Albedo,
    Roughness,
    Normal,
    Displacement,
    Metallic,
    Value,
    Rgb,
}

impl Default for OutputType {
    fn default() -> Self {
        OutputType::Value
    }
}

pub type TypeVariable = u8;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum OperatorType {
    Monomorphic(ImageType),
    Polymorphic(TypeVariable),
}

impl OperatorType {
    pub fn monomorphic(self) -> Option<ImageType> {
        match self {
            Self::Monomorphic(ty) => Some(ty),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resource {
    scheme: String,
    resource_path: PathBuf,
    fragment: Option<String>,
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(frag) = &self.fragment {
            write!(
                f,
                "{}:{}:{}",
                self.scheme,
                self.resource_path.to_str().unwrap(),
                frag
            )
        } else {
            write!(
                f,
                "{}:{}",
                self.scheme,
                self.resource_path.to_str().unwrap()
            )
        }
    }
}

impl std::convert::TryFrom<&str> for Resource {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let pieces: Vec<&str> = value.split(':').collect();

        let scheme = (*pieces
            .get(0)
            .ok_or("Missing schema in resource identifier")?)
        .to_string();
        let resource_path =
            PathBuf::from(pieces.get(1).ok_or("Missing path in resource identifier")?);
        let fragment = pieces.get(2).map(|x| (*x).to_string());

        Ok(Resource {
            scheme,
            resource_path,
            fragment,
        })
    }
}

impl Resource {
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_ref().map(|x| x.as_ref())
    }

    pub fn extend_fragment(&self, fragment: &str) -> Self {
        let mut new = self.clone();
        new.fragment = Some(fragment.to_string());
        new
    }

    pub fn drop_fragment(&self) -> Self {
        let mut new = self.clone();
        new.fragment = None;
        new
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn path(&self) -> &Path {
        &self.resource_path
    }

    pub fn unregistered_node() -> Resource {
        Resource {
            scheme: "node".to_string(),
            resource_path: PathBuf::from("__unregistered__"),
            fragment: None,
        }
    }

    pub fn is_fragment_of(&self, other: &Resource) -> bool {
        other.scheme == self.scheme && other.resource_path == self.resource_path
    }
}

/// Events concerning node operation triggered by the user
#[derive(Debug)]
pub enum UserNodeEvent {
    NewNode(Operator),
    RemoveNode(Resource),
    ConnectSockets(Resource, Resource),
    DisconnectSinkSocket(Resource),
    ParameterChange(Resource, &'static str, Vec<u8>),
    PositionNode(Resource, (i32, i32)),
    ForceRecompute,
}

#[derive(Debug)]
pub enum GraphEvent {
    NodeAdded(Resource, Operator, Option<(i32, i32)>),
    NodeRemoved(Resource),
    ConnectedSockets(Resource, Resource),
    DisconnectedSockets(Resource, Resource),
    Recomputed(Vec<Instruction>),
    SocketMonomorphized(Resource, ImageType),
    SocketDemonomorphized(Resource),
    OutputRemoved(Resource, OutputType),
    Cleared,
}

#[derive(Debug)]
pub enum UserRenderEvent {
    Rotate(u64, f32, f32),
    Pan(u64, f32, f32),
    Zoom(u64, f32),
    LightMove(u64, f32, f32),
    ChannelChange2D(u64, RenderChannel),
}

pub type ChannelSpec = (Resource, ImageChannel);

#[derive(Debug)]
pub enum ExportSpec {
    RGBA([ChannelSpec; 4]),
    RGB([ChannelSpec; 3]),
    Grayscale(ChannelSpec),
}

#[derive(Debug)]
pub enum UserIOEvent {
    ExportImage(ExportSpec, PathBuf),
    RequestExport(Option<Vec<(Resource, ImageType)>>),
    OpenSurface(PathBuf),
    SaveSurface(PathBuf),
    NewSurface,
    Quit,
}

#[derive(Debug)]
pub struct WindowHandle {
    raw: raw_window_handle::RawWindowHandle,
}

impl WindowHandle {
    pub fn new(handle: raw_window_handle::RawWindowHandle) -> Self {
        WindowHandle { raw: handle }
    }
}

unsafe impl raw_window_handle::HasRawWindowHandle for WindowHandle {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.raw
    }
}

unsafe impl Sync for WindowHandle {}
unsafe impl Send for WindowHandle {}

#[derive(Debug)]
pub enum ComputeEvent {
    OutputReady(
        Resource,
        crate::gpu::BrokerImage,
        crate::gpu::Layout,
        crate::gpu::Access,
        OutputType,
    ),
    ThumbnailGenerated(Resource, Vec<u8>),
}

#[derive(Debug, Clone, Copy)]
pub enum RendererType {
    Renderer3D,
    Renderer2D,
}

#[derive(Debug, Clone, Copy)]
pub enum RenderChannel {
    Displacement,
    Albedo,
    Normal,
    Roughness,
    Metallic,
}

#[derive(Debug, Display, Clone, Copy)]
pub enum ImageChannel {
    R,
    G,
    B,
    A,
}

impl ImageChannel {
    pub fn channel_index(&self) -> usize {
        match self {
            Self::R => 0,
            Self::G => 1,
            Self::B => 2,
            Self::A => 3,
        }
    }
}

#[derive(Debug)]
pub enum UIEvent {
    RendererAdded(u64, WindowHandle, u32, u32, RendererType),
    RendererRedraw(u64),
    RendererResize(u64, u32, u32),
    RendererRemoved(u64),
}

#[derive(Debug)]
pub enum Lang {
    UserNodeEvent(UserNodeEvent),
    UserRenderEvent(UserRenderEvent),
    UserIOEvent(UserIOEvent),
    UIEvent(UIEvent),
    GraphEvent(GraphEvent),
    ComputeEvent(ComputeEvent),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn test_resource_parse_node() {
        let x = Resource::try_from("node:/foo/bar-01").unwrap();
        assert_eq!(x.fragment, None);
        assert_eq!(x.scheme, "node");
        assert_eq!(x.resource_path, PathBuf::from("/foo/bar-01"));
    }

    #[test]
    fn test_resource_parse_node_socket() {
        // simple
        let x = Resource::try_from("node:/foo:socket_in").unwrap();
        assert_eq!(x.fragment, Some("socket_in".to_string()));
        assert_eq!(x.scheme, "node");
        assert_eq!(x.resource_path, PathBuf::from("/foo"));

        // in nested node
        let x = Resource::try_from("node:/foo/bar-01:socket").unwrap();
        assert_eq!(x.fragment, Some("socket".to_string()));
        assert_eq!(x.scheme, "node");
        assert_eq!(x.resource_path, PathBuf::from("/foo/bar-01"));
    }

    #[test]
    fn test_resource_display() {
        let r = Resource {
            scheme: "node".to_string(),
            resource_path: PathBuf::from("/foo/bar"),
            fragment: Some("socket".to_string()),
        };

        assert_eq!(format!("{}", r), "node:/foo/bar:socket");
    }
}
