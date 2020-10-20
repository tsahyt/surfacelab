use crate::lang::resource as r;
use crate::lang::*;

use conrod_core::{image, text, Point};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Graph {
    pub graph: super::graph::NodeGraph,
    resources: HashMap<Resource<r::Node>, petgraph::graph::NodeIndex>,
    exposed_parameters: Vec<(String, GraphParameter)>,
    param_box: ParamBoxDescription<GraphField>,
}

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

impl Default for Graph {
    fn default() -> Self {
        Self::new("base")
    }
}

#[derive(Debug)]
pub struct Layer {
    resource: Resource<Node>,
    title: String,
    icon: super::util::IconName,
    thumbnail: Option<image::Id>,
    operator_pbox: ParamBoxDescription<Field>,
    layer_pbox: ParamBoxDescription<Field>,
    masks: Vec<Mask>,
}

#[derive(Debug)]
pub struct Mask {}

#[derive(Debug)]
pub struct Layers {
    layers: Vec<Layer>,
    exposed_parameters: Vec<(String, GraphParameter)>,
    param_box: ParamBoxDescription<GraphField>,
}

#[derive(Debug)]
pub enum NodeCollection {
    Graph(Graph),
    Layers(Layers),
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

    pub fn set_active(&mut self, collection: Resource<r::Graph>) {
        self.collections
            .insert(self.active_resource, self.active_collection);
        self.active_resource = collection;
        self.active_collection = self.collections.remove(&self.active_resource).unwrap();
    }

    pub fn rename_graph(&mut self, from: &Resource<r::Graph>, to: &Resource<r::Graph>) {
        // fn update(target: &mut NodeCollection, to: &Resource<r::Graph>) {
        //     target.param_box.categories[0].parameters[0].control = Control::Entry {
        //         value: to.file().unwrap().to_string(),
        //     };
        //     for gp in target.exposed_parameters.iter_mut().map(|x| &mut x.1) {
        //         gp.parameter.set_graph(to.path());
        //     }
        //     for (mut res, idx) in target.resources.drain().collect::<Vec<_>>() {
        //         res.set_graph(to.path());
        //         target.resources.insert(res.clone(), idx);
        //         target.graph.node_weight_mut(idx).unwrap().resource = res;
        //     }
        // }

        // if &self.active_resource == from {
        //     self.active_resource = to.clone();
        //     update(&mut self.active_collection, to);
        // } else if let Some(mut graph) = self.collections.remove(from) {
        //     update(&mut graph, to);
        //     self.collections.insert(to.clone(), graph);
        // }
        todo!()
    }

    pub fn get_active(&self) -> &Resource<r::Graph> {
        &self.active_resource
    }

    pub fn get_active_graph(&self) -> &Graph {
        // &self.active_collection
        todo!()
    }

    pub fn get_active_graph_mut(&mut self) -> &mut Graph {
        // &mut self.active_collection
        todo!()
    }

    pub fn clear_all(&mut self) {
        self.active_collection = NodeCollection::Graph(Graph::default());
        self.collections.clear();
    }

    pub fn add_graph(&mut self, graph: Resource<r::Graph>) {
        self.collections
            .insert(graph.clone(), NodeCollection::Graph(Graph::new(graph.file().unwrap())));
    }

    pub fn add_layers(&mut self, graph: Resource<r::Graph>) {
        todo!()
        // self.collections
        //     .insert(graph.clone(), Graph::new(graph.file().unwrap()));
    }

    /// Get a list of collection names for displaying
    pub fn list_collection_names(&self) -> Vec<&str> {
        std::iter::once(self.active_resource.file().unwrap())
            .chain(self.collections.keys().map(|k| k.file().unwrap()))
            .collect()
    }

    /// Get a reference to the resource denominating the graph at the given
    /// index. This index refers to the ordering returned by `list_graph_names`.
    pub fn get_graph_resource(&self, index: usize) -> Option<&Resource<r::Graph>> {
        std::iter::once(&self.active_resource)
            .chain(self.collections.keys())
            .nth(index)
    }

    /// Get a slice of the exposed graph parameters of the currently active
    /// graph.
    pub fn get_exposed_parameters_mut(&mut self) -> &mut [(String, GraphParameter)] {
        // &mut self.active_collection.exposed_parameters
        todo!()
    }

    pub fn get_graph_parameters_mut(&mut self) -> &mut ParamBoxDescription<GraphField> {
        // &mut self.active_collection.param_box
        todo!()
    }

    fn target_graph_from_node(&mut self, node: &Resource<r::Node>) -> Option<&mut Graph> {
        // let graph_name = node.directory().unwrap();
        // let graph_res = Resource::graph(graph_name, None);

        // if self.active_resource == graph_res {
        //     Some(&mut self.active_collection)
        // } else {
        //     self.collections.get_mut(&graph_res)
        // }
        todo!()
    }

    fn target_graph_from_graph(&mut self, graph_res: &Resource<r::Graph>) -> Option<&mut Graph> {
        // if &self.active_resource == graph_res {
        //     Some(&mut self.active_collection)
        // } else {
        //     self.collections.get_mut(&graph_res)
        // }
        todo!()
    }

    pub fn add_node(&mut self, node: super::graph::NodeData) {
        // let node_res = node.resource.clone();

        // if let Some(target) = self.target_graph_from_node(&node_res) {
        //     let idx = target.graph.add_node(node);
        //     target.resources.insert(node_res, idx);
        // }
        todo!()
    }

    pub fn connect_sockets(&mut self, from: &Resource<r::Socket>, to: &Resource<r::Socket>) {
        // let from_node = from.socket_node();
        // if let Some(target) = self.target_graph_from_node(&from_node) {
        //     let from_idx = target.resources.get(&from_node).unwrap();
        //     let to_idx = target.resources.get(&to.socket_node()).unwrap();
        //     target.graph.add_edge(
        //         *from_idx,
        //         *to_idx,
        //         (
        //             from.fragment().unwrap().to_string(),
        //             to.fragment().unwrap().to_string(),
        //         ),
        //     );
        // }
        todo!()
    }

    pub fn disconnect_sockets(&mut self, from: &Resource<r::Socket>, to: &Resource<r::Socket>) {
        // let from_node = from.socket_node();
        // if let Some(target) = self.target_graph_from_node(&from_node) {
        //     use petgraph::visit::EdgeRef;

        //     let from_idx = target.resources.get(&from_node).unwrap();
        //     let to_idx = target.resources.get(&to.socket_node()).unwrap();

        //     // Assuming that there's only ever one edge connecting two sockets.
        //     if let Some(e) = target
        //         .graph
        //         .edges_connecting(*from_idx, *to_idx)
        //         .filter(|e| {
        //             (e.weight().0.as_str(), e.weight().1.as_str())
        //                 == (from.fragment().unwrap(), to.fragment().unwrap())
        //         })
        //         .map(|e| e.id())
        //         .next()
        //     {
        //         target.graph.remove_edge(e);
        //     }
        // }
        todo!()
    }

    pub fn remove_node(&mut self, node: &Resource<r::Node>) {
        // if let Some(target) = self.target_graph_from_node(&node) {
        //     if let Some(idx) = target.resources.remove(node) {
        //         // Obtain last node before removal for reindexing
        //         let last_idx = target.graph.node_indices().next_back().unwrap();
        //         let last_res = target.graph.node_weight(last_idx).unwrap().resource.clone();

        //         target.graph.remove_node(idx);
        //         target.resources.insert(last_res, idx);
        //     }
        // }
        todo!()
    }

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

    pub fn parameter_exposed(&mut self, graph: &Resource<r::Graph>, param: GraphParameter) {
        if let Some(target) = self.target_graph_from_graph(graph) {
            target
                .exposed_parameters
                .push((param.graph_field.clone(), param));
        }
    }

    pub fn parameter_concealed(&mut self, graph: &Resource<r::Graph>, field: &str) {
        if let Some(target) = self.target_graph_from_graph(graph) {
            target.exposed_parameters.remove(
                target
                    .exposed_parameters
                    .iter()
                    .position(|x| x.0 == field)
                    .expect("Tried to remove unknown parameter"),
            );
        }
    }

    pub fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        op: &ComplexOperator,
        pbox: &ParamBoxDescription<Field>,
    ) {
        if let Some(target) = self.target_graph_from_node(node) {
            if let Some(idx) = target.resources.get(node) {
                let node_weight = target.graph.node_weight_mut(*idx).unwrap();
                node_weight.update(Operator::ComplexOperator(op.clone()), pbox.clone());
            }
        }
    }

    pub fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id) {
        if let Some(node) = self.target_graph_from_node(node).and_then(|target| {
            let idx = target.resources.get(node)?;
            target.graph.node_weight_mut(*idx)
        }) {
            node.thumbnail = Some(thumbnail);
        }
    }

    pub fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        let mut old_id = None;

        if let Some(node) = self.target_graph_from_node(node).and_then(|target| {
            let idx = target.resources.get(node)?;
            target.graph.node_weight_mut(*idx)
        }) {
            old_id = node.thumbnail;
            node.thumbnail = None;
        }

        old_id
    }
}

pub struct App {
    pub graphs: NodeCollections,
    pub active_element: Option<petgraph::graph::NodeIndex>,
    pub render_image: Option<image::Id>,
    pub monitor_resolution: (u32, u32),

    pub add_modal: Option<Point>,
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
            graphs: NodeCollections::new(),
            active_element: None,
            render_image: None,
            monitor_resolution: (monitor_size.0, monitor_size.1),
            add_modal: None,
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

    pub fn active_parameters(
        &mut self,
    ) -> Option<(&mut ParamBoxDescription<MessageWriters>, &Resource<r::Node>)> {
        // let ae = self.active_element?;
        // let node = self.graphs.active_collection.graph.node_weight_mut(ae)?;
        // Some((&mut node.param_box, &node.resource))
        todo!()
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
