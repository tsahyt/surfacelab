use crate::lang::resource as r;
use crate::lang::*;

use conrod_core::image;
use id_tree::Tree;

use super::collection::Collection;

#[derive(Debug, Clone)]
pub struct Layer {
    pub resource: Resource<Node>,
    pub title: String,
    pub icon: crate::ui::util::IconName,
    pub thumbnail: Option<image::Id>,
    pub operator_pbox: ParamBoxDescription<MessageWriters>,
    pub opacity: f32,
    pub blend_mode: usize,
    pub enabled: bool,
    pub is_mask: bool,
    pub expanded: bool,
}

impl Layer {
    pub fn root_layer() -> Self {
        Self {
            resource: Resource::node("__root__", None),
            title: "root".to_owned(),
            icon: crate::ui::util::IconName::SOLID,
            thumbnail: None,
            operator_pbox: ParamBoxDescription::empty(),
            opacity: 0.,
            blend_mode: 0,
            enabled: false,
            is_mask: false,
            expanded: true,
        }
    }

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
                LayerType::Fill => crate::ui::util::IconName::SOLID,
                LayerType::Fx => crate::ui::util::IconName::FX,
            },
            thumbnail: None,
            operator_pbox: pbox,
            opacity,
            blend_mode,
            enabled: true,
            is_mask: false,
            expanded: true,
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
            icon: crate::ui::util::IconName::MASK,
            thumbnail: None,
            operator_pbox: pbox,
            opacity,
            blend_mode,
            enabled: true,
            is_mask: true,
            expanded: true,
        }
    }

    pub fn update(&mut self, param_box: ParamBoxDescription<MessageWriters>) {
        self.operator_pbox = param_box;
    }

    pub fn toggle_expanded(&mut self) {
        if !self.is_mask {
            self.expanded = !self.expanded;
        }
    }
}

impl crate::ui::tree::Expandable for Layer {
    fn expanded(&self) -> bool {
        self.expanded
    }
}

#[derive(Debug, Clone)]
pub struct Layers {
    pub layers: Tree<Layer>,
    exposed_parameters: Vec<(String, GraphParameter)>,
    param_box: ParamBoxDescription<GraphField>,
}

impl Layers {
    pub fn new(name: &str) -> Self {
        use id_tree::*;
        Self {
            layers: TreeBuilder::new()
                .with_root(Node::new(Layer::root_layer()))
                .build(),
            exposed_parameters: Vec::new(),
            param_box: ParamBoxDescription::graph_parameters(name),
        }
    }

    pub fn is_base_layer(&self, node: &id_tree::NodeId) -> bool {
        let root = self.layers.root_node_id().unwrap().clone();
        self.layers.traverse_level_order_ids(&root).unwrap().nth(1) == Some(node.clone())
    }

    pub fn expandable(&self, node: &id_tree::NodeId) -> bool {
        self.layers.children(node).expect("Invalid node").count() > 0
    }

    pub fn move_up(&mut self, layer: &Resource<r::Node>) {
        use id_tree::SwapBehavior::*;
        let node_id = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == layer)
            .expect("Unknown layer");
        let parent = self
            .layers
            .get(&node_id)
            .unwrap()
            .parent()
            .expect("Trying to move node without parent");

        let traversal: Vec<_> = self
            .layers
            .traverse_level_order_ids(&parent)
            .unwrap()
            .skip(1)
            .collect();
        if let Some([_, next]) = traversal[..].windows(2).find(|xs| match xs {
            [c, _] => c == &node_id,
            _ => false,
        }) {
            self.layers
                .swap_nodes(&node_id, next, TakeChildren)
                .expect("Failed to swap nodes");
        }
    }

    pub fn move_down(&mut self, layer: &Resource<r::Node>) {
        use id_tree::SwapBehavior::*;
        let node_id = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == layer)
            .expect("Unknown layer");
        let parent = self
            .layers
            .get(&node_id)
            .unwrap()
            .parent()
            .expect("Trying to move node without parent");

        let traversal: Vec<_> = self
            .layers
            .traverse_level_order_ids(&parent)
            .unwrap()
            .skip(1)
            .collect();
        if let Some([previous, _]) = traversal[..].windows(2).find(|xs| match xs {
            [_, c] => c == &node_id,
            _ => false,
        }) {
            self.layers
                .swap_nodes(&node_id, previous, TakeChildren)
                .expect("Failed to swap nodes");
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
        if let Some(root) = self.layers.root_node_id() {
            if let Some(node_id) = self
                .layers
                .traverse_pre_order_ids(root)
                .unwrap()
                .find(|i| &self.layers.get(i).unwrap().data().resource == node)
            {
                let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
                layer.thumbnail = Some(thumbnail);
            }
        }
    }

    fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        let mut old_thumbnail = None;

        if let Some(root) = self.layers.root_node_id() {
            if let Some(node_id) = self
                .layers
                .traverse_pre_order_ids(root)
                .unwrap()
                .find(|i| &self.layers.get(i).unwrap().data().resource == node)
            {
                let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
                old_thumbnail = layer.thumbnail;
                layer.thumbnail = None
            }
        }

        old_thumbnail
    }

    fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        _op: &ComplexOperator,
        pbox: &ParamBoxDescription<MessageWriters>,
    ) {
        if let Some(root) = self.layers.root_node_id() {
            if let Some(node_id) = self
                .layers
                .traverse_pre_order_ids(root)
                .unwrap()
                .find(|i| &self.layers.get(i).unwrap().data().resource == node)
            {
                let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
                layer.update(pbox.clone());
            }
        }
    }
}
