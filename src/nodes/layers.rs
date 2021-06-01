use crate::lang::*;
use enumset::EnumSet;
use itertools::Itertools;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::iter::FromIterator;
use strum::IntoEnumIterator;

/// Slice width for graph layouting on conversion. The "correct" value for this
/// depends on the size of nodes in the UI. These *should* be standardized to
/// 128.
const SLICE_WIDTH: f64 = 256.0;

/// A type encoding a function from material channels to sockets.
type ChannelMap = HashMap<MaterialChannel, String>;

/// A type encoding a function from sockets to material channels.
type InputMap = HashMap<String, MaterialChannel>;

/// A mask (layer) is any operator that has one input or less, and some number
/// of outputs greater than 0 that can be interpreted as grayscale images. To
/// retain flexibility, color outputs may be allowed, but only the red channel
/// will be used.
///
/// Masks are thus single-channel layers. When blending mask layers, there is no
/// way to apply a mask into the blend, i.e. masks cannot be "recursive".
///
/// For compute purposes, *all* sockets of any mask layer will always carry a
/// grayscale type, in particular this means that all polymorphic sockets will
/// be monomorphized thusly, regardless of circumstances.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mask {
    /// Human readable name of the mask
    name: String,
    /// Mask operator
    operator: Operator,
    /// The output socket of the operator to use
    output_socket: String,
    /// Options to blend the mask with mask layers below it
    blend_options: MaskBlendOptions,
}

impl From<Operator> for Mask {
    fn from(source: Operator) -> Self {
        Self {
            name: source.default_name().to_string(),
            output_socket: source
                .outputs()
                .keys()
                .next()
                .expect("Invalid operator for mask")
                .to_string(),
            operator: source,
            blend_options: MaskBlendOptions::default(),
        }
    }
}

/// Options for blending masks. Differs from ordinary blend options in that it
/// can not have any masks itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaskBlendOptions {
    opacity: f32,
    blend_mode: BlendMode,
    enabled: bool,
}

impl MaskBlendOptions {
    pub fn blend_operator(&self) -> Blend {
        Blend {
            blend_mode: self.blend_mode,
            mix: self.opacity,
            sharpness: 16.0,
            clamp_output: 1,
        }
    }
}

impl Default for MaskBlendOptions {
    fn default() -> Self {
        MaskBlendOptions {
            opacity: 1.0,
            blend_mode: BlendMode::Mix,
            enabled: true,
        }
    }
}

/// A mask stack is like a layer stack but specialized such that it must have
/// specific types of operators, and cannot have masks itself.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct MaskStack {
    stack: Vec<(Resource<Node>, Mask)>,
}

impl MaskStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn can_linearize(&self) -> bool {
        self.linearize_into(
            |_| Resource::node(""),
            &mut Vec::new(),
            &mut HashMap::new(),
            &mut 0,
        )
        .is_some()
    }

    /// Utilize this mask stack in a larger layer linearization.
    ///
    /// Linearization structures are passed into this function as mutable
    /// references, and resource naming functions are passed as closures.
    ///
    /// Will return None if an error occurs.
    pub fn linearize_into<G: Fn(&Mask) -> Resource<Node>>(
        &self,
        blend_resource: G,
        linearization: &mut Linearization,
        use_points: &mut HashMap<Resource<Node>, UsePoint>,
        step: &mut usize,
    ) -> Option<()> {
        let mut last_socket: Option<Resource<Socket>> = None;

        for (resource, mask) in self.stack.iter().filter(|m| m.1.blend_options.enabled) {
            *step += 1;

            match &mask.operator {
                Operator::AtomicOperator(aop) => {
                    // Move inputs
                    for socket in aop.inputs().keys() {
                        let input_resource = last_socket.clone()?;
                        use_points
                            .entry(input_resource.socket_node())
                            .and_modify(|e| e.last = *step)
                            .or_insert(UsePoint {
                                last: *step,
                                creation: usize::MIN,
                            });

                        linearization.push(Instruction::Move(
                            input_resource,
                            resource.node_socket(socket),
                        ));
                    }

                    linearization.push(Instruction::Execute(resource.clone(), aop.clone()));
                    if let Some(thmbsocket) = aop.outputs().keys().sorted().next() {
                        linearization
                            .push(Instruction::Thumbnail(resource.node_socket(thmbsocket)));
                    }
                }
                Operator::ComplexOperator(cop) => {
                    // Copy inputs to internal sockets
                    for socket in cop.inputs().keys() {
                        let input = cop.inputs.get(socket).expect("Missing internal socket");
                        let input_resource = last_socket.clone()?;
                        use_points
                            .entry(input_resource.socket_node())
                            .and_modify(|e| e.last = *step)
                            .or_insert(UsePoint {
                                last: *step,
                                creation: usize::MIN,
                            });

                        linearization.push(Instruction::Copy(
                            input_resource,
                            input.1.node_socket("data"),
                        ));
                    }

                    let (out_socket, (_, output)) = cop
                        .outputs
                        .iter()
                        .next()
                        .expect("Mask operator with missing output");
                    linearization.push(Instruction::Call(resource.clone(), cop.clone()));
                    linearization.push(Instruction::Copy(
                        output.node_socket("data"),
                        resource.node_socket(out_socket),
                    ));
                    if let Some(thmbsocket) = cop.outputs().keys().sorted().next() {
                        linearization
                            .push(Instruction::Thumbnail(resource.node_socket(thmbsocket)));
                    }
                }
            }

            use_points
                .entry(resource.clone())
                .and_modify(|e| e.creation = *step)
                .or_insert(UsePoint {
                    last: usize::MAX,
                    creation: *step,
                });

            let outputs = mask.operator.outputs();
            let (socket, _) = outputs
                .iter()
                .next()
                .expect("Mask operator with missing output");

            if let Some(background) = last_socket {
                *step += 1;

                let blend_res: Resource<Node> = blend_resource(mask);

                use_points
                    .entry(resource.clone())
                    .and_modify(|e| e.last = *step)
                    .or_insert(UsePoint {
                        last: *step,
                        creation: usize::MIN,
                    });
                use_points
                    .entry(background.socket_node())
                    .and_modify(|e| e.last = *step)
                    .or_insert(UsePoint {
                        last: *step,
                        creation: usize::MIN,
                    });

                linearization.push(Instruction::Move(
                    background.clone(),
                    blend_res.node_socket("background"),
                ));
                linearization.push(Instruction::Move(
                    resource.node_socket(socket),
                    blend_res.node_socket("foreground"),
                ));
                linearization.push(Instruction::Execute(
                    blend_res.clone(),
                    AtomicOperator::Blend(mask.blend_options.blend_operator()),
                ));

                use_points
                    .entry(blend_res.clone())
                    .and_modify(|e| e.creation = *step)
                    .or_insert(UsePoint {
                        last: usize::MAX,
                        creation: *step,
                    });

                last_socket = Some(blend_res.node_socket("color"));
            } else {
                last_socket = Some(resource.node_socket(socket));
            }
        }

        Some(())
    }

    /// Insert this mask stack into a graph, for use in converting layer stacks
    /// to graphs.
    pub fn insert_into_graph(
        &self,
        graph: &mut super::nodegraph::NodeGraph,
        mut x: f64,
        parent_size: u32,
    ) -> Option<(String, String)> {
        let mut last_socket: Option<(String, String)> = None;

        for (_, mask) in self.stack.iter().filter(|m| m.1.blend_options.enabled) {
            let mask_node = graph.new_node(&mask.operator, parent_size, None).0;
            graph.position_node(&mask_node, x, -SLICE_WIDTH);
            x += SLICE_WIDTH;

            // Since this is a mask, there is always 0 or 1 input
            for input_socket in mask.operator.inputs().keys() {
                let (node, socket) = last_socket.as_ref()?;
                graph
                    .connect_sockets(&node, &socket, &mask_node, input_socket)
                    .ok()?;
            }

            if let Some((background_node, background_socket)) = &last_socket {
                let blend_op =
                    Operator::from(AtomicOperator::from(mask.blend_options.blend_operator()));
                let blend_node = graph.new_node(&blend_op, parent_size, None).0;
                graph.position_node(&blend_node, x, 0.0);
                x += SLICE_WIDTH;

                graph
                    .connect_sockets(
                        background_node,
                        background_socket,
                        &blend_node,
                        "background",
                    )
                    .ok()?;
                graph
                    .connect_sockets(&mask_node, &mask.output_socket, &blend_node, "foreground")
                    .ok()?;

                last_socket = Some((blend_node, "color".to_owned()));
            } else {
                last_socket = Some((mask_node, mask.output_socket.to_owned()));
            }
        }

        last_socket
    }

    /// Push a new mask operator onto the mask stack
    pub fn push(&mut self, mask: Mask, resource: Resource<Node>) -> Option<()> {
        if !mask.operator.inputs().is_empty() && self.stack.is_empty() {
            return None;
        }

        self.stack.push((resource, mask));

        Some(())
    }

    /// Iterator over all masks in the stack
    pub fn iter(&self) -> impl Iterator<Item = &Mask> {
        self.stack.iter().map(|x| &x.1)
    }

    /// Get size of the mask stack
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// True if and only if the stack is empty
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Get the operator for a specific mask, specified by resource, if it exists.
    pub fn get_operator(&self, mask: &Resource<Node>) -> Option<&Operator> {
        self.stack
            .iter()
            .find_map(|(r, m)| if r == mask { Some(&m.operator) } else { None })
    }

    pub fn set_mask_parameter(&mut self, param: &Resource<Param>, data: &[u8]) {
        if let Some((_, mask)) = self.stack.iter_mut().find(|(r, _)| param.is_param_of(r)) {
            let field = param.fragment().unwrap();
            mask.operator.set_parameter(field, data);
        }
    }

    pub fn set_mask_opacity(&mut self, mask: &Resource<Node>, opacity: f32) {
        if let Some((_, mask)) = self.stack.iter_mut().find(|(r, _)| r == mask) {
            mask.blend_options.opacity = opacity;
        }
    }

    pub fn set_mask_blend_mode(&mut self, mask: &Resource<Node>, blend_mode: BlendMode) {
        if let Some((_, mask)) = self.stack.iter_mut().find(|(r, _)| r == mask) {
            mask.blend_options.blend_mode = blend_mode;
        }
    }

    pub fn set_mask_enabled(&mut self, mask: &Resource<Node>, enabled: bool) {
        if let Some((_, mask)) = self.stack.iter_mut().find(|(r, _)| r == mask) {
            mask.blend_options.enabled = enabled;
        }
    }

    pub fn remove_mask(&mut self, mask: &Resource<Node>) -> Option<Mask> {
        self.stack
            .iter_mut()
            .position(|(r, _)| r == mask)
            .map(|p| self.stack.remove(p).1)
    }
}

/// Options for blending a layer on top of the underlying stack.
#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct LayerBlendOptions {
    /// Mask stack to use. The empty mask stack is effectively ignored.
    mask: MaskStack,
    /// Blend opacity
    opacity: f32,
    /// Channels that should be blended over the bottom stack
    channels: EnumSet<MaterialChannel>,
    /// Blend mode to use, uniform across all channels
    blend_mode: BlendMode,
    /// Whether this layer is enabled at all
    enabled: bool,
}

impl LayerBlendOptions {
    /// Create a blend operator from the given blend options. The output will
    /// *always* be clamped!
    pub fn blend_operator(&self) -> AtomicOperator {
        AtomicOperator::Blend(Blend {
            blend_mode: self.blend_mode,
            mix: self.opacity,
            sharpness: 16.0,
            clamp_output: 1,
        })
    }

    /// True if and only if the layer has a nonempty mask stack
    pub fn has_masks(&self) -> bool {
        !self.mask.stack.is_empty()
    }

    /// Obtain the topmost mask in the stack if it exists.
    pub fn top_mask<F: Fn(&Mask) -> Resource<Node>, G: Fn(&Mask) -> Resource<Node>>(
        &self,
        mask_resource: F,
        blend_resource: G,
    ) -> Option<Resource<Socket>> {
        match self.mask.stack.len() {
            0 => None,
            1 => {
                let mask = self.mask.iter().last().unwrap();
                Some(mask_resource(mask).node_socket(&mask.output_socket))
            }
            _ => Some(blend_resource(self.mask.iter().last().unwrap()).node_socket("color")),
        }
    }
}

impl Default for LayerBlendOptions {
    fn default() -> Self {
        LayerBlendOptions {
            mask: MaskStack::new(),
            opacity: 1.0,
            channels: EnumSet::empty(),
            blend_mode: BlendMode::Mix,
            enabled: true,
        }
    }
}

/// A layer is either a fill layer or an FX layer. Each layer has a name, such
/// that it can be referenced via a Resource. The resource type for a layer is
/// `Resource<Node>`.
///
/// A fill layer using an operator, with its output sockets mapped to
/// material channels. The operator must not have any inputs! It can be
/// complex or atomic. The requirement to not have inputs means that most
/// atomic operators are not usable, outside of noises etc., and the
/// operator is most likely complex.
///
/// An FX layer is a layer that uses the material underneath it as input.
/// Therefore FX layers *cannot* be placed at the bottom of a layer stack.
///
/// Internally there is little difference between Fill and FX layers. Fill
/// layers are simply layers with no input sockets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    /// Internal name of the layer
    name: String,
    /// Human readable name of the layer
    title: String,
    /// Layer operator
    operator: Operator,
    /// Input socket mapping for layers below
    input_sockets: InputMap,
    /// Output socket mapping from sockets to PBR channels
    output_sockets: ChannelMap,
    /// Blend options
    blend_options: LayerBlendOptions,
    /// Type of layer
    layer_type: LayerType,
    /// Type variables for the operator of this layer
    type_variables: HashMap<TypeVariable, ImageType>,
}

impl From<Operator> for Layer {
    fn from(source: Operator) -> Self {
        Self {
            name: "new.layer".to_owned(),
            title: source.title().to_owned(),
            input_sockets: source
                .inputs()
                .iter()
                .map(|(k, _)| (k.clone(), MaterialChannel::Displacement))
                .collect(),
            output_sockets: HashMap::new(),
            blend_options: LayerBlendOptions::default(),
            layer_type: LayerType::Fx,
            type_variables: source
                .inputs()
                .drain()
                .filter_map(|(_, (t, _))| match t {
                    OperatorType::Polymorphic(v) => {
                        Some((v, ImageType::from(MaterialChannel::Displacement)))
                    }
                    _ => None,
                })
                .collect(),
            operator: source,
        }
    }
}

impl Layer {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn outputs(&self) -> &ChannelMap {
        &self.output_sockets
    }

    pub fn inputs(&self) -> Option<&InputMap> {
        match self.layer_type {
            LayerType::Fill => None,
            LayerType::Fx => Some(&self.input_sockets),
        }
    }

    pub fn layer_type(&self) -> LayerType {
        self.layer_type
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.blend_options.opacity = opacity;
    }

    pub fn get_opacity(&self) -> f32 {
        self.blend_options.opacity
    }

    pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        self.blend_options.blend_mode = blend_mode;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.blend_options.enabled = enabled;
    }

    pub fn set_mask_parameter(&mut self, mask: &Resource<Param>, data: &[u8]) {
        self.blend_options.mask.set_mask_parameter(mask, data);
    }

    pub fn set_mask_opacity(&mut self, mask: &Resource<Node>, opacity: f32) {
        self.blend_options.mask.set_mask_opacity(mask, opacity);
    }

    pub fn set_mask_blend_mode(&mut self, mask: &Resource<Node>, blend_mode: BlendMode) {
        self.blend_options
            .mask
            .set_mask_blend_mode(mask, blend_mode);
    }

    pub fn set_mask_enabled(&mut self, mask: &Resource<Node>, enabled: bool) {
        self.blend_options.mask.set_mask_enabled(mask, enabled);
    }

    pub fn get_blend_mode(&self) -> BlendMode {
        self.blend_options.blend_mode
    }

    pub fn get_blend_options(&self) -> &LayerBlendOptions {
        &self.blend_options
    }

    pub fn get_output_channels(&self) -> EnumSet<MaterialChannel> {
        self.blend_options.channels
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_owned();
    }

    pub fn operator(&self) -> &Operator {
        &self.operator
    }

    pub fn get_masks(&self) -> &MaskStack {
        &self.blend_options.mask
    }

    pub fn has_masks(&self) -> bool {
        !self.get_masks().is_empty()
    }

    pub fn push_mask(&mut self, mask: Mask, resource: Resource<Node>) -> Option<()> {
        self.blend_options.mask.push(mask, resource)
    }

    pub fn remove_mask(&mut self, mask: &Resource<Node>) -> Option<Mask> {
        self.blend_options.mask.remove_mask(mask)
    }

    /// Set a type variable on this layer, returning all affected sockets. Since
    /// the layer doesn't know its own resource, the sockets are returned as
    /// strings.
    ///
    /// May fail if the given socket cannot be found or has no type variable.
    pub fn set_type_variable(&mut self, socket: &str, ty: ImageType) -> Option<Vec<String>> {
        let variable = self.operator.type_variable_from_socket(socket)?;
        self.type_variables.insert(variable, ty);

        Some(self.operator.sockets_by_type_variable(variable))
    }

    /// Disable all output channels that do not typecheck
    pub fn type_sanitize_outputs(&mut self) -> Vec<MaterialChannel> {
        let operator = &self.operator;
        let sockets = &mut self.output_sockets;
        let type_vars = &self.type_variables;

        sockets
            .drain_filter(|chan, socket| {
                operator
                    .monomorphic_type(socket, &type_vars)
                    .map(|ty| !chan.legal_for(OperatorType::Monomorphic(ty)))
                    .unwrap_or(false)
            })
            .map(|(c, _)| c)
            .collect()
    }

    /// Determine the number of graph "layers" taken up by this layer, for use
    /// in converting the layer stack to a graph. This is not the total count of
    /// nodes, since channels can be stacked vertically and don't affect the width.
    pub fn graph_width(&self) -> usize {
        2 + 2 * self.get_masks().len()
    }
}

/// A stack of layers, equivalent to a graph of a specific form, that can be
/// linearized or converted.
///
/// Contrary to graphs, layer stacks have a well defined set of outputs, one per
/// PBR channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerStack {
    name: String,
    layers: Vec<(Resource<Node>, Layer)>,
    parameters: HashMap<String, GraphParameter>,
}

impl LayerStack {
    pub fn new(name: &str) -> Self {
        LayerStack {
            name: name.to_owned(),
            layers: Vec::new(),
            parameters: HashMap::new(),
        }
    }

    /// Obtain the next free resource name given a base, see node graph for
    /// analogous function
    fn next_free_name(&self, base_name: &str) -> String {
        use std::collections::HashSet;

        let mut resource = String::new();

        let knowns: HashSet<String> = self
            .layers
            .iter()
            .map(|x| x.0.file().unwrap().to_string())
            .collect();

        for i in 1.. {
            let name = format!("{}.{}", base_name, i);

            if !knowns.contains(&name) {
                resource = name;
                break;
            }
        }

        resource
    }

    /// Obtain the resource name of a layer
    pub fn layer_resource(&self, layer: &Layer) -> Resource<Node> {
        Resource::node(&format!("{}/{}", self.name, layer.name()))
    }

    /// Obtain the resource name of a mask
    pub fn mask_resource(&self, mask: &Mask) -> Resource<Node> {
        Resource::node(&format!("{}/{}", self.name, mask.name))
    }

    /// Obtain the resource name of a mask blend
    pub fn mask_blend_resource(&self, mask: &Mask) -> Resource<Node> {
        Resource::node(&format!("{}/{}.blend", self.name, mask.name))
    }

    /// Obtain the resource name of a layer blend
    pub fn blend_resource(&self, layer: &Layer, channel: MaterialChannel) -> Resource<Node> {
        Resource::node(&format!(
            "{}/{}.blend.{}",
            self.name,
            layer.name(),
            channel.short_name()
        ))
    }

    /// Push a new layer onto the stack.
    fn push(&mut self, layer: Layer, resource: Resource<Node>) {
        self.layers.push((resource, layer));
    }

    /// Push a new layer onto the stack
    pub fn push_layer(
        &mut self,
        mut layer: Layer,
        layer_type: LayerType,
        base_name: &str,
    ) -> Resource<Node> {
        let resource = Resource::node(&format!("{}/{}", self.name, self.next_free_name(base_name)));
        layer.name = resource.file().unwrap().to_owned();
        layer.layer_type = layer_type;
        self.push(layer, resource.clone());
        resource
    }

    /// Push a new mask onto the mask stack for a given layer
    pub fn push_mask(
        &mut self,
        mut mask: Mask,
        for_layer: &Resource<Node>,
        base_name: &str,
    ) -> Option<Resource<Node>> {
        let base_name = format!("{}.mask.{}", for_layer.file().unwrap(), base_name);
        let name = self.next_free_name(&base_name);

        if let Some((_, layer)) = self.layers.iter_mut().find(|(r, _)| r == for_layer) {
            mask.name = name.to_owned();

            let resource = Resource::node(&format!("{}/{}", self.name, name));
            layer.push_mask(mask, resource.clone())?;
            Some(resource)
        } else {
            None
        }
    }

    /// Remove a layer by resource
    pub fn remove_layer(&mut self, resource: &Resource<Node>) -> Option<Layer> {
        self.layers
            .iter()
            .position(|(r, _)| r == resource)
            .map(|p| self.layers.remove(p).1)
    }

    /// Remove a mask by resource
    pub fn remove_mask(&mut self, resource: &Resource<Node>) -> Option<Mask> {
        let parent_resource = layer_resource_from_mask_resource(resource);
        self.layers.iter_mut().find_map(|(r, l)| {
            if r == &parent_resource {
                l.remove_mask(resource)
            } else {
                None
            }
        })
    }

    /// Reset the layer stack, removing all layers
    pub fn reset(&mut self) {
        self.layers.clear();
    }

    /// Obtain the output resource for a material channel
    pub fn output_resource(&self, channel: MaterialChannel) -> Resource<Node> {
        Resource::node(&format!("{}/output.{}", self.name, channel.short_name(),))
    }

    /// Obtain all output resources
    pub fn output_resources(&self) -> Vec<Resource<Node>> {
        MaterialChannel::iter()
            .map(|chan| self.output_resource(chan))
            .collect()
    }

    /// Return all output sockets of the given layer
    pub fn layer_sockets(
        &self,
        layer: &Resource<Node>,
    ) -> Vec<(Resource<Socket>, OperatorType, bool)> {
        if let Some((_, l)) = self.layers.iter().find(|(r, _)| r == layer) {
            l.operator
                .outputs()
                .iter()
                .map(|(s, t)| {
                    (
                        layer.node_socket(s),
                        l.operator
                            .monomorphic_type(s, &l.type_variables)
                            .map(|x| OperatorType::Monomorphic(x))
                            .unwrap_or(*t),
                        l.operator.external_data(),
                    )
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Return all output sockets for the given mask
    pub fn mask_sockets(
        &self,
        layer: &Resource<Node>,
        mask: &Resource<Node>,
    ) -> Vec<(Resource<Socket>, OperatorType, bool)> {
        if let Some((_, l)) = self.layers.iter().find(|(r, _)| r == layer) {
            let op = l.get_masks().get_operator(mask).expect("Unknown mask");
            op.outputs()
                .iter()
                .map(|(s, t)| (mask.node_socket(s), *t, op.external_data()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Return all blend sockets for the given mask
    pub fn mask_blend_sockets(
        &self,
        mask: &Resource<Node>,
    ) -> Vec<(Resource<Socket>, OperatorType)> {
        let mut blend_res = mask.clone();
        let new_name = format!("{}.blend", blend_res.file().unwrap(),);
        blend_res.rename_file(&new_name);
        vec![(
            blend_res.node_socket("color"),
            OperatorType::Monomorphic(ImageType::Grayscale),
        )]
    }

    /// Return all blend sockets of the given layer
    pub fn blend_sockets(&self, layer: &Resource<Node>) -> Vec<(Resource<Socket>, OperatorType)> {
        if let Some((_, layer)) = self.layers.iter().find(|(r, _)| r == layer) {
            MaterialChannel::iter()
                .map(|channel| {
                    (
                        self.blend_resource(layer, channel).node_socket("color"),
                        OperatorType::Monomorphic(ImageType::from(channel)),
                    )
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Set the title, i.e. human readable name, of a layer
    pub fn set_title(&mut self, layer: &Resource<Node>, title: &str) {
        if let Some((_, layer)) = self.layers.iter_mut().find(|(r, _)| r == layer) {
            layer.set_title(title);
        }
    }

    /// Define an output for a layer and material channel
    pub fn set_output(
        &mut self,
        layer: &Resource<Node>,
        channel: MaterialChannel,
        socket_index: usize,
    ) {
        if let Some((_, layer)) = self.layers.iter_mut().find(|(r, _)| r == layer) {
            let socket = layer
                .operator
                .outputs()
                .keys()
                .sorted()
                .nth(socket_index)
                .cloned()
                .unwrap();
            layer.output_sockets.insert(channel, socket);
        }
    }

    /// Define an input for a layer and socket from material channel. Returns a
    /// vector of sockets that had their types changed as a result.
    pub fn set_input(
        &mut self,
        layer_socket: &Resource<Socket>,
        channel: MaterialChannel,
    ) -> Vec<(Resource<Socket>, ImageType)> {
        if let Some((_, l)) = self
            .layers
            .iter_mut()
            .find(|(r, _)| layer_socket.is_socket_of(r))
        {
            if let LayerType::Fx = l.layer_type {
                l.input_sockets
                    .insert(layer_socket.fragment().unwrap().to_owned(), channel);
                let ty = ImageType::from(channel);
                return l
                    .set_type_variable(layer_socket.fragment().unwrap(), ty)
                    .iter()
                    .flatten()
                    .map(|s| (layer_socket.socket_node().node_socket(s), ty))
                    .collect();
            }
        }
        return vec![];
    }

    pub fn type_sanitize_layer(&mut self, layer: &Resource<Node>) -> Vec<MaterialChannel> {
        if let Some((_, l)) = self.layers.iter_mut().find(|(r, _)| r == layer) {
            l.type_sanitize_outputs()
        } else {
            vec![]
        }
    }

    /// Toggle the visibility of an output channel
    pub fn set_output_channel(
        &mut self,
        layer: &Resource<Node>,
        channel: MaterialChannel,
        visibility: bool,
    ) {
        if let Some((_, l)) = self.layers.iter_mut().find(|(r, _)| r == layer) {
            if visibility {
                l.blend_options.channels.insert(channel);
            } else {
                l.blend_options.channels.remove(channel);
            }
        }
    }

    /// Set the opacity of a layer
    pub fn set_layer_opacity(&mut self, layer: &Resource<Node>, opacity: f32) {
        if layer.path_str().unwrap().contains("mask") {
            self.set_mask_opacity(layer, opacity);
        } else if let Some((_, layer)) = self.layers.iter_mut().find(|(r, _)| r == layer) {
            layer.set_opacity(opacity);
        }
    }

    /// Set the opacity of a mask
    fn set_mask_opacity(&mut self, mask: &Resource<Node>, opacity: f32) {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some((_, l)) = self.layers.iter_mut().find(|(r, _)| r == &parent_resource) {
            l.set_mask_opacity(mask, opacity);
        }
    }

    /// Set the blend mode of a layer
    pub fn set_layer_blend_mode(&mut self, layer: &Resource<Node>, blend_mode: BlendMode) {
        if layer.path_str().unwrap().contains("mask") {
            self.set_mask_blend_mode(layer, blend_mode);
        } else if let Some((_, layer)) = self.layers.iter_mut().find(|(r, _)| r == layer) {
            layer.set_blend_mode(blend_mode)
        }
    }

    /// Set the blend mode of a mask
    fn set_mask_blend_mode(&mut self, mask: &Resource<Node>, blend_mode: BlendMode) {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some((_, l)) = self.layers.iter_mut().find(|(r, _)| r == &parent_resource) {
            l.set_mask_blend_mode(mask, blend_mode);
        }
    }

    /// Enable or disable a layer
    pub fn set_layer_enabled(&mut self, layer: &Resource<Node>, enabled: bool) {
        if layer.path_str().unwrap().contains("mask") {
            self.set_mask_enabled(layer, enabled);
        } else if let Some((_, layer)) = self.layers.iter_mut().find(|(r, _)| r == layer) {
            layer.set_enabled(enabled);
        }
    }

    /// Enable or disable a mask
    fn set_mask_enabled(&mut self, mask: &Resource<Node>, enabled: bool) {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some((_, l)) = self.layers.iter_mut().find(|(r, _)| r == &parent_resource) {
            l.set_mask_enabled(mask, enabled);
        }
    }

    /// Determine whether the stack can be linearized in its current state
    pub fn can_linearize(&self) -> bool {
        use super::NodeCollection;

        self.linearize(super::LinearizationMode::TopoSort).is_some()
    }

    /// Attempt positioning a mask according to the LayerDropTarget.
    fn position_mask(&mut self, res: &Resource<Node>, position: &LayerDropTarget) -> Option<()> {
        Some(())
    }

    /// Attempt moving a layer (or mask) to a specified position in the stack's
    /// canonical order.
    pub fn position_layer(
        &mut self,
        res: &Resource<Node>,
        position: &LayerDropTarget,
    ) -> Option<()> {
        if res.path_str().unwrap().contains("mask") {
            self.position_mask(res, position)
        } else {
            let layer = self
                .layers
                .remove(self.layers.iter().position(|(r, _)| r == res)?);
            let mut target = self
                .layers
                .iter()
                .position(|(r, _)| position.target() == r)?;

            if let LayerDropTarget::Above(_) = position {
                target += 1
            }

            self.layers.insert(target, layer);

            Some(())
        }
    }

    /// Convert this layer stack into a node graph, if it is valid.
    pub fn to_graph(&self, parent_size: u32) -> Option<super::nodegraph::NodeGraph> {
        use super::nodegraph::*;

        let mut x = -(self
            .layers
            .iter()
            .map(|(_, l)| l.graph_width())
            .sum::<usize>() as f64
            / 2.0)
            * SLICE_WIDTH;

        let mut last_socket: HashMap<MaterialChannel, (String, String)> = HashMap::new();
        let mut last_mask: Option<(String, String)>;
        let mut graph = NodeGraph::new(&format!("{}_graph", self.name));

        for (_, layer) in self.layers.iter() {
            let op = layer.operator();

            let layer_node = graph.new_node(op, parent_size, None).0;
            graph.position_node(&layer_node, x, 0.0);
            x += SLICE_WIDTH;

            if let Some(inputs) = layer.inputs() {
                for (input, channel) in inputs.iter() {
                    let (input_node, input_socket) = last_socket.get(channel)?;
                    graph
                        .connect_sockets(input_node, input_socket, &layer_node, input)
                        .ok()?;
                }
            }

            if layer.has_masks() {
                let masks = layer.get_masks();
                let (mask_node, mask_socket) =
                    masks.insert_into_graph(&mut graph, x - SLICE_WIDTH, parent_size)?;
                last_mask = Some((mask_node, mask_socket));
                x += masks.len() as f64 * SLICE_WIDTH;
            } else {
                last_mask = None
            }

            let output_channels = layer.get_output_channels();
            for (i, (channel, socket)) in layer
                .outputs()
                .iter()
                .enumerate()
                .filter(|(_, (c, _))| output_channels.contains(**c))
            {
                if let Some((background_node, background_socket)) = last_socket.get(channel) {
                    let blend_op = Operator::from(layer.get_blend_options().blend_operator());
                    let blend_node = graph.new_node(&blend_op, parent_size, None).0;
                    graph.position_node(&blend_node, x, (i + 1) as f64 * SLICE_WIDTH);

                    graph
                        .connect_sockets(
                            background_node,
                            background_socket,
                            &blend_node,
                            "background",
                        )
                        .ok()?;
                    graph
                        .connect_sockets(&layer_node, socket, &blend_node, "foreground")
                        .ok()?;
                    if let Some((mask_node, mask_socket)) = last_mask.as_ref() {
                        graph
                            .connect_sockets(&mask_node, &mask_socket, &blend_node, "mask")
                            .ok()?;
                    }

                    last_socket.insert(*channel, (blend_node, "color".to_owned()));
                } else {
                    last_socket.insert(*channel, (layer_node.to_owned(), socket.to_owned()));
                }
            }
        }

        x += SLICE_WIDTH;
        for channel in MaterialChannel::iter().filter(|channel| last_socket.contains_key(channel)) {
            let output_op = Operator::from(AtomicOperator::Output(Output {
                output_type: OutputType::from(channel),
            }));
            let output_node = graph.new_node(&output_op, parent_size, None).0;
            graph.position_node(&output_node, x, 0.0);

            let (node, socket) = last_socket.get(&channel)?;
            graph
                .connect_sockets(node, socket, &output_node, "data")
                .ok()?;
        }

        Some(graph)
    }
}

impl super::ExposedParameters for LayerStack {
    fn exposed_parameters(&self) -> &HashMap<String, GraphParameter> {
        &self.parameters
    }

    fn exposed_parameters_mut(&mut self) -> &mut HashMap<String, GraphParameter> {
        &mut self.parameters
    }
}

impl super::NodeCollection for LayerStack {
    /// Layer stacks do not have inputs, so this always returns an empty HashMap.
    fn inputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)> {
        HashMap::new()
    }

    /// Layer stacks always have the same set of outputs, one per possible material channel.
    fn outputs(&self) -> HashMap<String, (OperatorType, Resource<Node>)> {
        let channels = self
            .layers
            .iter()
            .map(|(_, l)| l.get_output_channels())
            .fold(EnumSet::empty(), |z, c| z.union(c));

        HashMap::from_iter(channels.iter().map(|channel| {
            (
                channel.short_name().to_string(),
                (
                    OperatorType::Monomorphic(ImageType::from(channel)),
                    self.output_resource(channel),
                ),
            )
        }))
    }

    fn output_type(&self, node: &Resource<Node>) -> Option<OutputType> {
        let channels = self
            .layers
            .iter()
            .map(|(_, l)| l.get_output_channels())
            .fold(EnumSet::empty(), |z, c| z.union(c));
        channels
            .iter()
            .find(|chan| &self.output_resource(*chan) == node)
            .map(|chan| OutputType::from(chan))
    }

    fn graph_resource(&self) -> Resource<Graph> {
        Resource::graph(self.name.clone())
    }

    fn rename(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Linearize this layer stack into a vector of instructions to be
    /// interpreted by the compute backend. Analogous to the similarly named
    /// function in the NodeGraph.
    ///
    /// The linearization mode is ignored for layer stacks.
    fn linearize(&self, _mode: super::LinearizationMode) -> Option<(Linearization, UsePoints)> {
        let mut linearization = Vec::new();
        let mut use_points: HashMap<Resource<Node>, UsePoint> = HashMap::new();
        let mut step = 0;

        let mut last_socket: HashMap<MaterialChannel, Resource<Socket>> = HashMap::new();

        for (resource, layer) in self.layers.iter() {
            // Skip if disabled
            if !layer.blend_options.enabled || layer.blend_options.channels.is_empty() {
                continue;
            }

            step += 1;

            // Clear all optional sockets that have no inputs
            for socket in layer
                .operator
                .inputs()
                .iter()
                .filter_map(|(s, (_, optional))| {
                    if *optional && !layer.input_sockets.contains_key(s) {
                        Some(s)
                    } else {
                        None
                    }
                })
            {
                linearization.push(Instruction::ClearInput(resource.node_socket(socket)));
            }

            match &layer.operator {
                Operator::AtomicOperator(aop) => {
                    // Move inputs. Nop for fill layers
                    for (socket, channel) in layer.input_sockets.iter() {
                        debug_assert!(layer.layer_type != LayerType::Fill);
                        let input_resource = last_socket.get(channel)?.clone();
                        use_points
                            .entry(input_resource.socket_node())
                            .and_modify(|e| e.last = step)
                            .or_insert(UsePoint {
                                last: step,
                                creation: usize::MIN,
                            });

                        linearization.push(Instruction::Move(
                            input_resource,
                            resource.node_socket(socket),
                        ));
                    }
                    linearization.push(Instruction::Execute(resource.clone(), aop.clone()));

                    if let Some(thmbsocket) = aop.outputs().keys().sorted().next() {
                        linearization
                            .push(Instruction::Thumbnail(resource.node_socket(thmbsocket)));
                    }
                }
                Operator::ComplexOperator(cop) => {
                    // Copy inputs to internal sockets. Nop for fill layers
                    for (socket, channel) in layer.input_sockets.iter() {
                        debug_assert!(layer.layer_type != LayerType::Fill);
                        let input = cop.inputs.get(socket).expect("Missing internal socket");
                        let input_resource = last_socket.get(channel)?.clone();
                        use_points
                            .entry(input_resource.socket_node())
                            .and_modify(|e| e.last = step)
                            .or_insert(UsePoint {
                                last: step,
                                creation: usize::MIN,
                            });

                        linearization.push(Instruction::Copy(
                            input_resource,
                            input.1.node_socket("data"),
                        ));
                    }

                    // Call complex operator execution
                    linearization.push(Instruction::Call(resource.clone(), cop.clone()));

                    // Copy back outputs
                    for (out_socket, (_, output)) in cop.outputs.iter() {
                        linearization.push(Instruction::Copy(
                            output.node_socket("data"),
                            resource.node_socket(out_socket),
                        ))
                    }

                    if let Some(thmbsocket) = cop.outputs().keys().sorted().next() {
                        linearization
                            .push(Instruction::Thumbnail(resource.node_socket(thmbsocket)));
                    }
                }
            }

            use_points
                .entry(resource.clone())
                .and_modify(|e| e.creation = step)
                .or_insert(UsePoint {
                    last: usize::MAX,
                    creation: step,
                });

            if layer.blend_options.has_masks() {
                layer.blend_options.mask.linearize_into(
                    |mask| self.mask_blend_resource(mask),
                    &mut linearization,
                    &mut use_points,
                    &mut step,
                );
            }

            for (channel, socket) in layer.output_sockets.iter() {
                // Skip blending if channel is not selected
                if !layer.blend_options.channels.contains(*channel) {
                    continue;
                }

                if let Some(background) = last_socket.get(channel).cloned() {
                    step += 1;

                    let blend_res = self.blend_resource(layer, *channel);

                    use_points
                        .entry(background.socket_node())
                        .and_modify(|e| e.last = step)
                        .or_insert(UsePoint {
                            last: step,
                            creation: usize::MIN,
                        });
                    use_points
                        .entry(resource.clone())
                        .and_modify(|e| e.last = step)
                        .or_insert(UsePoint {
                            last: step,
                            creation: usize::MIN,
                        });

                    linearization.push(Instruction::Move(
                        background,
                        blend_res.node_socket("background"),
                    ));
                    linearization.push(Instruction::Move(
                        resource.node_socket(socket),
                        blend_res.node_socket("foreground"),
                    ));

                    // Handle mask or lack thereof
                    match layer.blend_options.top_mask(
                        |mask| self.mask_resource(mask),
                        |mask| self.mask_blend_resource(mask),
                    ) {
                        Some(mask_res) => {
                            linearization
                                .push(Instruction::Move(mask_res, blend_res.node_socket("mask")));
                        }
                        None => {
                            linearization
                                .push(Instruction::ClearInput(blend_res.node_socket("mask")));
                        }
                    }

                    linearization.push(Instruction::Execute(
                        blend_res.clone(),
                        layer.blend_options.blend_operator(),
                    ));

                    use_points
                        .entry(blend_res.clone())
                        .and_modify(|e| e.creation = step)
                        .or_insert(UsePoint {
                            last: usize::MAX,
                            creation: step,
                        });

                    last_socket.insert(*channel, blend_res.node_socket("color"));
                } else {
                    last_socket.insert(*channel, resource.node_socket(socket));
                }
            }
        }

        // Finally process the (virtual) output operators
        for channel in MaterialChannel::iter() {
            step += 1;

            let output = self.output_resource(channel);
            if let Some(socket) = last_socket.get(&channel).cloned() {
                use_points
                    .entry(socket.socket_node())
                    .and_modify(|e| e.last = step)
                    .or_insert(UsePoint {
                        last: step,
                        creation: usize::MIN,
                    });

                linearization.push(Instruction::Move(socket, output.node_socket("data")));
                linearization.push(Instruction::Execute(
                    output,
                    AtomicOperator::Output(Output {
                        output_type: OutputType::from(channel),
                    }),
                ));
            }
        }

        Some((linearization, use_points.drain().collect()))
    }

    fn parameter_change(&mut self, resource: &Resource<Param>, data: &[u8]) -> Option<Lang> {
        if resource.path_str().unwrap().contains("mask") {
            let res_file = resource.file().unwrap();
            let pos = res_file.find(".mask").unwrap();

            let mut parent_resource = resource.clone();
            parent_resource.rename_file(&res_file[..pos]);
            let parent_node = parent_resource.parameter_node();

            if let Some((_, layer)) = self.layers.iter_mut().find(|(r, _)| r == &parent_node) {
                layer.set_mask_parameter(resource, data);
            }
        } else {
            let field = resource.fragment().unwrap();

            if let Some((_, layer)) = self
                .layers
                .iter_mut()
                .find(|(r, _)| resource.is_param_of(r))
            {
                layer.operator.set_parameter(field, data);
            }
        }

        None
    }

    fn update_complex_operators(
        &mut self,
        _parent_size: u32,
        graph: &Resource<Graph>,
        new: &ComplexOperator,
    ) -> (Vec<super::ComplexOperatorUpdate>, Vec<GraphEvent>) {
        let mut updated = Vec::new();

        for (_, layer) in self.layers.iter_mut() {
            let complex = match &mut layer.operator {
                Operator::ComplexOperator(co) if &co.graph == graph => co,
                _ => continue,
            };

            complex.graph = new.graph.clone();
            complex.title = new.title.clone();
            complex.inputs = new.inputs.clone();
            complex.outputs = new.outputs.clone();

            for (field, subs) in &new.parameters {
                if complex.parameters.get(field).is_none() {
                    complex.parameters.insert(field.clone(), subs.clone());
                }
            }

            for (_, subs) in complex.parameters.iter_mut() {
                subs.resource_mut().set_graph(new.graph.path())
            }

            let params = complex.parameters.clone();
            updated.push((layer.to_owned(), params));
        }

        (
            updated
                .drain(0..)
                .map(|(l, p)| (self.layer_resource(&l), p))
                .collect(),
            vec![],
        )
    }

    fn resize_all(&mut self, parent_size: u32) -> Vec<Lang> {
        let mut evs = vec![];
        for (res, layer) in self.layers.iter() {
            // Main Layer
            evs.push(Lang::GraphEvent(GraphEvent::NodeResized(
                res.clone(),
                layer
                    .operator()
                    .size_request()
                    .map(|s| OperatorSize::AbsoluteSize(s))
                    .unwrap_or_default()
                    .absolute(parent_size),
                layer.operator().scalable(),
            )));

            // Blends
            for channel in layer.get_blend_options().channels.iter() {
                evs.push(Lang::GraphEvent(GraphEvent::NodeResized(
                    self.blend_resource(layer, channel),
                    parent_size,
                    true,
                )));
            }

            // Mask Stack
            for mask in layer.get_masks().iter() {
                evs.push(Lang::GraphEvent(GraphEvent::NodeResized(
                    self.mask_resource(mask),
                    mask.operator
                        .size_request()
                        .map(|s| OperatorSize::AbsoluteSize(s))
                        .unwrap_or_default()
                        .absolute(parent_size),
                    mask.operator.scalable(),
                )));

                evs.push(Lang::GraphEvent(GraphEvent::NodeResized(
                    self.mask_blend_resource(mask),
                    parent_size,
                    true,
                )));
            }
        }

        evs
    }

    fn rebuild_events(&self, parent_size: u32) -> Vec<Lang> {
        self.layers
            .iter()
            .map(|(res, layer)| {
                let mut evs = Vec::new();

                let mut sockets = self.layer_sockets(&res);
                let mut blend_sockets = self.blend_sockets(&res);

                evs.push(Lang::LayersEvent(LayersEvent::LayerPushed(
                    res.clone(),
                    layer.layer_type(),
                    layer.title().to_owned(),
                    layer.operator().clone(),
                    layer.get_blend_mode(),
                    layer.get_opacity(),
                    ParamBoxDescription::empty(),
                    parent_size,
                )));
                evs.extend(sockets.drain(0..).map(|(s, t, e)| {
                    Lang::GraphEvent(GraphEvent::OutputSocketAdded(s, t, e, parent_size))
                }));
                evs.extend(blend_sockets.drain(0..).map(|(s, t)| {
                    Lang::GraphEvent(GraphEvent::OutputSocketAdded(s, t, false, parent_size))
                }));

                for mask in layer.get_masks().iter() {
                    let mask_res = self.mask_resource(mask);

                    let mut sockets = self.mask_sockets(&res, &mask_res);
                    let mut blend_sockets = self.mask_blend_sockets(&mask_res);

                    evs.push(Lang::LayersEvent(LayersEvent::MaskPushed(
                        res.clone(),
                        mask_res,
                        mask.operator.title().to_owned(),
                        mask.operator.clone(),
                        mask.blend_options.blend_mode,
                        mask.blend_options.opacity,
                        ParamBoxDescription::empty(),
                        parent_size,
                    )));

                    evs.extend(sockets.drain(0..).map(|(s, t, e)| {
                        Lang::GraphEvent(GraphEvent::OutputSocketAdded(s, t, e, parent_size))
                    }));
                    evs.extend(blend_sockets.drain(0..).map(|(s, t)| {
                        Lang::GraphEvent(GraphEvent::OutputSocketAdded(s, t, false, parent_size))
                    }));
                }

                evs
            })
            .flatten()
            .collect()
    }

    fn element_param_box(&self, element: &Resource<Node>) -> ParamBoxDescription<MessageWriters> {
        match self.layers.iter().find(|(r, _)| r == element) {
            Some((_, l)) => match l.layer_type {
                LayerType::Fill => {
                    ParamBoxDescription::fill_layer_parameters(&l.operator, &l.output_sockets)
                        .transmitters_into()
                }
                LayerType::Fx => {
                    ParamBoxDescription::fx_layer_parameters(&l.operator).transmitters_into()
                }
            },
            None => ParamBoxDescription::empty(),
        }
    }
}

fn layer_resource_from_mask_resource(mask: &Resource<Node>) -> Resource<Node> {
    let res_file = mask.file().unwrap();
    let pos = res_file.find(".mask").unwrap();

    let mut parent_resource = mask.clone();
    parent_resource.rename_file(&res_file[..pos]);

    parent_resource
}
