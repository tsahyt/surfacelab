use crate::lang::resource as r;
use crate::lang::*;

use conrod_core::{image, text, Point};
use enum_dispatch::*;
use std::collections::{HashMap, VecDeque};

use super::i18n;

#[enum_dispatch]
trait Collection {
    fn rename_collection(&mut self, to: &Resource<r::Graph>);
    fn exposed_parameters(&mut self) -> &mut Vec<(String, GraphParameter)>;
    fn collection_parameters(&mut self) -> &mut ParamBoxDescription<GraphField>;
    fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id);
    fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id>;
    fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        op: &ComplexOperator,
        pbox: &ParamBoxDescription<MessageWriters>,
    );
}

#[derive(Debug, Clone)]
pub struct Graph {
    pub graph: NodeGraph,
    resources: HashMap<Resource<r::Node>, petgraph::graph::NodeIndex>,
    exposed_parameters: Vec<(String, GraphParameter)>,
    param_box: ParamBoxDescription<GraphField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeData {
    pub resource: Resource<Node>,
    pub thumbnail: Option<image::Id>,
    pub position: Point,
    pub title: String,
    pub inputs: Vec<(String, OperatorType)>,
    pub outputs: Vec<(String, OperatorType)>,
    pub type_variables: HashMap<TypeVariable, ImageType>,
    pub param_box: ParamBoxDescription<MessageWriters>,
}

impl NodeData {
    pub fn new(
        resource: Resource<Node>,
        position: Option<Point>,
        operator: &Operator,
        param_box: ParamBoxDescription<MessageWriters>,
    ) -> Self {
        let mut inputs: Vec<_> = operator
            .inputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        inputs.sort();
        let mut outputs: Vec<_> = operator
            .outputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        outputs.sort();
        let title = operator.title().to_owned();
        Self {
            resource,
            title,
            inputs,
            outputs,
            param_box,
            thumbnail: None,
            position: position.unwrap_or([0., 0.]),
            type_variables: HashMap::new(),
        }
    }

    pub fn update(&mut self, operator: Operator, param_box: ParamBoxDescription<MessageWriters>) {
        let mut inputs: Vec<_> = operator
            .inputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        inputs.sort();
        self.inputs = inputs;
        let mut outputs: Vec<_> = operator
            .outputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        outputs.sort();
        self.outputs = outputs;
        self.title = operator.title().to_owned();

        self.param_box = param_box;
    }

    pub fn set_type_variable(&mut self, var: TypeVariable, ty: Option<ImageType>) {
        match ty {
            Some(ty) => self.type_variables.insert(var, ty),
            None => self.type_variables.remove(&var),
        };
    }
}

pub type NodeGraph = petgraph::Graph<NodeData, (String, String)>;

impl Graph {
    pub fn new(name: &str) -> Self {
        Self {
            graph: petgraph::Graph::new(),
            resources: HashMap::new(),
            exposed_parameters: Vec::new(),
            param_box: ParamBoxDescription::graph_parameters(name),
        }
    }

    pub fn insert_index(&mut self, resource: Resource<r::Node>, index: petgraph::graph::NodeIndex) {
        self.resources.insert(resource, index);
    }

    pub fn remove_index(&mut self, resource: &Resource<r::Node>) {
        self.resources.remove(resource);
    }
}

impl Collection for Graph {
    fn rename_collection(&mut self, to: &Resource<r::Graph>) {
        self.param_box.categories[0].parameters[0].control = Control::Entry {
            value: to.file().unwrap().to_string(),
        };
        for gp in self.exposed_parameters.iter_mut().map(|x| &mut x.1) {
            gp.parameter.set_graph(to.path());
        }
        for (mut res, idx) in self.resources.drain().collect::<Vec<_>>() {
            res.set_graph(to.path());
            self.resources.insert(res.clone(), idx);
            self.graph.node_weight_mut(idx).unwrap().resource = res;
        }
    }

    fn exposed_parameters(&mut self) -> &mut Vec<(String, GraphParameter)> {
        &mut self.exposed_parameters
    }

    fn collection_parameters(&mut self) -> &mut ParamBoxDescription<GraphField> {
        &mut self.param_box
    }

    fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id) {
        if let Some(node) = self
            .resources
            .get(node)
            .copied()
            .and_then(|idx| self.graph.node_weight_mut(idx))
        {
            node.thumbnail = Some(thumbnail);
        }
    }

    fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        let mut old_id = None;

        if let Some(node) = self
            .resources
            .get(node)
            .copied()
            .and_then(|idx| self.graph.node_weight_mut(idx))
        {
            old_id = node.thumbnail;
            node.thumbnail = None;
        }

        old_id
    }

    fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        op: &ComplexOperator,
        pbox: &ParamBoxDescription<MessageWriters>,
    ) {
        if let Some(idx) = self.resources.get(node) {
            let node_weight = self.graph.node_weight_mut(*idx).unwrap();
            node_weight.update(Operator::ComplexOperator(op.clone()), pbox.clone());
        }
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new("base")
    }
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub resource: Resource<Node>,
    pub title: String,
    pub icon: super::util::IconName,
    pub thumbnail: Option<image::Id>,
    pub operator_pbox: ParamBoxDescription<MessageWriters>,
    pub opacity: f32,
    pub blend_mode: usize,
    pub enabled: bool,
    pub is_mask: bool,
}

impl Layer {
    pub fn layer(
        resource: Resource<Node>,
        ty: LayerType,
        title: &str,
        pbox: ParamBoxDescription<MessageWriters>,
        blend_mode: usize,
        opacity: f32,
    ) -> Self {
        Self {
            resource,
            title: title.to_owned(),
            icon: match ty {
                LayerType::Fill => super::util::IconName::SOLID,
                LayerType::Fx => super::util::IconName::FX,
            },
            thumbnail: None,
            operator_pbox: pbox,
            opacity,
            blend_mode,
            enabled: true,
            is_mask: false,
        }
    }

    pub fn mask(
        resource: Resource<Node>,
        title: &str,
        pbox: ParamBoxDescription<MessageWriters>,
        blend_mode: usize,
        opacity: f32,
    ) -> Self {
        Self {
            resource,
            title: title.to_owned(),
            icon: super::util::IconName::MASK,
            thumbnail: None,
            operator_pbox: pbox,
            opacity,
            blend_mode,
            enabled: true,
            is_mask: true,
        }
    }

    pub fn update(&mut self, param_box: ParamBoxDescription<MessageWriters>) {
        self.operator_pbox = param_box;
    }
}

#[derive(Debug, Clone)]
pub struct Layers {
    pub layers: VecDeque<Layer>,
    exposed_parameters: Vec<(String, GraphParameter)>,
    param_box: ParamBoxDescription<GraphField>,
}

impl Layers {
    pub fn new(name: &str) -> Self {
        Self {
            layers: VecDeque::new(),
            exposed_parameters: Vec::new(),
            param_box: ParamBoxDescription::graph_parameters(name),
        }
    }

    pub fn rows(&self) -> usize {
        self.layers.iter().len()
    }

    pub fn move_up(&mut self, layer: &Resource<r::Node>) {
        let idx_range = self.indices_for(layer);
        let to_move: Vec<_> = self.layers.drain(idx_range.clone()).rev().collect();

        let insertion_point = if self
            .layers
            .get(idx_range.start)
            .map(|l| l.is_mask)
            .unwrap_or(false)
        {
            idx_range.start.saturating_sub(1)
        } else {
            self.layers
                .iter()
                .take(idx_range.start)
                .enumerate()
                .rev()
                .skip_while(|(_, l)| l.is_mask)
                .map(|x| x.0)
                .next()
                .unwrap_or(0)
        };

        for l in to_move {
            self.layers.insert(insertion_point, l);
        }
    }

    pub fn move_down(&mut self, layer: &Resource<r::Node>) {
        let idx_range = self.indices_for(layer);
        let to_move: Vec<_> = self.layers.drain(idx_range.clone()).rev().collect();

        let insertion_point = if self
            .layers
            .get(idx_range.start)
            .map(|l| l.is_mask)
            .unwrap_or(false)
        {
            (idx_range.start + 1).min(self.layers.len())
        } else {
            self.layers
                .iter()
                .enumerate()
                .skip(idx_range.start + 1)
                .skip_while(|(_, l)| l.is_mask)
                .map(|x| x.0)
                .next()
                .unwrap_or(self.layers.len())
        };

        for l in to_move {
            self.layers.insert(insertion_point, l);
        }
    }

    /// Return all indices belonging to a layer including its masks. If given a
    /// mask, it will only return the index for this mask.
    fn indices_for(&self, layer: &Resource<r::Node>) -> std::ops::Range<usize> {
        let start = self
            .layers
            .iter()
            .position(|l| &l.resource == layer)
            .expect("Unknown layer");
        if self.layers[start].is_mask {
            return std::ops::Range {
                start,
                end: start + 1,
            };
        }

        let end = self
            .layers
            .iter()
            .enumerate()
            .skip(start + 1)
            .take_while(|(_, l)| l.is_mask)
            .last()
            .map(|x| x.0 + 1);

        match end {
            Some(end) => std::ops::Range { start, end },
            None => std::ops::Range {
                start,
                end: start + 1,
            },
        }
    }
}

impl Collection for Layers {
    fn rename_collection(&mut self, to: &Resource<r::Graph>) {
        self.param_box.categories[0].parameters[0].control = Control::Entry {
            value: to.file().unwrap().to_string(),
        };
        for gp in self.exposed_parameters.iter_mut().map(|x| &mut x.1) {
            gp.parameter.set_graph(to.path());
        }
    }

    fn exposed_parameters(&mut self) -> &mut Vec<(String, GraphParameter)> {
        &mut self.exposed_parameters
    }

    fn collection_parameters(&mut self) -> &mut ParamBoxDescription<GraphField> {
        &mut self.param_box
    }

    fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id) {
        if let Some(layer) = self.layers.iter_mut().find(|l| &l.resource == node) {
            layer.thumbnail = Some(thumbnail);
        }
    }

    fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        let mut old_thumbnail = None;

        if let Some(layer) = self.layers.iter_mut().find(|l| &l.resource == node) {
            old_thumbnail = layer.thumbnail;
            layer.thumbnail = None
        }

        old_thumbnail
    }

    fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        _op: &ComplexOperator,
        pbox: &ParamBoxDescription<MessageWriters>,
    ) {
        if let Some(layer) = self.layers.iter_mut().find(|l| &l.resource == node) {
            layer.update(pbox.clone());
        }
    }
}

#[enum_dispatch(Collection)]
#[derive(Debug, Clone)]
pub enum NodeCollection {
    Graph(Graph),
    Layers(Layers),
}

impl NodeCollection {
    pub fn as_graph_mut(&mut self) -> Option<&mut Graph> {
        match self {
            NodeCollection::Graph(g) => Some(g),
            NodeCollection::Layers(_) => None,
        }
    }

    pub fn as_layers_mut(&mut self) -> Option<&mut Layers> {
        match self {
            NodeCollection::Graph(_) => None,
            NodeCollection::Layers(l) => Some(l),
        }
    }
}

impl Default for NodeCollection {
    fn default() -> Self {
        Self::Graph(Graph::default())
    }
}

#[derive(Debug)]
pub struct NodeCollections {
    collections: HashMap<Resource<r::Graph>, NodeCollection>,
    active_collection: NodeCollection,
    active_resource: Resource<r::Graph>,
}

impl Default for NodeCollections {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeCollections {
    pub fn new() -> Self {
        Self {
            collections: HashMap::new(),
            active_collection: NodeCollection::default(),
            active_resource: Resource::graph("base", None),
        }
    }

    pub fn active_parameters(
        &mut self,
        active_node_element: Option<petgraph::graph::NodeIndex>,
        active_layer_element: Option<usize>,
    ) -> Option<(&mut ParamBoxDescription<MessageWriters>, &Resource<r::Node>)> {
        match &mut self.active_collection {
            NodeCollection::Graph(g) => {
                let ae = active_node_element?;
                let node = g.graph.node_weight_mut(ae)?;
                Some((&mut node.param_box, &node.resource))
            }
            NodeCollection::Layers(l) => {
                let ae = active_layer_element?;
                let layer = l.layers.get_mut(ae)?;
                Some((&mut layer.operator_pbox, &layer.resource))
            }
        }
    }

    pub fn set_active(&mut self, collection: Resource<r::Graph>) {
        self.collections
            .insert(self.active_resource.clone(), self.active_collection.clone());
        self.active_resource = collection;
        self.active_collection = self.collections.remove(&self.active_resource).unwrap();
    }

    pub fn rename_collection(&mut self, from: &Resource<r::Graph>, to: &Resource<r::Graph>) {
        if &self.active_resource == from {
            self.active_resource = to.clone();
            self.active_collection.rename_collection(to);
        } else if let Some(mut graph) = self.collections.remove(from) {
            graph.rename_collection(to);
            self.collections.insert(to.clone(), graph);
        }
    }

    pub fn get_active(&self) -> &Resource<r::Graph> {
        &self.active_resource
    }

    pub fn get_active_collection(&self) -> &NodeCollection {
        &self.active_collection
    }

    pub fn get_active_collection_mut(&mut self) -> &mut NodeCollection {
        &mut self.active_collection
    }

    pub fn clear_all(&mut self) {
        self.active_collection = NodeCollection::Graph(Graph::default());
        self.collections.clear();
    }

    pub fn add_graph(&mut self, graph: Resource<r::Graph>) {
        self.collections.insert(
            graph.clone(),
            NodeCollection::Graph(Graph::new(graph.file().unwrap())),
        );
    }

    pub fn add_layers(&mut self, graph: Resource<r::Graph>) {
        self.collections.insert(
            graph.clone(),
            NodeCollection::Layers(Layers::new(graph.file().unwrap())),
        );
    }

    /// Get a list of collection names for displaying
    pub fn list_collection_names(&self) -> Vec<&str> {
        std::iter::once(self.active_resource.file().unwrap())
            .chain(self.collections.keys().map(|k| k.file().unwrap()))
            .collect()
    }

    /// Get a reference to the resource denominating the collection at the given
    /// index. This index refers to the ordering returned by `list_graph_names`.
    pub fn get_collection_resource(&self, index: usize) -> Option<&Resource<r::Graph>> {
        std::iter::once(&self.active_resource)
            .chain(self.collections.keys())
            .nth(index)
    }

    /// Get a slice of the exposed graph parameters of the currently active
    /// graph.
    pub fn get_exposed_parameters_mut(&mut self) -> &mut [(String, GraphParameter)] {
        self.active_collection.exposed_parameters()
    }

    pub fn get_collection_parameters_mut(&mut self) -> &mut ParamBoxDescription<GraphField> {
        self.active_collection.collection_parameters()
    }

    fn target_graph_from_node(&mut self, node: &Resource<r::Node>) -> Option<&mut Graph> {
        self.target_collection_from_node(node)
            .and_then(|x| x.as_graph_mut())
    }

    fn target_layers_from_node(&mut self, node: &Resource<r::Node>) -> Option<&mut Layers> {
        self.target_collection_from_node(node)
            .and_then(|x| x.as_layers_mut())
    }

    fn target_collection_from_node(
        &mut self,
        node: &Resource<r::Node>,
    ) -> Option<&mut NodeCollection> {
        let graph_name = node.directory().unwrap();
        let graph_res = Resource::graph(graph_name, None);

        if self.active_resource == graph_res {
            Some(&mut self.active_collection)
        } else {
            self.collections.get_mut(&graph_res)
        }
    }

    fn target_collection_from_collection(
        &mut self,
        collection_res: &Resource<r::Graph>,
    ) -> Option<&mut NodeCollection> {
        if &self.active_resource == collection_res {
            Some(&mut self.active_collection)
        } else {
            self.collections.get_mut(&collection_res)
        }
    }

    /// Push a layer onto the parent layer stack. This is a NOP if the parent
    /// collection is a graph.
    pub fn push_layer(&mut self, layer: Layer) {
        let layer_res = layer.resource.clone();

        if let Some(target) = self.target_layers_from_node(&layer_res) {
            target.layers.push_front(layer);
        }
    }

    pub fn push_layer_under(&mut self, layer: Layer, under: &Resource<Node>) {
        let layer_res = layer.resource.clone();

        if let Some(target) = self.target_layers_from_node(&layer_res) {
            let pos = target
                .layers
                .iter()
                .position(|l| &l.resource == under)
                .unwrap();
            target.layers.insert(pos + 1, layer);
        }
    }

    pub fn remove_layer(&mut self, layer_res: &Resource<Node>) {
        if let Some(target) = self.target_layers_from_node(&layer_res) {
            target.layers.remove(
                target
                    .layers
                    .iter()
                    .position(|l| &l.resource == layer_res)
                    .expect("Trying to remove unknown layer"),
            );
        }
    }

    /// Add a node to a graph, based on the resource data given. This is a NOP
    /// if the parent graph is a layer.
    pub fn add_node(&mut self, node: NodeData) {
        let node_res = node.resource.clone();

        if let Some(target) = self.target_graph_from_node(&node_res) {
            let idx = target.graph.add_node(node);
            target.resources.insert(node_res, idx);
        }
    }

    /// Connect two sockets in a graph. This is a NOP for layers.
    pub fn connect_sockets(&mut self, from: &Resource<r::Socket>, to: &Resource<r::Socket>) {
        let from_node = from.socket_node();
        if let Some(target) = self.target_graph_from_node(&from_node) {
            let from_idx = target.resources.get(&from_node).unwrap();
            let to_idx = target.resources.get(&to.socket_node()).unwrap();
            target.graph.add_edge(
                *from_idx,
                *to_idx,
                (
                    from.fragment().unwrap().to_string(),
                    to.fragment().unwrap().to_string(),
                ),
            );
        }
    }

    /// Disconnect two sockets in a graph. This is a NOP for layers.
    pub fn disconnect_sockets(&mut self, from: &Resource<r::Socket>, to: &Resource<r::Socket>) {
        let from_node = from.socket_node();
        if let Some(target) = self.target_graph_from_node(&from_node) {
            use petgraph::visit::EdgeRef;

            let from_idx = target.resources.get(&from_node).unwrap();
            let to_idx = target.resources.get(&to.socket_node()).unwrap();

            // Assuming that there's only ever one edge connecting two sockets.
            if let Some(e) = target
                .graph
                .edges_connecting(*from_idx, *to_idx)
                .filter(|e| {
                    (e.weight().0.as_str(), e.weight().1.as_str())
                        == (from.fragment().unwrap(), to.fragment().unwrap())
                })
                .map(|e| e.id())
                .next()
            {
                target.graph.remove_edge(e);
            }
        }
    }

    /// Remove a node from a graph. This is a NOP for layers.
    pub fn remove_node(&mut self, node: &Resource<r::Node>) {
        if let Some(target) = self.target_graph_from_node(&node) {
            if let Some(idx) = target.resources.remove(node) {
                // Obtain last node before removal for reindexing
                let last_idx = target.graph.node_indices().next_back().unwrap();
                let last_res = target.graph.node_weight(last_idx).unwrap().resource.clone();

                target.graph.remove_node(idx);
                target.resources.insert(last_res, idx);
            }
        }
    }

    /// Monomorphize a socket in a graph. This is a NOP for layers.
    pub fn monomorphize_socket(&mut self, socket: &Resource<r::Socket>, ty: ImageType) {
        let node = socket.socket_node();

        if let Some(target) = self.target_graph_from_node(&node) {
            let idx = target.resources.get(&node).unwrap();
            let node = target.graph.node_weight_mut(*idx).unwrap();
            let var = type_variable_from_socket_iter(
                node.inputs.iter().chain(node.outputs.iter()),
                socket.fragment().unwrap(),
            )
            .unwrap();
            node.set_type_variable(var, Some(ty))
        }
    }

    /// Demonomorphize a socket in a graph. This is a NOP for layers.
    pub fn demonomorphize_socket(&mut self, socket: &Resource<r::Socket>) {
        let node = socket.socket_node();

        if let Some(target) = self.target_graph_from_node(&node) {
            let idx = target.resources.get(&node).unwrap();
            let node = target.graph.node_weight_mut(*idx).unwrap();
            let var = type_variable_from_socket_iter(
                node.inputs.iter().chain(node.outputs.iter()),
                socket.fragment().unwrap(),
            )
            .unwrap();
            node.set_type_variable(var, None)
        }
    }

    /// Rename a node. Note that this does *not* support moving a node from one
    /// graph to another!
    pub fn rename_node(&mut self, from: &Resource<r::Node>, to: &Resource<r::Node>) {
        if let Some(target) = self.target_graph_from_node(&from) {
            if let Some(idx) = target.resources.get(from).copied() {
                let node = target.graph.node_weight_mut(idx).unwrap();
                node.resource = to.clone();
                target.resources.insert(to.clone(), idx);
                target.resources.remove(from);
            }
        }
    }

    /// Expose a parameter from a collection, i.e. add it to the list of exposed
    /// parameters to display.
    pub fn parameter_exposed(&mut self, graph: &Resource<r::Graph>, param: GraphParameter) {
        if let Some(target) = self.target_collection_from_collection(graph) {
            target
                .exposed_parameters()
                .push((param.graph_field.clone(), param));
        }
    }

    /// Conceal a parameter from a collection, i.e. remove it from the list of exposed
    /// parameters to display.
    pub fn parameter_concealed(&mut self, graph: &Resource<r::Graph>, field: &str) {
        if let Some(target) = self.target_collection_from_collection(graph) {
            let idx = target
                .exposed_parameters()
                .iter()
                .position(|x| x.0 == field)
                .expect("Tried to remove unknown parameter");
            target.exposed_parameters().remove(idx);
        }
    }

    pub fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        op: &ComplexOperator,
        pbox: &ParamBoxDescription<MessageWriters>,
    ) {
        if let Some(target) = self.target_collection_from_node(node) {
            target.update_complex_operator(node, op, pbox);
        }
    }

    /// Register a thumbnail for a given "node". This works for both graphs and
    /// layers. In the case of a layer, the node is a layer, according to the
    /// resource scheme used for layers.
    pub fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id) {
        if let Some(target) = self.target_collection_from_node(node) {
            target.register_thumbnail(node, thumbnail);
        }
    }

    /// Unregister a thumbnail for a given "node". This works for both graphs and
    /// layers. In the case of a layer, the node is a layer, according to the
    /// resource scheme used for layers.
    pub fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        if let Some(target) = self.target_collection_from_node(node) {
            target.unregister_thumbnail(node)
        } else {
            None
        }
    }

    pub fn move_layer_up(&mut self, layer: &Resource<r::Node>) {
        if let Some(target) = self.target_layers_from_node(&layer) {
            target.move_up(layer);
        }
    }

    pub fn move_layer_down(&mut self, layer: &Resource<r::Node>) {
        if let Some(target) = self.target_layers_from_node(&layer) {
            target.move_down(layer);
        }
    }
}

#[derive(Clone, Debug)]
pub enum LayerFilter {
    Layer(LayerType),
    Mask(Resource<Node>),
}

pub enum RenderImage {
    None,
    Requested,
    Image(image::Id),
}

pub struct App {
    pub language: i18n::Language,
    pub graphs: NodeCollections,
    pub active_node_element: Option<petgraph::graph::NodeIndex>,
    pub active_layer_element: Option<usize>,

    pub render_image: RenderImage,
    pub monitor_resolution: (u32, u32),

    pub add_node_modal: Option<Point>,
    pub add_layer_modal: Option<LayerFilter>,
    pub render_modal: bool,

    pub render_params: ParamBoxDescription<RenderField>,
    pub surface_params: ParamBoxDescription<SurfaceField>,

    pub registered_operators: Vec<Operator>,
    pub addable_operators: Vec<Operator>,
    pub registered_sockets: Vec<super::export_row::RegisteredSocket>,
    pub export_entries: Vec<(String, ExportSpec)>,
}

impl App {
    pub fn new(monitor_size: (u32, u32)) -> Self {
        Self {
            language: i18n::Language::default(),
            graphs: NodeCollections::new(),
            active_node_element: None,
            active_layer_element: None,
            render_image: RenderImage::None,
            monitor_resolution: (monitor_size.0, monitor_size.1),
            add_node_modal: None,
            add_layer_modal: None,
            render_modal: false,
            render_params: ParamBoxDescription::render_parameters(),
            surface_params: ParamBoxDescription::surface_parameters(),
            registered_operators: AtomicOperator::all_default()
                .iter()
                .map(|x| Operator::from(x.clone()))
                .collect(),
            addable_operators: AtomicOperator::all_default()
                .iter()
                .map(|x| Operator::from(x.clone()))
                .collect(),
            registered_sockets: Vec::new(),
            export_entries: Vec::new(),
        }
    }

    pub fn add_export_entry(&mut self) {
        if let Some(default) = self.registered_sockets.last() {
            self.export_entries.push((
                "unnamed".to_owned(),
                ExportSpec::Grayscale(default.spec.clone()),
            ));
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct AppFonts {
    pub text_font: text::font::Id,
    pub icon_font: text::font::Id,
}
