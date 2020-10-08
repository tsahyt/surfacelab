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
    Input,
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
            Self::Input(Input::default()),
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
    pub graph: Resource<Graph>,
    pub title: String,
    pub inputs: HashMap<String, (OperatorType, Resource<Node>)>,
    pub outputs: HashMap<String, (OperatorType, Resource<Node>)>,
    pub parameters: HashMap<String, ParamSubstitution>,
}

impl ComplexOperator {
    pub fn new(graph: Resource<Graph>) -> Self {
        ComplexOperator {
            title: graph
                .file()
                .map(|x| x.to_string())
                .unwrap_or_else(|| "Unnamed Graph".to_string()),
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
        self.inputs.iter().map(|(k, v)| (k.clone(), v.0)).collect()
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        self.outputs.iter().map(|(k, v)| (k.clone(), v.0)).collect()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn default_name(&self) -> &str {
        self.graph.file().unwrap_or("unknown")
    }

    /// Complex Operators have data external to them, i.e. their outputs sockets
    /// are *copied to*, like Images or Inputs.
    fn external_data(&self) -> bool {
        true
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

    pub fn is_graph(&self, graph: &Resource<Graph>) -> bool {
        match self {
            Operator::AtomicOperator(_) => false,
            Operator::ComplexOperator(o) => &o.graph == graph,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Instruction {
    Execute(Resource<Node>, AtomicOperator),
    Call(Resource<Node>, ComplexOperator),
    Move(Resource<Socket>, Resource<Socket>),
    Copy(Resource<Socket>, Resource<Socket>),
    Thumbnail(Resource<Socket>),
}

impl Instruction {
    pub fn is_execution_step(&self) -> bool {
        match self {
            Self::Execute(..) | Self::Call(..) => true,
            _ => false,
        }
    }
}

#[repr(C)]
#[derive(
    AsBytes,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    Hash,
    ParameterField,
    EnumVariantNames,
)]
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
    NewNode(Resource<Graph>, Operator, (f64, f64)),
    RemoveNode(Resource<Node>),
    ConnectSockets(Resource<Socket>, Resource<Socket>),
    DisconnectSinkSocket(Resource<Socket>),
    ParameterChange(Resource<Param>, Vec<u8>),
    PositionNode(Resource<Node>, (f64, f64)),
    RenameNode(Resource<Node>, Resource<Node>),
    OutputSizeChange(Resource<Node>, i32),
    OutputSizeAbsolute(Resource<Node>, bool),
}

#[derive(Debug)]
pub enum UserGraphEvent {
    AddGraph,
    ChangeGraph(Resource<Graph>),
    RenameGraph(Resource<Graph>, Resource<Graph>),
    ExposeParameter(Resource<Param>, String, String, Control),
    ConcealParameter(Resource<Graph>, String),
    RefieldParameter(Resource<Graph>, String, String),
    RetitleParameter(Resource<Graph>, String, String),
}

#[derive(Debug)]
pub enum GraphEvent {
    GraphAdded(Resource<Graph>),
    GraphRenamed(Resource<Graph>, Resource<Graph>),
    NodeAdded(
        Resource<Node>,
        Operator,
        ParamBoxDescription<Field>,
        Option<(f64, f64)>,
        u32,
    ),
    NodeRemoved(Resource<Node>),
    NodeRenamed(Resource<Node>, Resource<Node>),
    NodeResized(Resource<Node>, u32),
    ComplexOperatorUpdated(Resource<Node>, ComplexOperator, ParamBoxDescription<Field>),
    ConnectedSockets(Resource<Socket>, Resource<Socket>),
    DisconnectedSockets(Resource<Socket>, Resource<Socket>),
    Relinearized(
        Resource<Graph>,
        Vec<Instruction>,
        Vec<(Resource<Node>, usize)>,
    ),
    Recompute(Resource<Graph>),
    SocketMonomorphized(Resource<Socket>, ImageType),
    SocketDemonomorphized(Resource<Socket>),
    ParameterExposed(Resource<Graph>, GraphParameter),
    ParameterConcealed(Resource<Graph>, String),
    OutputRemoved(Resource<Node>, OutputType),
    Cleared,
}

#[derive(Debug)]
pub enum SurfaceEvent {
    ExportImage(ExportSpec, u32, PathBuf),
}

pub type RendererID = u64;

#[derive(AsBytes, Copy, Clone, Debug, ParameterField)]
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
    TextureScale(RendererID, f32),
    LightType(RendererID, LightType),
    LightStrength(RendererID, f32),
    SetShadow(RendererID, ParameterBool),
    SetAO(RendererID, ParameterBool),
}

#[repr(u32)]
#[derive(
    Debug,
    EnumVariantNames,
    ParameterField,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Hash,
)]
pub enum ColorSpace {
    Srgb,
    Linear,
}

pub type ChannelSpec = (Resource<Socket>, ImageChannel);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportSpec {
    RGBA([ChannelSpec; 4]),
    RGB([ChannelSpec; 3]),
    Grayscale(ChannelSpec),
}

impl ExportSpec {
    pub fn image_type(self, ty: ImageType) -> Self {
        match &self {
            ExportSpec::RGBA(cs) => match ty {
                ImageType::Rgb => self,
                ImageType::Grayscale => ExportSpec::Grayscale(cs[0].clone()),
            },
            ExportSpec::RGB(cs) => match ty {
                ImageType::Rgb => self,
                ImageType::Grayscale => ExportSpec::Grayscale(cs[0].clone()),
            },
            ExportSpec::Grayscale(c) => match ty {
                ImageType::Grayscale => self,
                ImageType::Rgb => ExportSpec::RGB([c.clone(), c.clone(), c.clone()]),
            },
        }
    }

    pub fn alpha(self, alpha: bool) -> Self {
        if alpha {
            match &self {
                ExportSpec::RGB(cs) => {
                    ExportSpec::RGBA([cs[0].clone(), cs[1].clone(), cs[2].clone(), cs[2].clone()])
                }
                _ => self,
            }
        } else {
            match &self {
                ExportSpec::RGBA(cs) => {
                    ExportSpec::RGB([cs[0].clone(), cs[1].clone(), cs[2].clone()])
                }
                _ => self,
            }
        }
    }

    pub fn set_r(&mut self, spec: ChannelSpec) {
        match self {
            ExportSpec::RGBA(cs) => {
                cs[0] = spec;
            }
            ExportSpec::RGB(cs) => {
                cs[0] = spec;
            }
            ExportSpec::Grayscale(c) => {
                *c = spec;
            }
        }
    }

    pub fn set_g(&mut self, spec: ChannelSpec) {
        match self {
            ExportSpec::RGBA(cs) => {
                cs[1] = spec;
            }
            ExportSpec::RGB(cs) => {
                cs[1] = spec;
            }
            ExportSpec::Grayscale(_) => {}
        }
    }

    pub fn set_b(&mut self, spec: ChannelSpec) {
        match self {
            ExportSpec::RGBA(cs) => {
                cs[2] = spec;
            }
            ExportSpec::RGB(cs) => {
                cs[2] = spec;
            }
            ExportSpec::Grayscale(_) => {}
        }
    }

    pub fn set_a(&mut self, spec: ChannelSpec) {
        match self {
            ExportSpec::RGBA(cs) => {
                cs[3] = spec;
            }
            ExportSpec::RGB(_) => {}
            ExportSpec::Grayscale(_) => {}
        }
    }
}

#[derive(Debug)]
pub enum UserIOEvent {
    OpenSurface(PathBuf),
    SaveSurface(PathBuf),
    SetParentSize(u32),
    DeclareExport(String, ExportSpec),
    RenameExport(String, String),
    RunExports(PathBuf),
    NewSurface,
    Quit,
}

#[derive(Debug)]
pub enum ComputeEvent {
    OutputReady(
        Resource<Node>,
        crate::gpu::BrokerImage,
        crate::gpu::Layout,
        crate::gpu::Access,
        u32,
        OutputType,
    ),
    SocketCreated(Resource<Socket>, ImageType),
    SocketDestroyed(Resource<Socket>),
    ThumbnailCreated(Resource<Node>, crate::gpu::BrokerImageView),
    ThumbnailDestroyed(Resource<Node>),
    ThumbnailUpdated(Resource<Node>),
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

#[derive(Debug, Display, Clone, Copy, Serialize, Deserialize)]
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
    SurfaceEvent(SurfaceEvent),
    ComputeEvent(ComputeEvent),
    RenderEvent(RenderEvent),
}
