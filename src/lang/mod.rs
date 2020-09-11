use crate::compute::shaders::{OperatorShader, Shader, Uniforms};
pub mod operators;
pub mod parameters;
pub mod resource;
pub mod socketed;

use enum_dispatch::*;
use enumset::EnumSetType;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::*;
use strum_macros::*;
use surfacelab_derive::*;
use zerocopy::AsBytes;

pub use operators::*;
pub use parameters::*;
pub use resource::*;
pub use socketed::*;

#[enum_dispatch(Socketed, Parameters, Uniforms, Shader, OperatorParamBox)]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComplexOperator {
    pub graph: Resource,
    title: String,
    pub inputs: HashMap<String, OperatorType>,
    pub outputs: HashMap<String, (OperatorType, Resource)>,
    pub parameters: HashMap<String, ParamSubstitution>,
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
            parameters: HashMap::new(),
        }
    }
}

impl Parameters for ComplexOperator {
    fn set_parameter(&mut self, field: &str, data: &[u8]) {
        if let Some(p) = self.parameters.get_mut(field) {
            p.set_value(data);
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

#[enum_dispatch(Socketed, Parameters)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

    pub fn is_graph(&self, graph: &Resource) -> bool {
        match self {
            Operator::AtomicOperator(_) => false,
            Operator::ComplexOperator(o) => &o.graph == graph,
        }
    }

    pub fn external_data(&self) -> bool {
        match self {
            Self::AtomicOperator(op) => op.external_data(),
            _ => false,
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

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
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

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
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
    PositionNode(Resource, (f64, f64)),
    RenameNode(Resource, Resource),
    OutputSizeChange(Resource, i32),
    OutputSizeAbsolute(Resource, bool),
}

#[derive(Debug)]
pub enum UserGraphEvent {
    AddGraph,
    ChangeGraph(Resource),
    RenameGraph(Resource, Resource),
    ExposeParameter(Resource, String, String, Control),
    ConcealParameter(Resource, String),
    RefieldParameter(Resource, String, String),
    RetitleParameter(Resource, String, String),
}

#[derive(Debug)]
pub enum GraphEvent {
    GraphAdded(Resource),
    GraphRenamed(Resource, Resource),
    NodeAdded(
        Resource,
        Operator,
        ParamBoxDescription<Field>,
        Option<(f64, f64)>,
        u32,
    ),
    NodeRemoved(Resource),
    NodeRenamed(Resource, Resource),
    NodeResized(Resource, u32),
    ConnectedSockets(Resource, Resource),
    DisconnectedSockets(Resource, Resource),
    Relinearized(Resource, Vec<Instruction>),
    Recompute(Resource),
    SocketMonomorphized(Resource, ImageType),
    SocketDemonomorphized(Resource),
    ParameterExposed(Resource, GraphParameter),
    ParameterConcealed(Resource, String),
    OutputRemoved(Resource, OutputType),
    Cleared,
}

pub type RendererID = u64;

#[derive(AsBytes, Debug)]
#[repr(u32)]
pub enum LightType {
    PointLight = 0,
    SunLight = 1,
}

#[derive(Debug)]
pub enum UserRenderEvent {
    Rotate(RendererID, f32, f32),
    Pan(RendererID, f32, f32),
    Zoom(RendererID, f32),
    LightMove(RendererID, f32, f32),
    ChannelChange2D(RendererID, MaterialChannel),
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
pub enum ComputeEvent {
    OutputReady(
        Resource,
        crate::gpu::BrokerImage,
        crate::gpu::Layout,
        crate::gpu::Access,
        u32,
        OutputType,
    ),
    ThumbnailCreated(Resource, crate::gpu::BrokerImageView),
    ThumbnailDestroyed(Resource),
    ThumbnailUpdated(Resource),
}

#[derive(Debug, Clone, Copy)]
pub enum RendererType {
    Renderer3D,
    Renderer2D,
}

#[derive(EnumSetType, Debug)]
pub enum MaterialChannel {
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
    RendererRequested(RendererID, (u32, u32), (u32, u32), RendererType),
    RendererRedraw(RendererID),
    RendererResize(RendererID, u32, u32),
    RendererRemoved(RendererID),
}

#[derive(Debug)]
pub enum RenderEvent {
    RendererAdded(RendererID, crate::gpu::BrokerImageView),
    RendererRedrawn(RendererID),
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
    RenderEvent(RenderEvent),
}
