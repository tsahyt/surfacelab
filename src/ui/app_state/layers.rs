use std::collections::HashMap;

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
    pub type_variables: HashMap<TypeVariable, ImageType>,
}

impl Layer {
    pub fn root_layer() -> Self {
        Self {
            resource: Resource::node("__root__"),
            title: "root".to_owned(),
            icon: crate::ui::util::IconName::SOLID,
            thumbnail: None,
            operator_pbox: ParamBoxDescription::empty(),
            opacity: 0.,
            blend_mode: 0,
            enabled: false,
            is_mask: false,
            expanded: true,
            type_variables: HashMap::new(),
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
            type_variables: HashMap::new(),
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
            type_variables: HashMap::new(),
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

impl crate::ui::widgets::tree::Expandable for Layer {
    fn expanded(&self) -> bool {
        self.expanded
    }
}

#[derive(Debug, Clone)]
pub struct Layers {
    pub layers: Tree<Layer>,
    exposed_parameters: Vec<(String, GraphParameter)>,
    param_box: ParamBoxDescription<GraphField>,
    pub active_element: Option<id_tree::NodeId>,
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
            active_element: None,
        }
    }

    pub fn is_base_layer(&self, node: &id_tree::NodeId) -> bool {
        let root = self.layers.root_node_id().unwrap().clone();
        self.layers.traverse_level_order_ids(&root).unwrap().nth(1) == Some(node.clone())
    }

    pub fn expandable(&self, node: &id_tree::NodeId) -> bool {
        self.layers.children(node).expect("Invalid node").count() > 0
    }

    pub fn unset_output(
        &mut self,
        layer: &Resource<r::Node>,
        channel: MaterialChannel,
    ) -> Option<()> {
        let node_id = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == layer)?;
        let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
        let chans = layer
            .operator_pbox
            .categories
            .iter_mut()
            .find(|c| c.name == "output-channels")?;

        for param in chans.parameters.iter_mut() {
            match &mut param.control {
                Control::ChannelMap { enabled, chan, .. } => {
                    if *chan == channel {
                        *enabled = false;
                    }
                }
                _ => {}
            }
        }

        Some(())
    }

    /// Set type (variable) for a socket. May return None if no such socket can
    /// be found, or the parameter box does not have a channel map, e.g. for Masks
    pub fn set_type_variable(&mut self, socket: &Resource<r::Socket>, ty: ImageType) -> Option<()> {
        let layer = socket.socket_node();
        let socket_name = socket.fragment().unwrap();

        let node_id = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == &layer)?;
        let layer = self.layers.get_mut(&node_id).unwrap().data_mut();

        let chans = layer
            .operator_pbox
            .categories
            .iter_mut()
            .find(|c| c.name == "output-channels")?;

        for param in chans.parameters.iter_mut() {
            match &mut param.control {
                Control::ChannelMap { sockets, .. } => {
                    for s in sockets.iter_mut().filter(|s| s.0 == socket_name) {
                        s.1 = OperatorType::Monomorphic(ty);
                    }
                }
                _ => {}
            }
        }

        Some(())
    }

    /// Update the opacity when set from outside of the UI
    pub fn update_opacity(&mut self, layer: &Resource<r::Node>, opacity: f32) {
        if let Some(node_id) = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == layer)
        {
            let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
            layer.opacity = opacity;
        }
    }

    /// Update the blend mode when set from outside of the UI
    pub fn update_blend_mode(&mut self, layer: &Resource<r::Node>, blend_mode: BlendMode) {
        if let Some(node_id) = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == layer)
        {
            let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
            layer.blend_mode = blend_mode as usize;
        }
    }

    /// Update the enabled toggle when set from outside of the UI
    pub fn update_enabled(&mut self, layer: &Resource<r::Node>, enabled: bool) {
        if let Some(node_id) = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == layer)
        {
            let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
            layer.enabled = enabled;
        }
    }

    /// Update the title when set from outside of the UI
    pub fn update_title(&mut self, layer: &Resource<r::Node>, title: &str) {
        if let Some(node_id) = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == layer)
        {
            let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
            layer.title = title.to_string();
        }
    }

    /// Return an iterator of valid drop positions as currently visible in the
    /// tree. The iterator is guaranteed to be sorted.
    pub fn drag_limits(&self, res: &Resource<r::Node>) -> impl Iterator<Item = usize> {
        use std::collections::VecDeque;

        let tree = &self.layers;

        let mut stack: Vec<(id_tree::NodeId, usize)> = Vec::with_capacity(tree.height());
        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut pos = 0;

        if let Some(root) = tree.root_node_id() {
            let parent = tree
                .traverse_pre_order(root)
                .unwrap()
                .find_map(|n| {
                    if &n.data().resource == res {
                        Some(n.parent().unwrap())
                    } else {
                        None
                    }
                })
                .unwrap()
                .clone();
            stack.push((root.clone(), 0));

            while !stack.is_empty() {
                let (current, level) = stack.pop().unwrap();
                if tree
                    .get(&current)
                    .unwrap()
                    .parent()
                    .map(|p| p == &parent)
                    .unwrap_or(false)
                    || current == parent
                {
                    queue.push_back(pos);
                }
                if tree
                    .get(&current)
                    .expect("Invalid node ID in tree")
                    .data()
                    .expanded
                {
                    stack.extend(
                        tree.children_ids(&current)
                            .unwrap()
                            .cloned()
                            .map(|n| (n, level + 1)),
                    );
                }

                pos += 1;
            }
        }

        queue.into_iter()
    }

    /// Return a LayerDropTarget describing the given resource in a given
    /// desired target position relative to the current stack.
    pub fn drag_target(&self, res: &Resource<r::Node>, target: usize) -> LayerDropTarget {
        let canonical = super::super::widgets::tree::visible_tree_items_queue(&self.layers, true);

        if target == 0 {
            LayerDropTarget::Above(
                self.layers
                    .get(&canonical[target].0)
                    .unwrap()
                    .data()
                    .resource
                    .clone(),
            )
        } else {
            let target_res = &self
                .layers
                .get(&canonical[target - 1].0)
                .unwrap()
                .data()
                .resource;

            if res.path_str().unwrap().contains("mask")
                && !target_res.path_str().unwrap().contains("mask")
            {
                LayerDropTarget::Above(
                    self.layers
                        .get(&canonical[target + 1].0)
                        .unwrap()
                        .data()
                        .resource
                        .clone(),
                )
            } else {
                LayerDropTarget::Below(target_res.clone())
            }
        }
    }
}

impl Collection for Layers {
    fn rename_collection(&mut self, to: &Resource<r::Graph>) {
        self.param_box
            .update_parameter_by_transmitter(GraphField::Name, &to.file().unwrap().to_data());
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

    fn expose_parameter(&mut self, param: GraphParameter) {
        let node = param.parameter.parameter_node();
        let node_id = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == &node)
            .expect("Unknown layer");
        let pbox = &mut self
            .layers
            .get_mut(&node_id)
            .unwrap()
            .data_mut()
            .operator_pbox;
        pbox.set_expose_status(
            param.parameter.fragment().unwrap(),
            Some(ExposeStatus::Exposed),
        );

        self.exposed_parameters
            .push((param.graph_field.clone(), param));
    }

    fn conceal_parameter(&mut self, field: &str) {
        if let Some(idx) = self.exposed_parameters.iter().position(|x| x.0 == field) {
            let (_, param) = self.exposed_parameters.remove(idx);
            let node = param.parameter.parameter_node();
            let node_id = self
                .layers
                .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
                .unwrap()
                .find(|i| &self.layers.get(i).unwrap().data().resource == &node)
                .expect("Unknown layer");
            let pbox = &mut self
                .layers
                .get_mut(&node_id)
                .unwrap()
                .data_mut()
                .operator_pbox;
            pbox.set_expose_status(
                param.parameter.fragment().unwrap(),
                Some(ExposeStatus::Unexposed),
            );
        }
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

    fn active_element(
        &mut self,
    ) -> Option<(
        &Resource<r::Node>,
        &mut ParamBoxDescription<MessageWriters>,
        &HashMap<TypeVariable, ImageType>,
    )> {
        let idx = self.active_element.as_ref()?;
        let layer = self.layers.get_mut(&idx).ok()?;
        let data = layer.data_mut();
        Some((
            &data.resource,
            &mut data.operator_pbox,
            &data.type_variables,
        ))
    }

    fn active_resource(&self) -> Option<&Resource<r::Node>> {
        let idx = self.active_element.as_ref()?;
        let layer = self.layers.get(&idx).ok()?;
        let data = layer.data();
        Some(&data.resource)
    }

    fn set_active(&mut self, element: &Resource<r::Node>) {
        if let Some(root) = self.layers.root_node_id() {
            self.active_element = self
                .layers
                .traverse_pre_order_ids(root)
                .unwrap()
                .find(|i| &self.layers.get(i).unwrap().data().resource == element);
        }
    }

    fn update_parameter(&mut self, param: &Resource<r::Param>, value: &[u8]) {
        let layer_res = param.parameter_node();
        if let Some(node_id) = self
            .layers
            .traverse_pre_order_ids(self.layers.root_node_id().unwrap())
            .unwrap()
            .find(|i| &self.layers.get(i).unwrap().data().resource == &layer_res)
        {
            let layer = self.layers.get_mut(&node_id).unwrap().data_mut();
            layer.operator_pbox.update_parameter(param, value);
        }
    }
}
