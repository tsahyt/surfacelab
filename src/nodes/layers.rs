use crate::lang::*;
use enumset::EnumSet;
use itertools::Itertools;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::iter::FromIterator;
use strum::IntoEnumIterator;

/// Slice width for graph layouting on conversion
const SLICE_WIDTH: f64 = 256.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// A fill layer using an operator, with its output sockets mapped to
/// material channels. The operator must not have any inputs! It can be
/// complex or atomic. The requirement to not have inputs means that most
/// atomic operators are not usable, outside of noises etc., and the
/// operator is most likely complex.
pub struct FillLayer {
    title: String,
    operator: Operator,
    output_sockets: ChannelMap,
    blend_options: LayerBlendOptions,
}

impl From<Operator> for FillLayer {
    fn from(source: Operator) -> Self {
        Self {
            title: source.title().to_owned(),
            output_sockets: HashMap::new(),
            blend_options: LayerBlendOptions::default(),
            operator: source,
        }
    }
}

/// A type encoding a function from material channels to sockets.
type ChannelMap = HashMap<MaterialChannel, String>;

/// A type encoding a function from sockets to material channels.
type InputMap = HashMap<String, MaterialChannel>;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// An FX layer is a layer that uses the material underneath it as input.
/// Therefore FX layers *cannot* be placed at the bottom of a layer stack.
pub struct FxLayer {
    title: String,
    operator: Operator,
    input_sockets: InputMap,
    output_sockets: ChannelMap,
    blend_options: LayerBlendOptions,
}

impl From<Operator> for FxLayer {
    fn from(source: Operator) -> Self {
        Self {
            title: source.title().to_owned(),
            input_sockets: HashMap::from_iter(
                source
                    .inputs()
                    .drain()
                    .map(|(k, _)| (k, MaterialChannel::Displacement)),
            ),
            output_sockets: HashMap::new(),
            blend_options: LayerBlendOptions::default(),
            operator: source,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// A mask (layer) is any operator that has one input or less, and some number
/// of outputs greater than 0 that can be interpreted as grayscale images. To
/// retain flexibility, color outputs may be allowed, but only the red channel
/// will be used.
///
/// Masks are thus single-channel layers. When blending mask layers, there is no
/// way to apply a mask into the blend, i.e. masks cannot be "recursive".
pub struct Mask {
    name: String,
    operator: Operator,
    output_socket: String,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaskStack {
    stack: Vec<Mask>,
    resources: HashMap<String, usize>,
}

impl MaskStack {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            resources: HashMap::new(),
        }
    }

    /// Move a mask up, i.e. closer to the back of the vector if possible.
    pub fn move_up(&mut self, mask: &Resource<Node>) -> bool {
        if let Some(idx) = self.resources.get(mask.file().unwrap()).copied() {
            let n = self.stack.len();
            if idx == n - 1 {
                return false;
            } else {
                self.stack.swap(idx, idx + 1);

                if self.can_linearize() {
                    let other_name = self.stack[idx].name.clone();
                    self.swap_resources(mask.file().unwrap(), &other_name);
                    true
                } else {
                    self.stack.swap(idx, idx + 1);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Move a mask down, i.e. closer to the front of the vector if possible.
    pub fn move_down(&mut self, mask: &Resource<Node>) -> bool {
        if let Some(idx) = self.resources.get(mask.file().unwrap()).copied() {
            if idx == 0 {
                return false;
            } else {
                self.stack.swap(idx, idx - 1);

                if self.can_linearize() {
                    let other_name = self.stack[idx].name.clone();
                    self.swap_resources(mask.file().unwrap(), &other_name);
                    true
                } else {
                    self.stack.swap(idx, idx - 1);
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn can_linearize(&self) -> bool {
        self.linearize_into(
            |_| Resource::node("", None),
            |_| Resource::node("", None),
            &mut Vec::new(),
            &mut HashMap::new(),
            &mut 0,
        )
        .is_some()
    }

    pub fn linearize_into<F: Fn(&Mask) -> Resource<Node>, G: Fn(&Mask) -> Resource<Node>>(
        &self,
        mask_resource: F,
        blend_resource: G,
        linearization: &mut Linearization,
        use_points: &mut HashMap<Resource<Node>, UsePoint>,
        step: &mut usize,
    ) -> Option<()> {
        let mut last_socket: Option<Resource<Socket>> = None;

        for mask in self.stack.iter().filter(|m| m.blend_options.enabled) {
            *step += 1;

            let resource: Resource<Node> = mask_resource(mask);

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

    pub fn insert_into_graph(
        &self,
        graph: &mut super::nodegraph::NodeGraph,
        mut x: f64,
        parent_size: u32,
    ) -> Option<(String, String)> {
        let mut last_socket: Option<(String, String)> = None;

        let y = SLICE_WIDTH * 2.0;

        x -= SLICE_WIDTH * self.stack.len() as f64;

        for mask in self.stack.iter().filter(|m| m.blend_options.enabled) {
            let mask_node = graph.new_node(&mask.operator, parent_size).0;
            graph.position_node(&mask_node, x, y);
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
                let blend_node = graph.new_node(&blend_op, parent_size).0;
                graph.position_node(&blend_node, x, y);
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

    pub fn push(&mut self, mask: Mask, resource: Resource<Node>) -> Option<()> {
        if mask.operator.inputs().len() != 0 && self.stack.len() == 0 {
            return None;
        }

        self.stack.push(mask);
        self.resources
            .insert(resource.file().unwrap().to_owned(), self.stack.len() - 1);

        Some(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Mask> {
        self.stack.iter()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn get_operator(&self, mask: &Resource<Node>) -> Option<&Operator> {
        self.resources
            .get(mask.file().unwrap())
            .map(|idx| &self.stack[*idx].operator)
    }

    fn swap_resources(&mut self, a: &str, b: &str) {
        debug_assert!(a != b);
        let idx_a = self.resources.get_mut(a).expect("Unknown resource") as *mut usize;
        let idx_b = self.resources.get_mut(b).expect("Unknown resource") as *mut usize;
        unsafe {
            std::ptr::swap(idx_a, idx_b);
        }
    }

    pub fn set_mask_parameter(&mut self, param: &Resource<Param>, data: &[u8]) {
        if let Some(mask) = self
            .resources
            .get(param.file().unwrap())
            .copied()
            .and_then(|idx| self.stack.get_mut(idx))
        {
            let field = param.fragment().unwrap();
            mask.operator.set_parameter(field, data);
        }
    }

    pub fn set_mask_opacity(&mut self, mask: &Resource<Node>, opacity: f32) {
        if let Some(mask) = self
            .resources
            .get(mask.file().unwrap())
            .copied()
            .and_then(|idx| self.stack.get_mut(idx))
        {
            mask.blend_options.opacity = opacity;
        }
    }

    pub fn set_mask_blend_mode(&mut self, mask: &Resource<Node>, blend_mode: BlendMode) {
        if let Some(mask) = self
            .resources
            .get(mask.file().unwrap())
            .copied()
            .and_then(|idx| self.stack.get_mut(idx))
        {
            mask.blend_options.blend_mode = blend_mode;
        }
    }

    pub fn set_mask_enabled(&mut self, mask: &Resource<Node>, enabled: bool) {
        if let Some(mask) = self
            .resources
            .get(mask.file().unwrap())
            .copied()
            .and_then(|idx| self.stack.get_mut(idx))
        {
            mask.blend_options.enabled = enabled;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerBlendOptions {
    mask: MaskStack,
    opacity: f32,
    channels: EnumSet<MaterialChannel>,
    blend_mode: BlendMode,
    enabled: bool,
}

impl LayerBlendOptions {
    /// Create a blend operator from the given blend options. The output will
    /// *always* be clamped!
    pub fn blend_operator(&self) -> AtomicOperator {
        if self.has_masks() {
            AtomicOperator::BlendMasked(BlendMasked {
                blend_mode: self.blend_mode,
                sharpness: 16.0,
                clamp_output: 1,
            })
        } else {
            AtomicOperator::Blend(Blend {
                blend_mode: self.blend_mode,
                mix: self.opacity,
                sharpness: 16.0,
                clamp_output: 1,
            })
        }
    }

    pub fn has_masks(&self) -> bool {
        !self.mask.stack.is_empty()
    }

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

// TODO: See if backend layer representation can be unified to not differentiate between fill and fx

/// A layer is either a fill layer or an FX layer. Each layer has a name, such
/// that it can be referenced via a Resource. The resource type for a layer is
/// `Resource<Node>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Layer {
    FillLayer(String, FillLayer),
    FxLayer(String, FxLayer),
}

impl Layer {
    pub fn name(&self) -> &str {
        match self {
            Layer::FillLayer(s, _) => s,
            Layer::FxLayer(s, _) => s,
        }
    }

    pub fn outputs(&self) -> &ChannelMap {
        match self {
            Layer::FillLayer(_, l) => &l.output_sockets,
            Layer::FxLayer(_, l) => &l.output_sockets,
        }
    }

    pub fn inputs(&self) -> Option<&InputMap> {
        match self {
            Layer::FillLayer(_, _) => None,
            Layer::FxLayer(_, l) => Some(&l.input_sockets),
        }
    }

    pub fn layer_type(&self) -> LayerType {
        match self {
            Layer::FillLayer(_, _) => LayerType::Fill,
            Layer::FxLayer(_, _) => LayerType::Fx,
        }
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.opacity = opacity;
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.opacity = opacity;
            }
        }
    }

    pub fn get_opacity(&self) -> f32 {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => blend_options.opacity,
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => blend_options.opacity,
        }
    }

    pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.blend_mode = blend_mode;
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.blend_mode = blend_mode;
            }
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.enabled = enabled;
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.enabled = enabled;
            }
        }
    }

    pub fn set_mask_parameter(&mut self, mask: &Resource<Param>, data: &[u8]) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_parameter(mask, data);
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_parameter(mask, data);
            }
        }
    }

    pub fn set_mask_opacity(&mut self, mask: &Resource<Node>, opacity: f32) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_opacity(mask, opacity);
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_opacity(mask, opacity);
            }
        }
    }

    pub fn set_mask_blend_mode(&mut self, mask: &Resource<Node>, blend_mode: BlendMode) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_blend_mode(mask, blend_mode);
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_blend_mode(mask, blend_mode);
            }
        }
    }

    pub fn set_mask_enabled(&mut self, mask: &Resource<Node>, enabled: bool) {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_enabled(mask, enabled);
            }
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => {
                blend_options.mask.set_mask_enabled(mask, enabled);
            }
        }
    }

    pub fn get_blend_mode(&self) -> BlendMode {
        self.get_blend_options().blend_mode
    }

    pub fn get_blend_options(&self) -> &LayerBlendOptions {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => blend_options,
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => blend_options,
        }
    }

    pub fn get_output_channels(&self) -> EnumSet<MaterialChannel> {
        match self {
            Layer::FillLayer(_, FillLayer { blend_options, .. }) => blend_options.channels,
            Layer::FxLayer(_, FxLayer { blend_options, .. }) => blend_options.channels,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Layer::FillLayer(_, l) => &l.title,
            Layer::FxLayer(_, l) => &l.title,
        }
    }

    pub fn set_title(&mut self, title: &str) {
        match self {
            Layer::FillLayer(_, l) => {
                l.title = title.to_owned();
            }
            Layer::FxLayer(_, l) => {
                l.title = title.to_owned();
            }
        }
    }

    pub fn operator(&self) -> &Operator {
        match self {
            Layer::FillLayer(_, l) => &l.operator,
            Layer::FxLayer(_, l) => &l.operator,
        }
    }

    pub fn get_masks(&self) -> &MaskStack {
        &self.get_blend_options().mask
    }

    pub fn has_masks(&self) -> bool {
        !self.get_masks().is_empty()
    }

    pub fn push_mask(&mut self, mask: Mask, resource: Resource<Node>) -> Option<()> {
        match self {
            Layer::FillLayer(_, l) => l.blend_options.mask.push(mask, resource),
            Layer::FxLayer(_, l) => l.blend_options.mask.push(mask, resource),
        }
    }

    pub fn move_mask_up(&mut self, mask: &Resource<Node>) -> bool {
        match self {
            Layer::FillLayer(_, l) => l.blend_options.mask.move_up(mask),
            Layer::FxLayer(_, l) => l.blend_options.mask.move_up(mask),
        }
    }

    pub fn move_mask_down(&mut self, mask: &Resource<Node>) -> bool {
        match self {
            Layer::FillLayer(_, l) => l.blend_options.mask.move_down(mask),
            Layer::FxLayer(_, l) => l.blend_options.mask.move_down(mask),
        }
    }

    /// Determine the number of graph "layers" taken up by this layer, for use
    /// in converting the layer stack to a graph. This is not the total count of
    /// nodes, since channels can be stacked vertically and don't affect the width.
    pub fn graph_width(&self) -> usize {
        2 + 2 * self.get_masks().len()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerStack {
    name: String,
    layers: Vec<Layer>,
    resources: HashMap<String, usize>,
    parameters: HashMap<String, GraphParameter>,
    force_points: ForcePoints,
}

impl LayerStack {
    pub fn new(name: &str) -> Self {
        LayerStack {
            name: name.to_owned(),
            layers: Vec::new(),
            resources: HashMap::new(),
            parameters: HashMap::new(),
            force_points: Vec::new(),
        }
    }

    fn next_free_name(&self, base_name: &str) -> String {
        let mut resource = String::new();

        for i in 1.. {
            let name = format!("{}.{}", base_name, i);

            if !self.resources.contains_key(&name) {
                resource = name;
                break;
            }
        }

        resource
    }

    pub fn layer_resource(&self, layer: &Layer) -> Resource<Node> {
        Resource::node(&format!("{}/{}", self.name, layer.name()), None)
    }

    pub fn mask_resource(&self, mask: &Mask) -> Resource<Node> {
        Resource::node(&format!("{}/{}", self.name, mask.name), None)
    }

    pub fn mask_blend_resource(&self, mask: &Mask) -> Resource<Node> {
        Resource::node(&format!("{}/{}.blend", self.name, mask.name), None)
    }

    pub fn material_layer_resource(
        &self,
        layer: &Layer,
        channel: MaterialChannel,
    ) -> Resource<Node> {
        Resource::node(
            &format!("{}/{}.{}", self.name, layer.name(), channel.short_name()),
            None,
        )
    }

    pub fn blend_resource(&self, layer: &Layer, channel: MaterialChannel) -> Resource<Node> {
        Resource::node(
            &format!(
                "{}/{}.blend.{}",
                self.name,
                layer.name(),
                channel.short_name()
            ),
            None,
        )
    }

    fn push(&mut self, layer: Layer, resource: &Resource<Node>) {
        self.layers.push(layer);
        self.resources
            .insert(resource.file().unwrap().to_owned(), self.layers.len() - 1);
    }

    pub fn push_fill(&mut self, layer: FillLayer, base_name: &str) -> Resource<Node> {
        let resource = Resource::node(
            &format!("{}/{}", self.name, self.next_free_name(base_name)),
            None,
        );
        let layer = Layer::FillLayer(resource.file().unwrap().to_owned(), layer);
        self.push(layer, &resource);
        resource
    }

    pub fn push_fx(&mut self, layer: FxLayer, base_name: &str) -> Resource<Node> {
        let resource = Resource::node(
            &format!("{}/{}", self.name, self.next_free_name(base_name)),
            None,
        );
        let layer = Layer::FxLayer(resource.file().unwrap().to_owned(), layer);
        self.push(layer, &resource);
        resource
    }

    pub fn push_mask(
        &mut self,
        mut mask: Mask,
        for_layer: &Resource<Node>,
        base_name: &str,
    ) -> Option<Resource<Node>> {
        if let Some(idx) = self.resources.get(for_layer.file().unwrap()) {
            let base_name = format!("{}.mask.{}", for_layer.file().unwrap(), base_name);
            let name = self.next_free_name(&base_name);
            mask.name = name.to_owned();

            let resource = Resource::node(&format!("{}/{}", self.name, name), None);

            self.layers[*idx].push_mask(mask, resource.clone())?;

            Some(resource)
        } else {
            None
        }
    }

    pub fn remove(&mut self, resource: &Resource<Node>) -> Option<Layer> {
        if let Some(index) = self.resources.remove(resource.file().unwrap()) {
            let layer = self.layers.remove(index);

            for idx in self.resources.values_mut().filter(|i| **i >= index) {
                *idx -= 1;
            }

            Some(layer)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.layers.clear();
        self.resources.clear();
    }

    pub fn output_resource(&self, channel: MaterialChannel) -> Resource<Node> {
        Resource::node(
            &format!("{}/output.{}", self.name, channel.short_name(),),
            None,
        )
    }

    pub fn all_resources(&self) -> Vec<Resource<Node>> {
        self.layers
            .iter()
            .map(|layer| match layer {
                Layer::FillLayer(_, FillLayer { blend_options, .. }) => blend_options
                    .channels
                    .iter()
                    .map(|channel| self.blend_resource(layer, channel))
                    .chain(std::iter::once(self.layer_resource(layer)))
                    .collect::<Vec<_>>(),
                Layer::FxLayer(_, FxLayer { blend_options, .. }) => blend_options
                    .channels
                    .iter()
                    .map(|channel| self.blend_resource(layer, channel))
                    .chain(std::iter::once(self.layer_resource(layer)))
                    .collect::<Vec<_>>(),
            })
            .flatten()
            .collect()
    }

    /// Return all output sockets of the given layer
    pub fn layer_sockets(
        &self,
        layer: &Resource<Node>,
    ) -> Vec<(Resource<Socket>, OperatorType, bool)> {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            match &self.layers[*idx] {
                Layer::FillLayer(_, fill) => fill
                    .operator
                    .outputs()
                    .iter()
                    .map(|(s, t)| (layer.node_socket(s), *t, fill.operator.external_data()))
                    .collect(),
                Layer::FxLayer(_, fx) => fx
                    .operator
                    .outputs()
                    .iter()
                    .map(|(s, t)| (layer.node_socket(s), *t, fx.operator.external_data()))
                    .collect(),
            }
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
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            let op = self.layers[*idx]
                .get_masks()
                .get_operator(mask)
                .expect("Unknown mask");
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
        blend_res.modify_path(|pb| pb.set_file_name(new_name));
        vec![(
            blend_res.node_socket("color"),
            OperatorType::Monomorphic(ImageType::Grayscale),
        )]
    }

    /// Return all blend sockets of the given layer
    pub fn blend_sockets(&self, layer: &Resource<Node>) -> Vec<(Resource<Socket>, OperatorType)> {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            let layer = &self.layers[*idx];
            MaterialChannel::iter()
                .map(|channel| {
                    (
                        self.blend_resource(layer, channel).node_socket("color"),
                        OperatorType::Monomorphic(channel.to_image_type()),
                    )
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn set_title(&mut self, layer: &Resource<Node>, title: &str) {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            self.layers[*idx].set_title(title);
        }
    }

    pub fn set_output(
        &mut self,
        layer: &Resource<Node>,
        channel: MaterialChannel,
        socket_index: usize,
    ) {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            match &mut self.layers[*idx] {
                Layer::FillLayer(_, fill) => {
                    let socket = fill
                        .operator
                        .outputs()
                        .keys()
                        .sorted()
                        .nth(socket_index)
                        .cloned()
                        .unwrap();
                    fill.output_sockets.insert(channel, socket);
                }
                Layer::FxLayer(_, fx) => {
                    let socket = fx
                        .operator
                        .outputs()
                        .keys()
                        .sorted()
                        .nth(socket_index)
                        .cloned()
                        .unwrap();
                    fx.output_sockets.insert(channel, socket);
                }
            }
        }
    }

    pub fn set_input(&mut self, layer_socket: &Resource<Socket>, channel: MaterialChannel) {
        if let Some(idx) = self.resources.get(layer_socket.file().unwrap()) {
            if let Layer::FxLayer(_, fx) = &mut self.layers[*idx] {
                fx.input_sockets
                    .insert(layer_socket.fragment().unwrap().to_owned(), channel);
            }
        }
    }

    pub fn set_output_channel(
        &mut self,
        layer: &Resource<Node>,
        channel: MaterialChannel,
        visibility: bool,
    ) {
        if let Some(idx) = self.resources.get(layer.file().unwrap()) {
            match &mut self.layers[*idx] {
                Layer::FillLayer(_, fill) => {
                    if visibility {
                        fill.blend_options.channels.insert(channel);
                    } else {
                        fill.blend_options.channels.remove(channel);
                    }
                }
                Layer::FxLayer(_, fx) => {
                    if visibility {
                        fx.blend_options.channels.insert(channel);
                    } else {
                        fx.blend_options.channels.remove(channel);
                    }
                }
            }
        }
    }

    pub fn set_layer_opacity(&mut self, layer: &Resource<Node>, opacity: f32) {
        if layer.path_str().unwrap().contains("mask") {
            self.set_mask_opacity(layer, opacity);
        } else {
            if let Some(idx) = self.resources.get(layer.file().unwrap()) {
                self.layers[*idx].set_opacity(opacity);
            }
        }
    }

    fn set_mask_opacity(&mut self, mask: &Resource<Node>, opacity: f32) {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some(idx) = self.resources.get(parent_resource.file().unwrap()) {
            self.layers[*idx].set_mask_opacity(mask, opacity);
        }
    }

    pub fn set_layer_blend_mode(&mut self, layer: &Resource<Node>, blend_mode: BlendMode) {
        if layer.path_str().unwrap().contains("mask") {
            self.set_mask_blend_mode(layer, blend_mode);
        } else {
            if let Some(idx) = self.resources.get(layer.file().unwrap()) {
                self.layers[*idx].set_blend_mode(blend_mode)
            }
        }
    }

    fn set_mask_blend_mode(&mut self, mask: &Resource<Node>, blend_mode: BlendMode) {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some(idx) = self.resources.get(parent_resource.file().unwrap()) {
            self.layers[*idx].set_mask_blend_mode(mask, blend_mode);
        }
    }

    pub fn set_layer_enabled(&mut self, layer: &Resource<Node>, enabled: bool) {
        if layer.path_str().unwrap().contains("mask") {
            self.set_mask_enabled(layer, enabled);
        } else {
            if let Some(idx) = self.resources.get(layer.file().unwrap()) {
                self.layers[*idx].set_enabled(enabled);
                if let Some(successor) = self.layers.get(*idx + 1) {
                    self.force_points.push(self.layer_resource(successor));
                }
            }
        }
    }

    fn set_mask_enabled(&mut self, mask: &Resource<Node>, enabled: bool) {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some(idx) = self.resources.get(parent_resource.file().unwrap()) {
            self.layers[*idx].set_mask_enabled(mask, enabled);
        }
    }

    pub fn clear_force_points(&mut self) {
        self.force_points.clear();
    }

    pub fn can_linearize(&self) -> bool {
        use super::NodeCollection;

        self.linearize(super::LinearizationMode::TopoSort).is_some()
    }

    /// Move a layer up one position in the stack, i.e. closer to the back of
    /// the vector. Returns moved resources in a linear depiction of the whole
    /// stack including masks.
    pub fn move_up(&mut self, layer: &Resource<Node>) -> bool {
        if layer.path_str().unwrap().contains("mask") {
            self.move_mask_up(layer)
        } else {
            if let Some(idx) = self.resources.get(layer.file().unwrap()).copied() {
                if idx == self.layers.len() - 1 {
                    return false;
                } else {
                    self.layers.swap(idx, idx + 1);
                    if self.can_linearize() {
                        self.swap_resources(
                            layer.file().unwrap(),
                            &self.layers[idx].name().to_owned(),
                        );
                        true
                    } else {
                        self.layers.swap(idx, idx + 1);
                        false
                    }
                }
            } else {
                false
            }
        }
    }

    fn move_mask_up(&mut self, mask: &Resource<Node>) -> bool {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some(idx) = self.resources.get(parent_resource.file().unwrap()) {
            self.layers[*idx].move_mask_up(mask)
        } else {
            false
        }
    }

    /// Move a layer down one position in the stack. Returns moved resources in
    /// a linear depiction of the whole stack including masks.
    pub fn move_down(&mut self, layer: &Resource<Node>) -> bool {
        if layer.path_str().unwrap().contains("mask") {
            self.move_mask_down(layer)
        } else {
            if let Some(idx) = self.resources.get(layer.file().unwrap()).copied() {
                if idx == 0 {
                    false
                } else {
                    self.layers.swap(idx, idx - 1);
                    if self.can_linearize() {
                        self.swap_resources(
                            layer.file().unwrap(),
                            &self.layers[idx].name().to_owned(),
                        );
                        true
                    } else {
                        self.layers.swap(idx, idx - 1);
                        false
                    }
                }
            } else {
                false
            }
        }
    }

    fn move_mask_down(&mut self, mask: &Resource<Node>) -> bool {
        let parent_resource = layer_resource_from_mask_resource(mask);

        if let Some(idx) = self.resources.get(parent_resource.file().unwrap()) {
            self.layers[*idx].move_mask_down(mask)
        } else {
            false
        }
    }

    fn swap_resources(&mut self, a: &str, b: &str) {
        debug_assert!(a != b);
        let idx_a = self.resources.get_mut(a).expect("Unknown resource") as *mut usize;
        let idx_b = self.resources.get_mut(b).expect("Unknown resource") as *mut usize;
        unsafe {
            std::ptr::swap(idx_a, idx_b);
        }
    }

    /// Convert this layer stack into a node graph, if it is valid.
    pub fn to_graph(&self, parent_size: u32) -> Option<super::nodegraph::NodeGraph> {
        use super::nodegraph::*;

        let mut x = -(self.layers.iter().map(|l| l.graph_width()).sum::<usize>() as f64 / 2.0)
            * SLICE_WIDTH;

        let mut last_socket: HashMap<MaterialChannel, (String, String)> = HashMap::new();
        let mut last_mask: Option<(String, String)>;
        let mut graph = NodeGraph::new(&format!("{}_graph", self.name));

        for layer in self.layers.iter() {
            let op = layer.operator();

            let layer_node = graph.new_node(op, parent_size).0;
            graph.position_node(&layer_node, x, SLICE_WIDTH);
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
                let (mask_node, mask_socket) = masks.insert_into_graph(&mut graph, x, parent_size)?;
                last_mask = Some((mask_node, mask_socket));
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
                    let blend_node = graph.new_node(&blend_op, parent_size).0;
                    graph.position_node(&blend_node, x, i as f64 * SLICE_WIDTH);

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
                output_type: channel.to_output_type(),
            }));
            let output_node = graph.new_node(&output_op, parent_size).0;
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
            .map(|l| l.get_output_channels())
            .fold(EnumSet::empty(), |z, c| z.union(c));

        HashMap::from_iter(channels.iter().map(|channel| {
            (
                channel.short_name().to_string(),
                (
                    OperatorType::Monomorphic(channel.to_image_type()),
                    self.output_resource(channel),
                ),
            )
        }))
    }

    fn graph_resource(&self) -> Resource<Graph> {
        Resource::graph(self.name.clone(), None)
    }

    fn rename(&mut self, name: &str) {
        self.name = name.to_string();
    }

    /// Linearize this layer stack into a vector of instructions to be
    /// interpreted by the compute backend. Analogous to the similarly named
    /// function in the NodeGraph.
    ///
    /// The linearization mode is ignored for layer stacks.
    fn linearize(
        &self,
        _mode: super::LinearizationMode,
    ) -> Option<(Linearization, UsePoints, ForcePoints)> {
        let mut linearization = Vec::new();
        let mut use_points: HashMap<Resource<Node>, UsePoint> = HashMap::new();
        let mut step = 0;

        let mut last_socket: HashMap<MaterialChannel, Resource<Socket>> = HashMap::new();

        for layer in self.layers.iter() {
            match layer {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        blend_options,
                        operator,
                        output_sockets,
                        ..
                    },
                ) => {
                    // Skip execution if no channels are blended or the layer is disabled.
                    if blend_options.channels.is_empty() || !blend_options.enabled {
                        continue;
                    }

                    step += 1;

                    let resource = self.layer_resource(layer);
                    match operator {
                        Operator::AtomicOperator(aop) => {
                            linearization.push(Instruction::Execute(resource.clone(), aop.clone()));
                            if let Some(thmbsocket) = aop.outputs().keys().sorted().next() {
                                linearization
                                    .push(Instruction::Thumbnail(resource.node_socket(thmbsocket)));
                            }
                        }
                        Operator::ComplexOperator(cop) => {
                            // Inputs can be skipped vs the nodegraph
                            // linearization, since fill layers must not have
                            // inputs.
                            linearization.push(Instruction::Call(resource.clone(), cop.clone()));
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

                    if blend_options.has_masks() {
                        blend_options.mask.linearize_into(
                            |mask| self.mask_resource(mask),
                            |mask| self.mask_blend_resource(mask),
                            &mut linearization,
                            &mut use_points,
                            &mut step,
                        );
                    }

                    for (channel, socket) in output_sockets.iter() {
                        // Skip blending if channel is not selected
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        if let Some(background) = last_socket.get(channel) {
                            step += 1;

                            let blend_res = self.blend_resource(layer, *channel);

                            use_points
                                .entry(resource.clone())
                                .and_modify(|e| e.last = step)
                                .or_insert(UsePoint {
                                    last: step,
                                    creation: usize::MIN,
                                });
                            use_points
                                .entry(background.socket_node())
                                .and_modify(|e| e.last = step)
                                .or_insert(UsePoint {
                                    last: step,
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
                            if let Some(mask_res) = blend_options.top_mask(
                                |mask| self.mask_resource(mask),
                                |mask| self.mask_blend_resource(mask),
                            ) {
                                linearization.push(Instruction::Move(
                                    mask_res,
                                    blend_res.node_socket("mask"),
                                ));
                            }
                            linearization.push(Instruction::Execute(
                                blend_res.clone(),
                                blend_options.blend_operator(),
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
                Layer::FxLayer(
                    _,
                    FxLayer {
                        operator,
                        input_sockets,
                        output_sockets,
                        blend_options,
                        ..
                    },
                ) => {
                    // Skip if disabled
                    if !blend_options.enabled {
                        continue;
                    }

                    step += 1;

                    let resource = self.layer_resource(layer);

                    match operator {
                        Operator::AtomicOperator(aop) => {
                            // Move inputs
                            for (socket, channel) in input_sockets.iter() {
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
                            // Copy inputs to internal sockets
                            for (socket, channel) in input_sockets.iter() {
                                let input =
                                    cop.inputs.get(socket).expect("Missing internal socket");
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

                    if blend_options.has_masks() {
                        blend_options.mask.linearize_into(
                            |mask| self.mask_resource(mask),
                            |mask| self.mask_blend_resource(mask),
                            &mut linearization,
                            &mut use_points,
                            &mut step,
                        );
                    }

                    for (channel, socket) in output_sockets.iter() {
                        // Skip blending if channel is not selected
                        if !blend_options.channels.contains(*channel) {
                            continue;
                        }

                        let blend_res = self.blend_resource(layer, *channel);

                        if let Some(background) = last_socket.get(channel).cloned() {
                            step += 1;

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
                            if let Some(mask_res) = blend_options.top_mask(
                                |mask| self.mask_resource(mask),
                                |mask| self.mask_blend_resource(mask),
                            ) {
                                linearization.push(Instruction::Move(
                                    mask_res,
                                    blend_res.node_socket("mask"),
                                ));
                            }
                            linearization.push(Instruction::Execute(
                                blend_res.clone(),
                                blend_options.blend_operator(),
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
                        output_type: channel.to_output_type(),
                    }),
                ));
            }
        }

        Some((
            linearization,
            use_points.drain().collect(),
            self.force_points.clone(),
        ))
    }

    fn parameter_change(
        &mut self,
        resource: &Resource<Param>,
        data: &[u8],
    ) -> Result<Option<Lang>, String> {
        if resource.path_str().unwrap().contains("mask") {
            let res_file = resource.file().unwrap();
            let pos = res_file.find(".mask").unwrap();

            let mut parent_resource = resource.clone();
            parent_resource.modify_path(|pb| pb.set_file_name(&res_file[..pos]));

            if let Some(idx) = self
                .resources
                .get(parent_resource.parameter_node().file().unwrap())
            {
                self.layers[*idx].set_mask_parameter(resource, data);
            }
        } else {
            let field = resource.fragment().unwrap();

            if let Some(idx) = self
                .resources
                .get(resource.parameter_node().file().unwrap())
            {
                match &mut self.layers[*idx] {
                    Layer::FillLayer(_, FillLayer { operator, .. }) => {
                        operator.set_parameter(field, data)
                    }
                    Layer::FxLayer(_, layer) => layer.operator.set_parameter(field, data),
                }
            }
        }

        Ok(None)
    }

    fn update_complex_operators(
        &mut self,
        _parent_size: u32,
        graph: &Resource<Graph>,
        new: &ComplexOperator,
    ) -> (Vec<super::ComplexOperatorUpdate>, Vec<GraphEvent>) {
        let mut updated = Vec::new();

        for layer in self.layers.iter_mut() {
            let complex = match layer {
                Layer::FillLayer(
                    _,
                    FillLayer {
                        operator: Operator::ComplexOperator(co),
                        ..
                    },
                ) if &co.graph == graph => co,
                Layer::FxLayer(
                    _,
                    FxLayer {
                        operator: Operator::ComplexOperator(co),
                        ..
                    },
                ) if &co.graph == graph => co,
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
            // TODO: find a way to avoid this clone
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
        self.all_resources()
            .drain(0..)
            .map(|res| Lang::GraphEvent(GraphEvent::NodeResized(res, parent_size)))
            .collect()
    }

    fn rebuild_events(&self, parent_size: u32) -> Vec<Lang> {
        self.layers
            .iter()
            .map(|layer| {
                let mut evs = Vec::new();

                let res = self.layer_resource(layer);

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
        if let Some(idx) = self.resources.get(element.file().unwrap()) {
            match &self.layers[*idx] {
                Layer::FillLayer(_, l) => {
                    ParamBoxDescription::fill_layer_parameters(&l.operator, &l.output_sockets)
                        .map_transmitters(|t| t.clone().into())
                }
                Layer::FxLayer(_, l) => ParamBoxDescription::fx_layer_parameters(&l.operator)
                    .map_transmitters(|t| t.clone().into()),
            }
        } else {
            ParamBoxDescription::empty()
        }
    }
}

fn layer_resource_from_mask_resource(mask: &Resource<Node>) -> Resource<Node> {
    let res_file = mask.file().unwrap();
    let pos = res_file.find(".mask").unwrap();

    let mut parent_resource = mask.clone();
    parent_resource.modify_path(|pb| pb.set_file_name(&res_file[..pos]));

    parent_resource
}
