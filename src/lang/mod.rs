use crate::compute::shaders::{OperatorShader, Shader, Uniforms};
pub mod operators;
pub mod parameters;
pub mod resource;
pub mod socketed;

use enum_dispatch::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::*;
use strum_macros::*;
use surfacelab_derive::*;

pub use operators::*;
pub use parameters::*;
pub use resource::*;
pub use socketed::*;

#[enum_dispatch(Socketed, Parameters, Uniforms, Shader, OperatorParamBox)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AtomicOperator {
    Blend,
    PerlinNoise,
    Rgb,
    Grayscale,
    Ramp,
    NormalMap,
    Image,
    Output,
}

impl AtomicOperator {
    /// Returns whether an operator can use external data.
    pub fn external_data(&self) -> bool {
        match self {
            Self::Image { .. } => true,
            _ => false,
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
            Self::Output(Output::default()),
        ]
    }

    pub fn is_output(&self) -> bool {
        match self {
            Self::Output { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexOperator {
    pub graph: Resource,
    title: String,
    pub inputs: HashMap<String, OperatorType>,
    pub outputs: HashMap<String, (OperatorType, Resource)>,
    pub substitutions: HashMap<String, ParamSubstitution>,
}

impl ComplexOperator {
    pub fn new(graph: Resource) -> Self {
        ComplexOperator {
            title: graph
                .file()
                .map(|x| x.to_string())
                .unwrap_or("Unnamed Graph".to_string()),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            graph,
            substitutions: HashMap::new(),
        }
    }
}

impl Socketed for ComplexOperator {
    fn inputs(&self) -> HashMap<String, OperatorType> {
        self.inputs.clone()
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        self.outputs.iter().map(|(k, v)| (k.clone(), v.0)).collect()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn default_name<'a>(&'a self) -> &str {
        self.graph.file().unwrap_or("unknown")
    }
}

#[enum_dispatch(Socketed, OperatorParamBox)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operator {
    AtomicOperator(AtomicOperator),
    ComplexOperator(ComplexOperator),
}

impl Operator {
    pub fn to_atomic(&self) -> Option<&AtomicOperator> {
        match self {
            Self::AtomicOperator(op) => Some(op),
            _ => None,
        }
    }

    pub fn external_data(&self) -> bool {
        match self {
            Self::AtomicOperator(op) => op.external_data(),
            _ => false,
        }
    }
}

impl Parameters for Operator {
    fn set_parameter(&mut self, field: &str, data: &[u8]) {
        match self {
            Self::AtomicOperator(op) => op.set_parameter(field, data),
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub enum Instruction {
    Execute(Resource, AtomicOperator),
    Call(Resource, ComplexOperator),
    Move(Resource, Resource),
    Copy(Resource, Resource),
    Thumbnail(Resource),
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

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Serialize, Deserialize)]
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
    NewNode(Resource, Operator),
    RemoveNode(Resource),
    ConnectSockets(Resource, Resource),
    DisconnectSinkSocket(Resource),
    ParameterChange(Resource, Vec<u8>),
    PositionNode(Resource, (i32, i32)),
    RenameNode(Resource, Resource),
    OutputSizeChange(Resource, i32),
    OutputSizeAbsolute(Resource, bool),
}

#[derive(Debug)]
pub enum UserGraphEvent {
    AddGraph(String),
    ChangeGraph(Resource),
    ExposeParameter(Resource, String, String, GraphParameterType, Vec<u8>),
    ConcealParameter(Resource, String),
}

#[derive(Debug)]
pub enum GraphEvent {
    GraphAdded(Resource),
    NodeAdded(Resource, Operator, Option<(i32, i32)>, u32),
    NodeRemoved(Resource),
    NodeRenamed(Resource, Resource),
    NodeResized(Resource, u32),
    ConnectedSockets(Resource, Resource),
    DisconnectedSockets(Resource, Resource),
    Relinearized(Resource, Vec<Instruction>),
    Recompute(Resource),
    SocketMonomorphized(Resource, ImageType),
    SocketDemonomorphized(Resource),
    OutputRemoved(Resource, OutputType),
    Report(
        Vec<(Resource, Operator, (i32, i32))>,
        Vec<(Resource, Resource)>,
    ),
    Cleared,
}

pub type RendererID = u64;

#[derive(Debug)]
pub enum UserRenderEvent {
    Rotate(RendererID, f32, f32),
    Pan(RendererID, f32, f32),
    Zoom(RendererID, f32),
    LightMove(RendererID, f32, f32),
    ChannelChange2D(RendererID, RenderChannel),
    DisplacementAmount(RendererID, f32),
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
    ExportImage(ExportSpec, u32, PathBuf),
    RequestExport(Option<Vec<(Resource, ImageType)>>),
    OpenSurface(PathBuf),
    SaveSurface(PathBuf),
    SetParentSize(u32),
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
        u32,
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
    RendererAdded(RendererID, WindowHandle, u32, u32, RendererType),
    RendererRedraw(RendererID),
    RendererResize(RendererID, u32, u32),
    RendererRemoved(RendererID),
}

#[derive(Debug)]
pub enum Lang {
    UserNodeEvent(UserNodeEvent),
    UserGraphEvent(UserGraphEvent),
    UserRenderEvent(UserRenderEvent),
    UserIOEvent(UserIOEvent),
    UIEvent(UIEvent),
    GraphEvent(GraphEvent),
    ComputeEvent(ComputeEvent),
}
