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

/// Atomic Operators are operators that can not be decomposed into smaller
/// parts.
#[enum_dispatch(Socketed, Parameters, Uniforms, Shader, OperatorParamBox)]
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum AtomicOperator {
    Blend,
    BlendMasked,
    PerlinNoise,
    Rgb,
    Range,
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

    /// A vector of all atomic operators with their default parameters. Useful
    /// for frontends to present a list of all operators.
    pub fn all_default() -> Vec<Self> {
        vec![
            Self::Blend(Blend::default()),
            Self::BlendMasked(BlendMasked::default()),
            Self::PerlinNoise(PerlinNoise::default()),
            Self::Rgb(Rgb::default()),
            Self::Range(Range::default()),
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

/// Complex operators are operators that are created through another graph or
/// layer stack with inputs and outputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComplexOperator {
    /// Graph giving rise to the operator
    pub graph: Resource<Graph>,

    /// Human readable name
    pub title: String,

    /// Input sockets with their types and internal nodes
    pub inputs: HashMap<String, (OperatorType, Resource<Node>)>,

    /// Output sockets with their types and internal nodes
    pub outputs: HashMap<String, (OperatorType, Resource<Node>)>,

    /// Parameter substitutions performed on this operator
    pub parameters: HashMap<String, ParamSubstitution>,
}

impl ComplexOperator {
    /// Create a complex operator from a graph resource
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

    /// Return hash of all parameter substitutions. For compute component
    /// results caching.
    pub fn parameter_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        for param in self.parameters.values() {
            param.hash(&mut hasher)
        }
        hasher.finish()
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

/// Any operator, complex or atomic.
#[enum_dispatch(Socketed, Parameters)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Operator {
    AtomicOperator(AtomicOperator),
    ComplexOperator(ComplexOperator),
}

impl Operator {
    /// Cast to atomic operator if possible.
    pub fn as_atomic(&self) -> Option<&AtomicOperator> {
        match self {
            Self::AtomicOperator(op) => Some(op),
            _ => None,
        }
    }

    /// Return true if and only if this operator is a complex operator for the
    /// given graph.
    pub fn is_graph(&self, graph: &Resource<Graph>) -> bool {
        match self {
            Operator::AtomicOperator(_) => false,
            Operator::ComplexOperator(o) => &o.graph == graph,
        }
    }

    /// A mask is any operator that has one input or less, and some number of
    /// outputs greater than 0 that can be interpreted as grayscale images.
    pub fn is_mask(&self) -> bool {
        self.inputs().len() <= 1
            && self.outputs().values().any(|t| match t {
                OperatorType::Monomorphic(ImageType::Grayscale) => true,
                _ => false,
            })
    }
}

/// A linearization is an executable form of a graph, typically some sort of
/// topological sort, that can be executed by the compute component.
pub type Linearization = Vec<Instruction>;

/// Use points describe when something was used or created in a linearization,
/// indexed by steps.
#[derive(Debug, Clone)]
pub struct UsePoint {
    pub last: usize,
    pub creation: usize,
}

/// Structure holding use point information for each node.
pub type UsePoints = Vec<(Resource<Node>, UsePoint)>;

/// A force point is a node that has to be explicitly recomputed on request,
/// overriding any caching schemes.
pub type ForcePoints = Vec<Resource<Node>>;

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

/// Enum describing the types of images in the system. Images can be either RGB
/// or Grayscale. Without further information as to where this is used, no
/// assumptions should be made about representation!
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
    /// Images are grayscale by default
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

/// Types of outputs. Possible values include PBR channels as well as
/// generalized formats.
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

/// Type variables are internally represented as `u8`. Therefore there can only
/// be 256 type variables for each operator.
pub type TypeVariable = u8;

/// The OperatorType describes types used by an operator. They can be either
/// monomorphic with a well defined image type, or polymorphic, with some fixed
/// type variable that is specific to the operator. Multiple polymorphic types
/// with the same type variable used in the same operator always unify together.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub enum OperatorType {
    Monomorphic(ImageType),
    Polymorphic(TypeVariable),
}

impl OperatorType {
    /// Get the monomorphic type if possible.
    pub fn monomorphic(self) -> Option<ImageType> {
        match self {
            Self::Monomorphic(ty) => Some(ty),
            _ => None,
        }
    }
}

/// Events concerning node operation triggered by the user, such as adding,
/// removing, etc.
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

/// Events concerning graph operation triggered by the user, such as adding,
/// removing, etc.
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

/// Events concerning graphs, not directly coming from user input.
#[derive(Debug)]
pub enum GraphEvent {
    GraphAdded(Resource<Graph>),
    GraphRenamed(Resource<Graph>, Resource<Graph>),
    NodeAdded(
        Resource<Node>,
        Operator,
        ParamBoxDescription<MessageWriters>,
        Option<(f64, f64)>,
        u32,
    ),
    OutputSocketAdded(Resource<Socket>, OperatorType, bool, u32),
    NodeRemoved(Resource<Node>),
    NodeRenamed(Resource<Node>, Resource<Node>),
    NodeResized(Resource<Node>, u32),
    ComplexOperatorUpdated(
        Resource<Node>,
        ComplexOperator,
        ParamBoxDescription<MessageWriters>,
    ),
    ConnectedSockets(Resource<Socket>, Resource<Socket>),
    DisconnectedSockets(Resource<Socket>, Resource<Socket>),
    Relinearized(Resource<Graph>, Linearization, UsePoints, ForcePoints),
    Recompute(Resource<Graph>),
    SocketMonomorphized(Resource<Socket>, ImageType),
    SocketDemonomorphized(Resource<Socket>),
    ParameterExposed(Resource<Graph>, GraphParameter),
    ParameterConcealed(Resource<Graph>, String),
    OutputRemoved(Resource<Node>, OutputType),
    Cleared,
}

/// Layers come in two types, as far as the user is concerned, Fill and FX.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayerType {
    Fill,
    Fx,
}

/// Events concerning layer operation triggered by the user, such as adding,
/// reordering, etc.
#[derive(Debug)]
pub enum UserLayersEvent {
    AddLayers,
    PushLayer(Resource<Graph>, LayerType, Operator),
    PushMask(Resource<Node>, Operator),
    RemoveLayer(Resource<Node>),
    MoveUp(Resource<Node>),
    MoveDown(Resource<Node>),
    SetOutput(Resource<Node>, MaterialChannel, usize, bool),
    SetInput(Resource<Socket>, MaterialChannel),
    SetOpacity(Resource<Node>, f32),
    SetBlendMode(Resource<Node>, BlendMode),
    SetTitle(Resource<Node>, String),
    SetEnabled(Resource<Node>, bool),
    Convert(Resource<Graph>),
}

/// Events concerning layers, not directly coming from user input.
#[derive(Debug)]
pub enum LayersEvent {
    LayersAdded(Resource<Graph>, u32),
    LayerPushed(
        Resource<Node>,
        LayerType,
        String,
        Operator,
        BlendMode,
        f32,
        ParamBoxDescription<MessageWriters>,
        u32,
    ),
    LayerRemoved(Resource<Node>),
    MaskPushed(
        Resource<Node>,
        Resource<Node>,
        String,
        Operator,
        BlendMode,
        f32,
        ParamBoxDescription<MessageWriters>,
        u32,
    ),
    MovedUp(Resource<Node>),
    MovedDown(Resource<Node>),
}

/// Events concerning surfaces, not directly coming from user input.
#[derive(Debug)]
pub enum SurfaceEvent {
    ExportImage(ExportSpec, u32, PathBuf),
    ExportSpecLoaded(String, ExportSpec),
}

/// Renderers are indexed by an ID, internally merely a `u64`.
pub type RendererID = u64;

/// Light types supported by renderers.
#[derive(AsBytes, Copy, Clone, Debug, ParameterField)]
#[repr(u32)]
pub enum LightType {
    PointLight = 0,
    SunLight = 1,
}

/// Object types supported by the SDF 3D renderer
#[derive(AsBytes, Copy, Clone, Debug, ParameterField)]
#[repr(u32)]
pub enum ObjectType {
    Plane = 0,
    Cube = 1,
    Sphere = 2,
    Cylinder = 3,
}

/// Events concerning renderer operation triggered by the user
#[derive(Debug)]
pub enum UserRenderEvent {
    Rotate(RendererID, f32, f32),
    Pan(RendererID, f32, f32),
    Zoom(RendererID, f32),
    LightMove(RendererID, f32, f32),
    ChannelChange2D(RendererID, MaterialChannel),
    DisplacementAmount(RendererID, f32),
    TextureScale(RendererID, f32),
    EnvironmentStrength(RendererID, f32),
    EnvironmentBlur(RendererID, f32),
    LightType(RendererID, LightType),
    LightStrength(RendererID, f32),
    FogStrength(RendererID, f32),
    FocalLength(RendererID, f32),
    ApertureSize(RendererID, f32),
    FocalDistance(RendererID, f32),
    SetShadow(RendererID, ParameterBool),
    SetAO(RendererID, ParameterBool),
    LoadHDRI(RendererID, PathBuf),
    ObjectType(RendererID, ObjectType),
    SampleCount(RendererID, u32),
}

/// Supported color spaces for (external) images.
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

/// Description of an image channel by naming the socket at which the image
/// resides and the channel specifically.
pub type ChannelSpec = (Resource<Socket>, ImageChannel);

/// Export specifications, constructed as an appropriate set of channel
/// specifications. Exposes a Builder-esque interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportSpec {
    RGBA([ChannelSpec; 4]),
    RGB([ChannelSpec; 3]),
    Grayscale(ChannelSpec),
}

impl ExportSpec {
    /// Convert to another image type.
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

    /// Set existence of alpha channel.
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

    /// Set red channel. Will set grayscale on grayscale specs.
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

    /// Set green channel if available
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

    /// Set blue channel if available
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

    /// Set alpha channel if available
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

/// IO related events triggered by the user
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

/// Events triggered during computation or setup thereof
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

/// Type of renderer.
#[derive(Debug, Clone, Copy)]
pub enum RendererType {
    Renderer3D,
    Renderer2D,
}

/// Supported PBR material channels
#[allow(clippy::derive_hash_xor_eq)]
#[derive(EnumSetType, EnumIter, Debug, Hash, Serialize, Deserialize, Display)]
pub enum MaterialChannel {
    Displacement,
    Albedo,
    Normal,
    Roughness,
    Metallic,
}

impl MaterialChannel {
    /// Obtain output type from a material channel.
    pub fn to_output_type(self) -> OutputType {
        match self {
            MaterialChannel::Displacement => OutputType::Displacement,
            MaterialChannel::Albedo => OutputType::Albedo,
            MaterialChannel::Normal => OutputType::Normal,
            MaterialChannel::Roughness => OutputType::Roughness,
            MaterialChannel::Metallic => OutputType::Metallic,
        }
    }

    /// Obtain image type from a material channel.
    pub fn to_image_type(self) -> ImageType {
        match self {
            MaterialChannel::Displacement => ImageType::Grayscale,
            MaterialChannel::Albedo => ImageType::Rgb,
            MaterialChannel::Normal => ImageType::Rgb,
            MaterialChannel::Roughness => ImageType::Grayscale,
            MaterialChannel::Metallic => ImageType::Grayscale,
        }
    }

    /// Short name of a material channel.
    pub fn short_name(&self) -> &'static str {
        match self {
            MaterialChannel::Albedo => "col",
            MaterialChannel::Roughness => "rgh",
            MaterialChannel::Metallic => "met",
            MaterialChannel::Normal => "nor",
            MaterialChannel::Displacement => "dsp",
        }
    }
}

#[derive(Debug, Display, Clone, Copy, Serialize, Deserialize)]
pub enum ImageChannel {
    R,
    G,
    B,
    A,
}

impl ImageChannel {
    /// Image channel index by RGBA ordering.
    pub fn channel_index(&self) -> usize {
        match self {
            Self::R => 0,
            Self::G => 1,
            Self::B => 2,
            Self::A => 3,
        }
    }
}

/// Events stemming from UI operation, not directly triggered by the user.
#[derive(Debug)]
pub enum UIEvent {
    RendererRequested(RendererID, (u32, u32), (u32, u32), RendererType),
    RendererRedraw(RendererID),
    RendererResize(RendererID, u32, u32),
    RendererRemoved(RendererID),
}

/// Events from the renderer
#[derive(Debug)]
pub enum RenderEvent {
    RendererAdded(RendererID, crate::gpu::BrokerImageView),
    RendererRedrawn(RendererID),
}

/// Master event type used by the application bus. This defines the common
/// language of the application.
#[derive(Debug)]
pub enum Lang {
    UserNodeEvent(UserNodeEvent),
    UserGraphEvent(UserGraphEvent),
    UserLayersEvent(UserLayersEvent),
    UserRenderEvent(UserRenderEvent),
    UserIOEvent(UserIOEvent),
    UIEvent(UIEvent),
    GraphEvent(GraphEvent),
    LayersEvent(LayersEvent),
    SurfaceEvent(SurfaceEvent),
    ComputeEvent(ComputeEvent),
    RenderEvent(RenderEvent),
}
