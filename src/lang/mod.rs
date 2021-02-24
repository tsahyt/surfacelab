use crate::compute::shaders::{OperatorPassDescription, Shader, Uniforms};
pub mod operators;
pub mod parameters;
pub mod resource;
pub mod socketed;

use enum_dispatch::*;
use enumset::{EnumSet, EnumSetType};

use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
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
    Checker,
    PerlinNoise,
    Rgb,
    Hsv,
    Range,
    Swizzle,
    Transform,
    Grayscale,
    Ramp,
    NormalMap,
    Image,
    Output,
    Input,
}

impl AtomicOperator {
    /// A vector of all atomic operators with their default parameters. Useful
    /// for frontends to present a list of all operators.
    pub fn all_default() -> Vec<Self> {
        vec![
            Self::Blend(Blend::default()),
            Self::BlendMasked(BlendMasked::default()),
            Self::Checker(Checker::default()),
            Self::PerlinNoise(PerlinNoise::default()),
            Self::Rgb(Rgb::default()),
            Self::Hsv(Hsv::default()),
            Self::Range(Range::default()),
            Self::Swizzle(Swizzle::default()),
            Self::Transform(Transform::default()),
            Self::Grayscale(Grayscale::default()),
            Self::Ramp(Ramp::default()),
            Self::NormalMap(NormalMap::default()),
            Self::Image(Image::default()),
            Self::Output(Output::default()),
            Self::Input(Input::default()),
        ]
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
                OperatorType::Monomorphic(ImageType::Rgb) => false,
                _ => true,
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

/// Get the monomorphic type if possible. Will fail on polymorphic types.
impl TryFrom<OperatorType> for ImageType {
    type Error = &'static str;

    fn try_from(value: OperatorType) -> Result<Self, Self::Error> {
        match value {
            OperatorType::Monomorphic(ty) => Ok(ty),
            _ => Err("Attempted to convert polymorphic operator type to image type"),
        }
    }
}

impl TryFrom<OperatorType> for TypeVariable {
    type Error = &'static str;

    fn try_from(value: OperatorType) -> Result<Self, Self::Error> {
        match value {
            OperatorType::Polymorphic(v) => Ok(v),
            _ => Err("Attempted to get type variable from monomorphic type"),
        }
    }
}

/// Events concerning node operation triggered by the user, such as adding,
/// removing, etc. These events should be treated as unsanitized, since they are
/// user generated.
#[derive(Debug)]
pub enum UserNodeEvent {
    /// The user requests creation of a new node inside a given graph, using this operator, at layout position.
    NewNode(Resource<Graph>, Operator, (f64, f64)),
    /// The user requests the removal of a given node.
    RemoveNode(Resource<Node>),
    /// The user requests a connection between the two sockets.
    ConnectSockets(Resource<Socket>, Resource<Socket>),
    /// The user requests the disconnection of the given sink socket.
    DisconnectSinkSocket(Resource<Socket>),
    /// The user changes the given parameter to the supplied value.
    ParameterChange(Resource<Param>, Vec<u8>),
    /// The user repositions the node to the given coordinates.
    PositionNode(Resource<Node>, (f64, f64)),
    /// The user renames a node from a resource to another resource.
    RenameNode(Resource<Node>, Resource<Node>),
    /// The user changes the output size of the given node
    OutputSizeChange(Resource<Node>, i32),
    /// The user sets the absolute size property of a node
    OutputSizeAbsolute(Resource<Node>, bool),
}

/// Events concerning graph operation triggered by the user, such as adding,
/// removing, etc. These events should be treated as unsanitized, since they are
/// user generated.
#[derive(Debug)]
pub enum UserGraphEvent {
    /// The user adds a new graph.
    AddGraph,
    /// The user changes the *current* graph to operate on
    ChangeGraph(Resource<Graph>),
    /// The user renames a graph from a resource to another resource
    RenameGraph(Resource<Graph>, Resource<Graph>),
    /// The user asks for exposure of a given parameter, where the two strings
    /// represent (in order) the *graph field*, i.e. the field name to be used
    /// for the exposed parameter, and the *title*, i.e. the human readable name
    /// of the parameter. Finally a control is given that should be used in the
    /// graph parameter box.
    ExposeParameter(Resource<Param>, String, String, Control),
    /// The user asks to conceal a parameter in a graph, identified by its graph
    /// field.
    ConcealParameter(Resource<Graph>, String),
    /// The user renames a graph field from a string to a string.
    RefieldParameter(Resource<Graph>, String, String),
    /// The user renames the human readable title of a graph field from a string
    /// to a string.
    RetitleParameter(Resource<Graph>, String, String),
}

/// Events concerning graphs, not directly coming from user input.
#[derive(Debug)]
pub enum GraphEvent {
    /// A graph identified by this resource has been added to the system.
    GraphAdded(Resource<Graph>),
    /// A graph has been renamed from a resource to a resource.
    GraphRenamed(Resource<Graph>, Resource<Graph>),
    /// A node has been added inside of a graph, identified by a resource. The
    /// node uses the supplied operator, and has the parameters given in the
    /// parameter box description. If it has a fixed position, it is given in
    /// the Option. Finally the size of the node's images (in *absolute pixel
    /// count*) is given.
    NodeAdded(
        Resource<Node>,
        Operator,
        ParamBoxDescription<MessageWriters>,
        Option<(f64, f64)>,
        u32,
    ),
    /// An output socket has been created in the system, with a given type. The
    /// boolean denotes whether the socket is associated with external data. The
    /// `u32` denotes the pixel size of the socket.
    OutputSocketAdded(Resource<Socket>, OperatorType, bool, u32),
    /// A node has been removed from the system
    NodeRemoved(Resource<Node>),
    /// A node has been renamed/moved from a resource to a resource.
    NodeRenamed(Resource<Node>, Resource<Node>),
    /// A node has been resized to the new given size.
    NodeResized(Resource<Node>, u32),
    /// A complex operator has been updated in the system, and is now
    /// represented by the given parameters.
    ComplexOperatorUpdated(
        Resource<Node>,
        ComplexOperator,
        ParamBoxDescription<MessageWriters>,
    ),
    /// Two sockets have been connected.
    ConnectedSockets(Resource<Socket>, Resource<Socket>),
    /// Two sockets have been disconnected from each other.
    DisconnectedSockets(Resource<Socket>, Resource<Socket>),
    /// A graph has been relinearized, resulting in the new linearization data
    /// supplied.
    Relinearized(Resource<Graph>, Linearization, UsePoints, ForcePoints),
    /// A graph needs to be recomputed.
    Recompute(Resource<Graph>),
    /// A sockets type has been monomorphized to the given image type.
    SocketMonomorphized(Resource<Socket>, ImageType),
    /// A sockets type is no longer monomorphic.
    SocketDemonomorphized(Resource<Socket>),
    /// A parameter has been exposed for a given graph.
    ParameterExposed(Resource<Graph>, GraphParameter),
    /// A parameter has been concealed for a given graph, identified by its
    /// graph field.
    ParameterConcealed(Resource<Graph>, String),
    /// An output has been removed.
    OutputRemoved(Resource<Node>, OutputType),
    /// *All* graphs have been cleared in the system.
    Cleared,
}

/// Layers come in two types, as far as the user is concerned, Fill and FX.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayerType {
    Fill,
    Fx,
}

/// Events concerning layer operation triggered by the user, such as adding,
/// removing, reordering, etc. These events should be treated as unsanitized,
/// since they are user generated.
#[derive(Debug)]
pub enum UserLayersEvent {
    /// The user requests a new layer stack.
    AddLayers,
    /// The user seeks to push a new layer onto the layer stack, with a given
    /// type and operator.
    PushLayer(Resource<Graph>, LayerType, Operator),
    /// The user seeks to push a new mask onto the layer stack, with a given
    /// type and operator.
    PushMask(Resource<Node>, Operator),
    /// The user requests removal of a layer.
    RemoveLayer(Resource<Node>),
    /// The user requests removal of a mask.
    RemoveMask(Resource<Node>),
    /// The user requests to move the specified layer or mask upwards in the
    /// stack.
    MoveUp(Resource<Node>),
    /// The user requests to move the specified layer or mask downwards in the
    /// stack.
    MoveDown(Resource<Node>),
    /// The user requests setting the output of a layer to the given material
    /// channel to the specified output as enumerated. The boolean denotes
    /// whether the channel is enabled or not.
    SetOutput(Resource<Node>, MaterialChannel, usize, bool),
    /// The user requests setting the input for a layer denoted by the input
    /// socket to the given material channel.
    SetInput(Resource<Socket>, MaterialChannel),
    /// The user requests setting the opacity of the given layer or mask
    SetOpacity(Resource<Node>, f32),
    /// The user requests setting the blend mode of the given layer or mask
    SetBlendMode(Resource<Node>, BlendMode),
    /// The user requests changing the title of the given layer or mask
    SetTitle(Resource<Node>, String),
    /// The user seeks to enable/disable the given layer or mask
    SetEnabled(Resource<Node>, bool),
    /// The user requests conversion of this layer stack to a graph
    Convert(Resource<Graph>),
}

/// Events concerning layers, not directly coming from user input.
#[derive(Debug)]
pub enum LayersEvent {
    /// A layer stack has been added to the system, with the given parent size.
    LayersAdded(Resource<Graph>, u32),
    /// A layer has been pushed onto a layer stack, with the goven resource and
    /// type. The fields describe the following, in order
    ///
    /// 1. The resource of the new layer
    /// 2. The type of the layer
    /// 3. The human readable title
    /// 4. The operator used in the layer
    /// 5. The blend mode of the new layer
    /// 6. The opacity of the layer
    /// 7. A parameter box description to describe the parameters of the layer
    /// 8. The image size
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
    /// A layer or mask has been removed.
    LayerRemoved(Resource<Node>),
    /// A mask has been pushed for a layer in a stack. Fields are similar to
    /// `LayersAdded`, except for the resources that also include the parent
    /// layer.
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
    /// A layer/mask has been moved up one position in the stack.
    MovedUp(Resource<Node>),
    /// A layer/mask has been moved down one position in the stack.
    MovedDown(Resource<Node>),
}

/// Events concerning surfaces, not directly coming from user input.
#[derive(Debug)]
pub enum SurfaceEvent {
    /// The system requests an export according to the given export spec, with
    /// the given image size, to the path specified.
    ExportImage(ExportSpec, u32, PathBuf),
    /// The system reports having loaded an export specification with a given name.
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

/// Events concerning renderer operation triggered by the user.
#[derive(Debug)]
pub enum UserRenderEvent {
    /// The user requests rotation of the view by angles theta and phi.
    Rotate(RendererID, f32, f32),
    /// The user requests panning of view by x and y deltas.
    Pan(RendererID, f32, f32),
    /// The user requests zooming of view
    Zoom(RendererID, f32),
    /// The user requests moving the light position by x and y deltas.
    LightMove(RendererID, f32, f32),
    /// The user requests display of the specified channel
    ChannelChange2D(RendererID, MaterialChannel),
    /// The user requests setting the displacement amount
    DisplacementAmount(RendererID, f32),
    /// The user requests setting the texture scale
    TextureScale(RendererID, f32),
    /// The user requests setting the strength of the HDRi
    EnvironmentStrength(RendererID, f32),
    /// The user requests setting the blurring of the HDRi
    EnvironmentBlur(RendererID, f32),
    /// The user requests setting the light type
    LightType(RendererID, LightType),
    /// The user requests setting the light strength
    LightStrength(RendererID, f32),
    /// The user requests setting the fog strength
    FogStrength(RendererID, f32),
    /// The user requests setting the focal length
    FocalLength(RendererID, f32),
    /// The user requests setting the aperture size
    ApertureSize(RendererID, f32),
    /// The user requests setting the focal distance
    FocalDistance(RendererID, f32),
    /// The user requests enabling/disabling shadow calculation
    SetShadow(RendererID, ParameterBool),
    /// The user requests enabling/disabling ambient occlusion calculation
    SetAO(RendererID, ParameterBool),
    /// The user seeks to load a new HDRi from file
    LoadHDRI(RendererID, PathBuf),
    /// The user requests setting the object type to be rendered
    ObjectType(RendererID, ObjectType),
    /// The user requests setting the sample count
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
    Grayscale([ChannelSpec; 1]),
}

impl ExportSpec {
    /// Convert to another image type.
    pub fn image_type(self, ty: ImageType) -> Self {
        match &self {
            ExportSpec::RGBA(cs) => match ty {
                ImageType::Rgb => self,
                ImageType::Grayscale => ExportSpec::Grayscale([cs[0].clone()]),
            },
            ExportSpec::RGB(cs) => match ty {
                ImageType::Rgb => self,
                ImageType::Grayscale => ExportSpec::Grayscale([cs[0].clone()]),
            },
            ExportSpec::Grayscale([c]) => match ty {
                ImageType::Grayscale => self,
                ImageType::Rgb => ExportSpec::RGB([c.clone(), c.clone(), c.clone()]),
            },
        }
    }

    /// Set existence of alpha channel.
    pub fn set_has_alpha(self, alpha: bool) -> Self {
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
    pub fn set_red(&mut self, spec: ChannelSpec) {
        match self {
            ExportSpec::RGBA(cs) => {
                cs[0] = spec;
            }
            ExportSpec::RGB(cs) => {
                cs[0] = spec;
            }
            ExportSpec::Grayscale(c) => {
                c[0] = spec;
            }
        }
    }

    /// Set green channel if available
    pub fn set_green(&mut self, spec: ChannelSpec) {
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
    pub fn set_blue(&mut self, spec: ChannelSpec) {
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
    pub fn set_alpha(&mut self, spec: ChannelSpec) {
        match self {
            ExportSpec::RGBA(cs) => {
                cs[3] = spec;
            }
            ExportSpec::RGB(_) => {}
            ExportSpec::Grayscale(_) => {}
        }
    }

    pub fn channel_specs(&self) -> &[ChannelSpec] {
        match self {
            ExportSpec::RGBA(s) => &s[..],
            ExportSpec::RGB(s) => &s[..],
            ExportSpec::Grayscale(s) => &s[..],
        }
    }
}

/// IO related events triggered by the user. Should be treated as unsanitized
/// because they are usually user generated.
#[derive(Debug)]
pub enum UserIOEvent {
    /// The user requests loading a surface from file to replace the current.
    OpenSurface(PathBuf),
    /// The user requests saving the current surface to file.
    SaveSurface(PathBuf),
    /// The user requests setting the parent size.
    SetParentSize(u32),
    /// The user requests declaration of an export specification.
    DeclareExport(String, ExportSpec),
    /// The user requests renaming of an export specification.
    RenameExport(String, String),
    /// The user requests export according to existing specification.
    RunExports(PathBuf),
    /// The user requests a new surface file.
    NewSurface,
    /// The user requests quitting the application.
    Quit,
}

/// Events triggered during computation or setup thereof
#[derive(Debug)]
pub enum ComputeEvent {
    /// The system has computed an output image.
    OutputReady(
        Resource<Node>,
        crate::gpu::BrokerImage,
        crate::gpu::Layout,
        crate::gpu::Access,
        u32,
        OutputType,
    ),
    /// The system has created a compute socket with a fixed type.
    SocketCreated(Resource<Socket>, ImageType),
    /// The system has destroyed a compute socket.
    SocketDestroyed(Resource<Socket>),
    /// The system has created and filled a thumbnail.
    ThumbnailCreated(Resource<Node>, crate::gpu::BrokerImageView),
    /// The system has destroyed a thumbnail.
    ThumbnailDestroyed(Resource<Node>),
    /// The system has the given thumbnail for the given node.
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

#[repr(u32)]
#[derive(
    Debug, Display, Clone, Copy, Serialize, Deserialize, PartialEq, AsBytes, ParameterField,
)]
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
/// Instead these events are created by the UI internally.
#[derive(Debug)]
pub enum UIEvent {
    /// The UI requests a renderer with a fixed ID, specifying the monitor size
    /// and view size and the type of renderer to be created.
    RendererRequested(RendererID, (u32, u32), (u32, u32), RendererType),
    /// The UI requests a redraw of the given renderer.
    RendererRedraw(RendererID),
    /// The UI requests resizing the given renderer.
    RendererResize(RendererID, u32, u32),
    /// The UI requests removal of the renderer.
    RendererRemoved(RendererID),
}

/// Events from the renderer.
#[derive(Debug)]
pub enum RenderEvent {
    /// A renderer has been added with the given ID and image view.
    RendererAdded(RendererID, crate::gpu::BrokerImageView),
    /// The specified renderer has been redrawn.
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
