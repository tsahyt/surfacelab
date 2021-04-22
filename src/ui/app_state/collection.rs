use crate::lang::resource as r;
use crate::lang::*;

use conrod_core::image;
use enum_dispatch::*;
use std::collections::HashMap;

use super::graph;
use super::layers;

#[enum_dispatch]
pub trait Collection {
    fn rename_collection(&mut self, to: &Resource<r::Graph>);
    fn exposed_parameters(&mut self) -> &mut Vec<(String, GraphParameter)>;
    fn collection_parameters(&mut self) -> &mut ParamBoxDescription<GraphField>;
    fn expose_parameter(&mut self, param: GraphParameter);
    fn conceal_parameter(&mut self, field: &str);
    fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id);
    fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id>;
    fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        op: &ComplexOperator,
        pbox: &ParamBoxDescription<MessageWriters>,
    );
    fn active_element(
        &mut self,
    ) -> Option<(&Resource<r::Node>, &mut ParamBoxDescription<MessageWriters>)>;
    fn active_resource(&self) -> Option<&Resource<r::Node>>;
    fn set_active(&mut self, element: &Resource<r::Node>);
}

#[enum_dispatch(Collection)]
#[derive(Debug, Clone)]
pub enum NodeCollection {
    Graph(graph::Graph),
    Layers(layers::Layers),
}

impl NodeCollection {
    pub fn as_graph_mut(&mut self) -> Option<&mut graph::Graph> {
        match self {
            NodeCollection::Graph(g) => Some(g),
            NodeCollection::Layers(_) => None,
        }
    }

    pub fn as_layers_mut(&mut self) -> Option<&mut layers::Layers> {
        match self {
            NodeCollection::Graph(_) => None,
            NodeCollection::Layers(l) => Some(l),
        }
    }
}

impl Default for NodeCollection {
    fn default() -> Self {
        Self::Graph(graph::Graph::default())
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
            active_resource: Resource::graph("base"),
        }
    }

    pub fn active_parameters(
        &mut self,
    ) -> Option<(&Resource<r::Node>, &mut ParamBoxDescription<MessageWriters>)> {
        match &mut self.active_collection {
            NodeCollection::Graph(g) => g.active_element(),
            NodeCollection::Layers(l) => l.active_element(),
        }
    }

    pub fn get_active_element(&self) -> Option<&Resource<r::Node>> {
        match &self.active_collection {
            NodeCollection::Graph(g) => g.active_resource(),
            NodeCollection::Layers(l) => l.active_resource(),
        }
    }

    pub fn set_active_collection(&mut self, collection: Resource<r::Graph>) {
        self.collections
            .insert(self.active_resource.clone(), self.active_collection.clone());
        self.active_resource = collection;
        self.active_collection = self.collections.remove(&self.active_resource).unwrap();
    }

    /// Sets the currently active element in the collection of node collections.
    /// This will *also* change the active collection if the supplied element is
    /// not part of the currently active collection!
    pub fn set_active_element(&mut self, node: Resource<r::Node>) {
        let graph = node.node_graph();
        if graph != self.active_resource {
            self.set_active_collection(graph);
        }

        match &mut self.active_collection {
            NodeCollection::Graph(g) => g.set_active(&node),
            NodeCollection::Layers(l) => l.set_active(&node),
        }
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
        self.active_collection = NodeCollection::Graph(graph::Graph::default());
        self.collections.clear();
    }

    pub fn add_graph(&mut self, graph: Resource<r::Graph>) {
        self.collections.insert(
            graph.clone(),
            NodeCollection::Graph(graph::Graph::new(graph.file().unwrap())),
        );
    }

    pub fn remove_graph(&mut self, graph: &Resource<r::Graph>) {
        self.collections.remove(graph);
        if &self.active_resource == graph {
            self.set_active_collection(self.collections.keys().next().unwrap().clone());
        }
    }

    pub fn add_layers(&mut self, graph: Resource<r::Graph>) {
        self.collections.insert(
            graph.clone(),
            NodeCollection::Layers(layers::Layers::new(graph.file().unwrap())),
        );
    }

    pub fn remove_layers(&mut self, graph: &Resource<r::Graph>) {
        self.collections.remove(graph);
        if &self.active_resource == graph {
            self.set_active_collection(self.collections.keys().next().unwrap().clone());
        }
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

    fn target_graph_from_node(&mut self, node: &Resource<r::Node>) -> Option<&mut graph::Graph> {
        self.target_collection_from_node(node)
            .and_then(|x| x.as_graph_mut())
    }

    fn target_layers_from_node(&mut self, node: &Resource<r::Node>) -> Option<&mut layers::Layers> {
        self.target_collection_from_node(node)
            .and_then(|x| x.as_layers_mut())
    }

    fn target_collection_from_node(
        &mut self,
        node: &Resource<r::Node>,
    ) -> Option<&mut NodeCollection> {
        let graph_name = node.directory().unwrap();
        let graph_res = Resource::graph(graph_name);

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
    pub fn push_layer(&mut self, layer: layers::Layer) {
        use id_tree::{InsertBehavior::*, Node};

        let layer_res = layer.resource.clone();

        if let Some(target) = self.target_layers_from_node(&layer_res) {
            let root = target.layers.root_node_id().unwrap().clone();
            target
                .layers
                .insert(Node::new(layer), UnderNode(&root))
                .expect("Layer insert failed");
        }
    }

    pub fn push_layer_under(&mut self, layer: layers::Layer, under: &Resource<Node>) {
        use id_tree::{InsertBehavior::*, Node};
        let layer_res = layer.resource.clone();

        if let Some(target) = self.target_layers_from_node(&layer_res) {
            let n = target
                .layers
                .traverse_pre_order_ids(target.layers.root_node_id().unwrap())
                .unwrap()
                .find(|i| &target.layers.get(i).unwrap().data().resource == under)
                .expect("Trying to remove unknown layer");
            target
                .layers
                .insert(Node::new(layer), UnderNode(&n))
                .expect("Mask insert failed");
        }
    }

    /// Removes a layer or mask
    pub fn remove_layer(&mut self, layer_res: &Resource<Node>) {
        use id_tree::RemoveBehavior::*;

        if let Some(target) = self.target_layers_from_node(&layer_res) {
            let n = target
                .layers
                .traverse_pre_order_ids(target.layers.root_node_id().unwrap())
                .unwrap()
                .find(|i| &target.layers.get(i).unwrap().data().resource == layer_res)
                .expect("Trying to remove unknown layer");
            target
                .layers
                .remove_node(n, DropChildren)
                .expect("Removal unsuccessful");
        }
    }

    /// Add a node to a graph, based on the resource data given. This is a NOP
    /// if the parent graph is a layer.
    pub fn add_node(&mut self, node: graph::NodeData) {
        let node_res = node.resource.clone();

        if let Some(target) = self.target_graph_from_node(&node_res) {
            target.add_node(node_res, node);
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

    /// Unset a material output of a layer. NOP for graphs.
    pub fn unset_output(&mut self, res: &Resource<Node>, channel: MaterialChannel) {
        if let Some(target) = self.target_layers_from_node(res) {
            target.unset_output(res, channel);
        }
    }

    /// Remove a node from a graph. This is a NOP for layers.
    pub fn remove_node(&mut self, node: &Resource<r::Node>) {
        if let Some(target) = self.target_graph_from_node(&node) {
            target.remove_node(node);
        }
    }

    /// Monomorphize a socket in a graph.
    pub fn monomorphize_socket(&mut self, socket: &Resource<r::Socket>, ty: ImageType) {
        let node = socket.socket_node();

        match self.target_collection_from_node(&node) {
            Some(NodeCollection::Graph(target)) => {
                let idx = target.resources.get(&node).unwrap();
                let node = target.graph.node_weight_mut(*idx).unwrap();
                let var = type_variable_from_socket_iter(
                    node.inputs.iter().chain(node.outputs.iter()),
                    socket.fragment().unwrap(),
                )
                .unwrap();
                node.set_type_variable(var, Some(ty))
            }
            Some(NodeCollection::Layers(target)) => {
                target.set_type_variable(socket, ty);
            }
            None => {}
        }
    }

    /// Demonomorphize a socket in a graph. This is a NOP for layers, since
    /// layers never have open input sockets.
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
            target.expose_parameter(param);
        }
    }

    /// Conceal a parameter from a collection, i.e. remove it from the list of exposed
    /// parameters to display.
    pub fn parameter_concealed(&mut self, graph: &Resource<r::Graph>, field: &str) {
        if let Some(target) = self.target_collection_from_collection(graph) {
            target.conceal_parameter(field);
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
