pub mod resource;
pub mod parameters;
pub mod socketed;
pub mod operators;

use maplit::hashmap;
use std::collections::HashMap;
use std::path::*;
use strum_macros::*;
use serde_derive::{Deserialize, Serialize};
use surfacelab_derive::*;
use enum_dispatch::*;

pub use resource::*;
pub use parameters::*;
pub use socketed::*;
pub use operators::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operator {
    Blend(Blend),
    PerlinNoise(PerlinNoise),
    Rgb(Rgb),
    Grayscale(Grayscale),
    Ramp(Ramp),
    NormalMap(NormalMap),
    Image(Image),
    Output(Output),
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
            Self::Image(..) => HashMap::new(),
            Self::Output(Output { output_type }) => hashmap! {
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
            Self::Blend(Blend::default()),
            Self::PerlinNoise(PerlinNoise::default()),
            Self::Rgb(Rgb::default()),
            Self::Grayscale(Grayscale::default()),
            Self::Ramp(Ramp::default()),
            Self::NormalMap(NormalMap::default()),
            Self::Image(Image::default()),
            Self::Output(Output::default())
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
            Self::Image(p) => p.set_parameter(field, data),
            Self::Output(p) => p.set_parameter(field, data),
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
