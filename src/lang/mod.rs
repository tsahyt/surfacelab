use crate::compute::shaders::{
    IntermediateDataDescription, OperatorPassDescription, Shader, Uniforms,
};

pub mod config;
pub mod operators;
pub mod parameters;
pub mod resource;
pub mod socketed;

use enum_dispatch::*;
use enumset::{EnumSet, EnumSetType};
use num_enum::UnsafeFromPrimitive;

use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::*;
use strum_macros::*;
use thiserror::Error;
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
    AlphaExtract,
    AmbientOcclusion,
    Blend,
    BlendMasked,
    Blur,
    Checker,
    ColorAdjust,
    Distance,
    PerlinNoise,
    Voronoi,
    Rgb,
    Value,
    Range,
    Shape,
    Swizzle,
    Split,
    Svg(operators::Svg),
    Merge,
    Transform,
    Threshold,
    Grayscale,
    Ramp,
    NormalMap,
    NormalBlend,
    NoiseSpread,
    Warp,
    Image,
    Output,
    Input,
}

impl AtomicOperator {
    /// A vector of all atomic operators with their default parameters. Useful
    /// for frontends to present a list of all operators.
    pub fn all_default() -> Vec<Self> {
        vec![
            Self::AlphaExtract(AlphaExtract::default()),
            Self::AmbientOcclusion(AmbientOcclusion::default()),
            Self::Blend(Blend::default()),
            Self::BlendMasked(BlendMasked::default()),
            Self::Blur(Blur::default()),
            Self::Checker(Checker::default()),
            Self::Distance(Distance::default()),
            Self::PerlinNoise(PerlinNoise::default()),
            Self::Voronoi(Voronoi::default()),
            Self::Rgb(Rgb::default()),
            Self::Value(Value::default()),
            Self::ColorAdjust(ColorAdjust::default()),
            Self::Range(Range::default()),
            Self::Shape(Shape::default()),
            Self::Swizzle(Swizzle::default()),
            Self::Split(Split::default()),
            Self::Svg(operators::Svg::default()),
            Self::Merge(Merge::default()),
            Self::Transform(Transform::default()),
            Self::Threshold(Threshold::default()),
            Self::Grayscale(Grayscale::default()),
            Self::Ramp(Ramp::default()),
            Self::NormalMap(NormalMap::default()),
            Self::NormalBlend(NormalBlend::default()),
            Self::NoiseSpread(NoiseSpread::default()),
            Self::Warp(Warp::default()),
            Self::Image(Image::default()),
            Self::Output(Output::default()),
            Self::Input(Input::default()),
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum OperatorSize {
    RelativeToParent(i32),
    AbsoluteSize(u32),
}

impl Default for OperatorSize {
    fn default() -> Self {
        Self::RelativeToParent(0)
    }
}

impl OperatorSize {
    /// Create an absolute size from a float by snapping to the nearest "normal"
    /// texture size. Normal here means powers of two *or* multiples of 3k up to
    /// 12k.
    pub fn abs_nearest(val: f32) -> Self {
        let pow2 = 2_u32.pow(val.log(2.).floor().max(5.) as u32);
        let mul3k = (val as u32 / 3072).min(4) * 3072;

        Self::AbsoluteSize(pow2.max(mul3k))
    }

    /// Convert relative size to absolute, given a parent size.
    pub fn to_abs(self, parent_size: u32) -> Self {
        match self {
            OperatorSize::RelativeToParent(s) => Self::AbsoluteSize(if s > 0 {
                parent_size << s as i16
            } else {
                parent_size >> -s as i16
            }),
            s => s,
        }
    }

    /// Get this size as an absolute size, given a parent size, clamped to 32
    /// and 16384.
    pub fn absolute(self, parent_size: u32) -> u32 {
        match self.to_abs(parent_size) {
            OperatorSize::AbsoluteSize(s) => s,
            _ => unreachable!(),
        }
        .clamp(32, 16384)
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

#[derive(Error, Debug)]
pub enum MonomorphizationError {
    #[error("Socket missing in node")]
    MissingSocket,
    #[error("Monomorphization of polymorphic socket attempted")]
    PolymorphicSocket(TypeVariable),
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

    /// Returns the graph called by the operator if and only if this is a
    /// complex operator.
    pub fn graph(&self) -> Option<&Resource<Graph>> {
        match self {
            Operator::AtomicOperator(_) => None,
            Operator::ComplexOperator(o) => Some(&o.graph),
        }
    }

    /// A mask is any operator that has one input or less, and some number of
    /// outputs greater than 0 that can be interpreted as grayscale images.
    pub fn is_mask(&self) -> bool {
        self.inputs().len() <= 1
            && self
                .outputs()
                .values()
                .any(|t| !matches!(t, OperatorType::Monomorphic(ImageType::Rgb)))
    }

    /// Obtain the monomorphic type of a socket if possible.
    pub fn monomorphic_type(
        &self,
        socket: &str,
        type_vars: &HashMap<TypeVariable, ImageType>,
    ) -> Result<ImageType, MonomorphizationError> {
        let ty = self
            .inputs()
            .get(socket)
            .cloned()
            .or_else(|| self.outputs().get(socket).cloned())
            .ok_or(MonomorphizationError::MissingSocket)?;
        match ty {
            OperatorType::Polymorphic(p) => match type_vars.get(&p) {
                Some(x) => Ok(*x),
                _ => Err(MonomorphizationError::PolymorphicSocket(p)),
            },
            OperatorType::Monomorphic(x) => Ok(x),
        }
    }

    /// Determine whether an operator is scalable or not
    pub fn scalable(&self) -> bool {
        self.size_request().is_none() && !self.is_output() && !self.is_input()
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

#[derive(Clone, Debug)]
pub enum Instruction {
    /// Execute an atomic operator for the given node
    Execute(Resource<Node>, AtomicOperator),
    /// Perform a call at the given node to the complex operator as specified
    Call(Resource<Node>, ComplexOperator),
    /// Move data from socket to socket
    Move(Resource<Socket>, Resource<Socket>),
    /// Copy data from socket to socket
    Copy(Resource<Socket>, Resource<Socket>),
    /// Generate a thumbnail for the given socket
    Thumbnail(Resource<Socket>),
}

impl Instruction {
    pub fn is_execution_step(&self) -> bool {
        matches!(self, Self::Execute(..) | Self::Call(..))
    }

    pub fn is_call_skippable(&self) -> bool {
        matches!(
            self,
            Self::Execute(_, AtomicOperator::Output { .. })
                | Self::Execute(_, AtomicOperator::Input { .. })
                | Self::Thumbnail(..)
        )
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
    Display,
    Serialize,
    Deserialize,
    Hash,
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

impl From<OutputType> for ImageType {
    fn from(source: OutputType) -> Self {
        match source {
            OutputType::Albedo => ImageType::Rgb,
            OutputType::Roughness => ImageType::Grayscale,
            OutputType::Normal => ImageType::Rgb,
            OutputType::Displacement => ImageType::Grayscale,
            OutputType::Metallic => ImageType::Grayscale,
            OutputType::AmbientOcclusion => ImageType::Grayscale,
            OutputType::Alpha => ImageType::Grayscale,
            OutputType::Value => ImageType::Grayscale,
            OutputType::Rgb => ImageType::Rgb,
        }
    }
}

impl From<MaterialChannel> for ImageType {
    fn from(source: MaterialChannel) -> Self {
        match source {
            MaterialChannel::Displacement => ImageType::Grayscale,
            MaterialChannel::Albedo => ImageType::Rgb,
            MaterialChannel::Normal => ImageType::Rgb,
            MaterialChannel::Roughness => ImageType::Grayscale,
            MaterialChannel::Metallic => ImageType::Grayscale,
            MaterialChannel::Alpha => ImageType::Grayscale,
        }
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
    /// Check whether this operator type is compatible (can be unified with)
    /// another type. Note that unification of two polymorphic types is
    /// forbidden!
    pub fn can_unify(&self, other: &OperatorType) -> bool {
        match (self, other) {
            (OperatorType::Monomorphic(t), OperatorType::Monomorphic(q)) => t == q,
            (OperatorType::Monomorphic(_), OperatorType::Polymorphic(_)) => true,
            (OperatorType::Polymorphic(_), OperatorType::Monomorphic(_)) => true,
            (OperatorType::Polymorphic(_), OperatorType::Polymorphic(_)) => false,
        }
    }

    /// Like `can_unify` but takes mappings of type variables to respect during
    /// the process. Unification of two polymorphic types is forbidden
    /// regardless!
    pub fn can_unify_with(
        &self,
        other: &OperatorType,
        ty_vars_self: &HashMap<TypeVariable, ImageType>,
        ty_vars_other: &HashMap<TypeVariable, ImageType>,
    ) -> bool {
        match (self, other) {
            (OperatorType::Monomorphic(t), OperatorType::Monomorphic(q)) => t == q,
            (OperatorType::Monomorphic(t), OperatorType::Polymorphic(q)) => {
                ty_vars_other.get(q).map(|z| t == z).unwrap_or(true)
            }
            (OperatorType::Polymorphic(q), OperatorType::Monomorphic(t)) => {
                ty_vars_self.get(q).map(|z| t == z).unwrap_or(true)
            }
            (OperatorType::Polymorphic(t), OperatorType::Polymorphic(q)) => ty_vars_self
                .get(t)
                .and_then(|z| ty_vars_other.get(q).map(|w| w == z))
                .unwrap_or(false),
        }
    }

    /// Unify this operator type with another operator, modifying type variable
    /// assignments in the process if required. This will not overwrite existing
    /// type variable assignments! If the types are incompatible as given, no
    /// change will occur.
    pub fn unify_with(
        &self,
        other: &OperatorType,
        ty_vars_self: &mut HashMap<TypeVariable, ImageType>,
        ty_vars_other: &mut HashMap<TypeVariable, ImageType>,
    ) {
        match (self, other) {
            (OperatorType::Monomorphic(_), OperatorType::Monomorphic(_)) => {}
            (OperatorType::Monomorphic(t), OperatorType::Polymorphic(q)) => {
                if ty_vars_other.get(q).is_none() {
                    ty_vars_other.insert(*q, *t);
                }
            }
            (OperatorType::Polymorphic(q), OperatorType::Monomorphic(t)) => {
                if ty_vars_self.get(q).is_none() {
                    ty_vars_self.insert(*q, *t);
                }
            }
            (OperatorType::Polymorphic(t), OperatorType::Polymorphic(q)) => {
                if ty_vars_self.get(t).is_none() {
                    if let Some(x) = ty_vars_other.get(q) {
                        ty_vars_self.insert(*t, *x);
                    }
                }
                if ty_vars_other.get(q).is_none() {
                    if let Some(x) = ty_vars_self.get(t) {
                        ty_vars_other.insert(*t, *x);
                    }
                }
            }
        }
    }
}

impl From<ImageType> for OperatorType {
    fn from(source: ImageType) -> Self {
        Self::Monomorphic(source)
    }
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
    /// The user requests creation of a new node inside a given graph, using
    /// this operator, at layout position. The optional socket specifies an
    /// existing socket to be connected to a suitable socket on the new node
    /// after creation. The optional string specifies a name to be requested.
    /// This request may not be filled if a node of this name already exists!
    NewNode(
        Resource<Graph>,
        Operator,
        (f64, f64),
        Option<Resource<Socket>>,
        Option<String>,
    ),
    /// The user requests the removal of a given node.
    RemoveNode(Resource<Node>),
    /// The user requests a connection between the two sockets. Requires the
    /// first socket to be the source and the second to be the sink, i.e. order
    /// matters!
    ConnectSockets(Resource<Socket>, Resource<Socket>),
    /// The user requests the disconnection of the given sink socket.
    DisconnectSinkSocket(Resource<Socket>),
    /// The user requests connecting a node between two sockets
    ConnectBetweenSockets(Resource<Node>, Resource<Socket>, Resource<Socket>),
    /// The user requests quick blending of the two given nodes using the given operator
    QuickCombine(Operator, Resource<Node>, Resource<Node>),
    /// The user changes the given parameter from the first value to the second value.
    ParameterChange(Resource<Param>, Vec<u8>, Vec<u8>),
    /// The user repositions the node to the given coordinates.
    PositionNode(Resource<Node>, (f64, f64)),
    /// The user renames a node from a resource to another resource.
    RenameNode(Resource<Node>, Resource<Node>),
    /// The user changes the output size of the given node
    OutputSizeChange(Resource<Node>, OperatorSize),
    /// The user requests display of the given socket or disabling it
    ViewSocket(Option<Resource<Socket>>),
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
    /// The user seeks to delete a graph.
    DeleteGraph(Resource<Graph>),
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
    /// The user requests extraction of the following nodes from this graph into a new graph
    Extract(Vec<Resource<Node>>),
    /// The user requests injection of a complex operator into the current
    /// graph, replacing the complex operator.
    Inject(Resource<Node>, ComplexOperator),
}

/// Events concerning graphs, not directly coming from user input.
#[derive(Debug)]
pub enum GraphEvent {
    /// A graph identified by this resource has been added to the system.
    GraphAdded(Resource<Graph>),
    /// A graph has been removed from the system.
    GraphRemoved(Resource<Graph>),
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
    NodeRemoved(Resource<Node>, Operator, (f64, f64)),
    /// A node has been renamed/moved from a resource to a resource.
    NodeRenamed(Resource<Node>, Resource<Node>),
    /// A node has been resized to the new given size. The bool indicates
    /// whether the size is scalable.
    NodeResized(Resource<Node>, u32, bool),
    /// A complex operator has been updated in the system, and is now
    /// represented by the given parameters.
    ComplexOperatorUpdated(
        Resource<Node>,
        ComplexOperator,
        ParamBoxDescription<MessageWriters>,
    ),
    /// Two sockets have been connected. The first socket is the source, the
    /// second the sink.
    ConnectedSockets(Resource<Socket>, Resource<Socket>),
    /// Two sockets have been disconnected from each other.
    DisconnectedSockets(Resource<Socket>, Resource<Socket>),
    /// A graph has been relinearized, resulting in the new linearization data
    /// supplied.
    Relinearized(Resource<Graph>, Linearization, UsePoints),
    /// A graph needs to be recomputed.
    Recompute(Resource<Graph>, Vec<(ExportSpec, PathBuf)>),
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
    /// Loaded graphs have been serialized
    Serialized(Vec<u8>),
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
    /// The user requests deletion of a layer stack.
    DeleteLayers(Resource<Graph>),
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
    /// The user requests setting the opacity of the given layer or mask from
    /// the first value to the second value
    SetOpacity(Resource<Node>, f32, f32),
    /// The user requests setting the blend mode of the given layer or mask from
    /// the first value to the second value
    SetBlendMode(Resource<Node>, BlendMode, BlendMode),
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
    /// A layer stack has been added to the system, with the supplied parent
    /// size and a list of material channel outputs.
    LayersAdded(Resource<Graph>, u32, Vec<Resource<Node>>),
    /// A layer stack has been removed from the system.
    LayersRemoved(Resource<Graph>),
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
    /// The specified channel output for a given layer has been disabled.
    OutputUnset(Resource<Node>, MaterialChannel),
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
    /// The system reports having declared an export specification.
    ExportSpecDeclared(ExportSpec),
    /// The system reports having removed an export specification
    ExportSpecRemoved(ExportSpec),
    /// The system has updated an export spec from the first to the second.
    ExportSpecUpdated(ExportSpec, ExportSpec),
    /// The parent size has been set
    ParentSizeSet(u32),
}

/// Renderers are indexed by an ID, internally merely a `u64`.
pub type RendererID = u64;

/// Light types supported by renderers.
#[derive(AsBytes, Copy, Clone, Debug, Serialize, EnumVariantNames, Deserialize)]
#[repr(u32)]
#[strum(serialize_all = "kebab_case")]
pub enum LightType {
    PointLight = 0,
    SunLight = 1,
}

/// Object types supported by the SDF 3D renderer
#[derive(AsBytes, Copy, Clone, Debug, Serialize, EnumVariantNames, Deserialize)]
#[repr(u32)]
#[strum(serialize_all = "kebab_case")]
pub enum ObjectType {
    Plane = 0,
    FinitePlane = 1,
    Cube = 2,
    Sphere = 3,
    Cylinder = 4,
}

/// Shading modes supported by the SDF 3D renderer
#[derive(
    AsBytes, Copy, Clone, Debug, Serialize, EnumVariantNames, Deserialize, UnsafeFromPrimitive,
)]
#[repr(u32)]
#[strum(serialize_all = "kebab_case")]
pub enum ShadingMode {
    Pbr = 0,
    Matcap = 1,
}

impl ShadingMode {
    pub fn has_lights(self) -> bool {
        matches!(self, Self::Pbr)
    }

    pub fn has_matcap(self) -> bool {
        matches!(self, Self::Matcap)
    }
}

/// Tonemapping operators for renderer
#[derive(AsBytes, Copy, Clone, Debug, Serialize, EnumVariantNames, Deserialize)]
#[repr(u32)]
#[strum(serialize_all = "kebab_case")]
pub enum ToneMap {
    Reinhard = 0,
    ReinhardJodie = 1,
    Hable = 2,
    Aces = 3,
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
    /// The user requests setting the rotation of the HDRi
    EnvironmentRotation(RendererID, f32),
    /// The user requests setting the light type
    LightType(RendererID, LightType),
    /// The user requests setting the light strength
    LightStrength(RendererID, f32),
    /// The user requests setting the light size
    LightSize(RendererID, f32),
    /// The user requests setting the fog strength
    FogStrength(RendererID, f32),
    /// The user requests setting the focal length
    FocalLength(RendererID, f32),
    /// The user requests setting the aperture size
    ApertureSize(RendererID, f32),
    /// The user requests setting the number of aperture blades
    ApertureBlades(RendererID, i32),
    /// The user requests setting the aperture rotation
    ApertureRotation(RendererID, f32),
    /// The user requests setting the focal distance
    FocalDistance(RendererID, f32),
    /// The user requests enabling/disabling shadow calculation
    SetShadow(RendererID, ParameterBool),
    /// The user requests setting the ambient occlusion strength
    AoStrength(RendererID, f32),
    /// The user seeks to load a new HDRI from file
    LoadHdri(RendererID, Option<PathBuf>),
    /// The user seeks to load a new matcap from file
    LoadMatcap(RendererID, Option<PathBuf>),
    /// The user requests setting the object type to be rendered
    ObjectType(RendererID, ObjectType),
    /// The user requests setting the renderer shading mode
    ShadingMode(RendererID, ShadingMode),
    /// The user requests changing the tone mapping operator
    ToneMap(RendererID, ToneMap),
    /// The user requests setting the sample count
    SampleCount(RendererID, u32),
    /// The user requests resetting of the camera position
    CenterCamera(RendererID),
}

/// Supported color spaces for (external) images.
#[repr(u32)]
#[derive(Debug, EnumIter, ToString, PartialEq, Eq, Clone, Copy, Serialize, Deserialize, Hash)]
#[strum(serialize_all = "kebab_case")]
pub enum ColorSpace {
    Srgb,
    Linear,
}

#[derive(Debug, EnumIter, ToString, PartialEq, Copy, Clone, Serialize, Deserialize)]
#[strum(serialize_all = "kebab_case")]
pub enum ExportFormat {
    Png,
    Jpeg,
    Hdr,
    Tiff,
    Tga,
}

impl ExportFormat {
    pub fn file_extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Hdr => "hdr",
            ExportFormat::Tiff => "tiff",
            ExportFormat::Tga => "tga",
        }
    }
}

/// Export specifications
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportSpec {
    pub name: String,
    pub node: Resource<Node>,
    pub color_space: ColorSpace,
    pub bit_depth: u8,
    pub format: ExportFormat,
}

impl From<&Resource<Node>> for ExportSpec {
    fn from(source: &Resource<Node>) -> Self {
        Self {
            name: source.file().unwrap_or("unnamed").to_string(),
            node: source.clone(),
            color_space: ColorSpace::Srgb,
            bit_depth: 8,
            format: ExportFormat::Png,
        }
    }
}

impl ExportSpec {
    fn legal(color_space: ColorSpace, format: ExportFormat, bit_depth: u8) -> bool {
        use ColorSpace::*;
        use ExportFormat::*;

        match (color_space, format, bit_depth) {
            (Srgb, Png, 8) => true,
            (Linear, Png, 8) => true,
            (Srgb, Jpeg, 8) => true,
            (Linear, Jpeg, 8) => true,
            (Srgb, Tiff, 8) => true,
            (Linear, Tiff, 8) => true,
            (Srgb, Tga, 8) => true,
            (Linear, Tga, 8) => true,
            (Srgb, Png, 16) => true,
            (Linear, Png, 16) => true,
            (Linear, Hdr, 32) => true,
            _ => false,
        }
    }

    /// Determine whether a color space is legal for this spec
    pub fn color_space_legal(&self, color_space: ColorSpace) -> bool {
        Self::legal(color_space, self.format, self.bit_depth)
    }

    /// Determine whether a format is legal for this spec
    pub fn format_legal(&self, format: ExportFormat) -> bool {
        Self::legal(self.color_space, format, self.bit_depth)
    }

    /// Determine whether a bit depth is legal for this spec
    pub fn bit_depth_legal(&self, bit_depth: u8) -> bool {
        Self::legal(self.color_space, self.format, bit_depth)
    }

    /// Sanitize this spec such that all entries are legal for the color space
    pub fn sanitize_for_color_space(&mut self) {
        use itertools::Itertools;
        use strum::IntoEnumIterator;

        if !self.color_space_legal(self.color_space) {
            let (format, bit_depth) = ExportFormat::iter()
                .cartesian_product([8, 16, 32].iter().copied())
                .find(|(f, b)| Self::legal(self.color_space, *f, *b))
                .unwrap();
            self.format = format;
            self.bit_depth = bit_depth;
        }
    }

    /// Sanitize this spec such that all entries are legal for the format
    pub fn sanitize_for_format(&mut self) {
        use itertools::Itertools;
        use strum::IntoEnumIterator;

        if !self.format_legal(self.format) {
            let (color_space, bit_depth) = ColorSpace::iter()
                .cartesian_product([8, 16, 32].iter().copied())
                .find(|(c, b)| Self::legal(*c, self.format, *b))
                .unwrap();
            self.color_space = color_space;
            self.bit_depth = bit_depth;
        }
    }

    /// Sanitize this spec such that all entries are legal for the bit depth
    pub fn sanitize_for_bit_depth(&mut self) {
        use itertools::Itertools;
        use strum::IntoEnumIterator;

        if !self.bit_depth_legal(self.bit_depth) {
            let (color_space, format) = ColorSpace::iter()
                .cartesian_product(ExportFormat::iter())
                .find(|(c, f)| Self::legal(*c, *f, self.bit_depth))
                .unwrap();
            self.color_space = color_space;
            self.format = format;
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
    /// The user seeks to add an image resource from a file.
    AddImageResource(PathBuf),
    /// The user seeks to set the colorspace of an image.
    SetImageColorSpace(Resource<Img>, ColorSpace),
    /// The user requests packing of an image.
    PackImage(Resource<Img>),
    /// The user seeks to remove an image resource
    RemoveImageResource(Resource<Img>),
    /// The user requests reloading of an external image resource
    ReloadImageResource(Resource<Img>),
    /// The user seeks to add an SVG resource from a file.
    AddSvgResource(PathBuf),
    /// The user requests packing of an SVG resource.
    PackSvg(Resource<resource::Svg>),
    /// The user seeks to remove an SVG resource
    RemoveSvgResource(Resource<resource::Svg>),
    /// The user requests reloading of an external image resource
    ReloadSvgResource(Resource<resource::Svg>),
    /// The user requests setting the parent size.
    SetParentSize(u32),
    /// The user requests declaration of a new export specification. The bool
    /// declares that the provided name should be kept.
    NewExportSpec(ExportSpec, bool),
    /// The user requests updating of an export specification with new data.
    /// This may include a name change.
    UpdateExportSpec(String, ExportSpec),
    /// The user requests removal of a named export specification
    RemoveExportSpec(String),
    /// The user requests export according to existing specification.
    RunExports(PathBuf),
    /// The user requests a new surface file.
    NewSurface,
    /// The user requests quitting the application.
    Quit,
    /// The user requests an undo
    Undo,
    /// The user requests a redo
    Redo,
    /// The user resized the window
    ResizeWindow(u32, u32),
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
    /// The system has computed a socket for viewing
    SocketViewReady(
        crate::gpu::BrokerImage,
        crate::gpu::Layout,
        crate::gpu::Access,
        u32,
        ImageType,
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
    /// An image resource has been registered. The bool describes whether the resource is packed.
    ImageResourceAdded(Resource<Img>, ColorSpace, bool),
    /// An image resource has been unregistered.
    ImageResourceRemoved(Resource<Img>, Option<PathBuf>),
    /// Image colorspace has been changed.
    ImageColorSpaceSet(Resource<Img>, ColorSpace),
    /// Image has been packed
    ImagePacked(Resource<Img>),
    /// An SVG resource has been registered. The bool describes whether the resource is packed.
    SvgResourceAdded(Resource<resource::Svg>, bool),
    /// SVG has been packed
    SvgPacked(Resource<resource::Svg>),
    /// An SVG resource has been unregistered.
    SvgResourceRemoved(Resource<resource::Svg>, Option<PathBuf>),
    /// Compute data has been serialized
    Serialized(Vec<u8>),
    /// Compute data has been cleared,
    Cleared,
    /// System compiled VRAM usage report, bytes used and total bytes in managed region
    VramUsage(usize, usize),
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
    Alpha,
}

impl MaterialChannel {
    pub fn legal_for(self, ty: OperatorType) -> bool {
        match ty {
            OperatorType::Monomorphic(ty) => ImageType::from(self) == ty,
            OperatorType::Polymorphic(_) => true,
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
            MaterialChannel::Alpha => "alpha",
        }
    }
}

#[repr(u32)]
#[derive(Debug, Display, Clone, Copy, Serialize, Deserialize, PartialEq, AsBytes)]
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
    RendererRemove(RendererID),
}

/// Events from the renderer.
#[derive(Debug)]
pub enum RenderEvent {
    /// A renderer has been added with the given ID and image view.
    RendererAdded(
        RendererID,
        crate::gpu::BrokerImageView,
        ParamBoxDescription<RenderField>,
    ),
    /// The specified renderer has been redrawn.
    RendererRedrawn(RendererID),
    /// The specified renderer has been removed.
    RendererRemoved(RendererID),
    /// The specified render settings have been updated
    SettingsUpdated(RendererID, ParamBoxDescription<RenderField>),
    /// Render settings have been serialized.
    Serialized(Vec<u8>),
}

/// Events from the IO component
#[derive(Debug)]
pub enum IOEvent {
    /// Node Data has been loaded by the IO component
    NodeDataLoaded(Vec<u8>),
    /// Compute Data has been loaded by the IO component
    ComputeDataLoaded(Vec<u8>),
    /// Render Settings have been loaded by the IO component
    RenderSettingsLoaded(Vec<u8>),
}

/// Events from the scheduler
#[derive(Debug)]
pub enum ScheduleEvent {
    /// Autosave schedule reached
    Autosave,
    /// VRAM statistics scheduled
    VramUsage,
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
    IOEvent(IOEvent),
    UIEvent(UIEvent),
    GraphEvent(GraphEvent),
    LayersEvent(LayersEvent),
    SurfaceEvent(SurfaceEvent),
    ComputeEvent(ComputeEvent),
    RenderEvent(RenderEvent),
    ScheduleEvent(ScheduleEvent),
}
