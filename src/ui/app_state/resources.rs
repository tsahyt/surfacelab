use crate::lang::resource as r;
use crate::ui::widgets::tree::Expandable;
use std::any::*;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ResourceCategory {
    Graph,
    Node,
    Socket,
    Image,
    Input,
    Output,
}

impl ResourceCategory {
    pub fn expandable(&self) -> bool {
        match self {
            ResourceCategory::Graph => true,
            ResourceCategory::Node => true,
            ResourceCategory::Socket => false,
            ResourceCategory::Image => true,
            ResourceCategory::Input => false,
            ResourceCategory::Output => false,
        }
    }
}

pub struct ResourceInfo {
    res: r::Resource<()>,
    res_str: String,
    res_ty: TypeId,
    category: ResourceCategory,
    expanded: bool,
}

impl ResourceInfo {
    pub fn new<T: 'static + r::Scheme>(
        resource: r::Resource<T>,
        category: ResourceCategory,
    ) -> Self {
        Self {
            res_ty: TypeId::of::<T>(),
            res_str: format!("{}", resource),
            res: resource.cast(),
            category,
            expanded: true,
        }
    }

    /// Get a typed resource. This call will succeed if and only if the type
    /// parameter T matches that of the resource used to build this info struct.
    pub fn get_resource<T: 'static>(&self) -> Option<r::Resource<T>> {
        if TypeId::of::<T>() == self.res_ty {
            Some(self.res.clone().cast())
        } else {
            None
        }
    }

    pub fn resource_string(&self) -> &str {
        &self.res_str
    }

    pub fn category(&self) -> ResourceCategory {
        self.category
    }

    pub fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn represents_resource<T: 'static + PartialEq>(&self, other: &r::Resource<T>) -> bool {
        if TypeId::of::<T>() == self.res_ty {
            &self.res.clone().cast() == other
        } else {
            false
        }
    }
}

pub enum ResourceTreeItem {
    ResourceInfo(ResourceInfo),
    Folder(String, bool),
}

impl ResourceTreeItem {
    pub fn resource_string(&self) -> &str {
        match self {
            ResourceTreeItem::ResourceInfo(i) => i.resource_string(),
            ResourceTreeItem::Folder(s, _) => &s,
        }
    }

    pub fn toggle_expanded(&mut self) {
        match self {
            ResourceTreeItem::ResourceInfo(i) => i.toggle_expanded(),
            ResourceTreeItem::Folder(_, b) => *b = !*b,
        }
    }

    pub fn represents_resource<T: 'static + PartialEq>(&self, other: &r::Resource<T>) -> bool {
        match self {
            ResourceTreeItem::ResourceInfo(i) => i.represents_resource(other),
            ResourceTreeItem::Folder(_, _) => false,
        }
    }

    pub fn category(&self) -> Option<ResourceCategory> {
        match self {
            ResourceTreeItem::ResourceInfo(i) => Some(i.category()),
            ResourceTreeItem::Folder(_, _) => None,
        }
    }
}

impl Expandable for ResourceTreeItem {
    fn expanded(&self) -> bool {
        match self {
            ResourceTreeItem::ResourceInfo(i) => i.expanded,
            ResourceTreeItem::Folder(_, e) => *e,
        }
    }
}

pub struct ResourceTree {
    tree: id_tree::Tree<ResourceTreeItem>,
    root: id_tree::NodeId,
    graphs: id_tree::NodeId,
    images: id_tree::NodeId,
}

impl Default for ResourceTree {
    fn default() -> Self {
        use id_tree::{InsertBehavior::*, *};

        let mut t = TreeBuilder::new()
            .with_root(Node::new(ResourceTreeItem::Folder(
                "This File".to_owned(),
                true,
            )))
            .build();

        let root = t.root_node_id().unwrap().clone();

        let graphs = t
            .insert(
                Node::new(ResourceTreeItem::Folder("Graphs".to_owned(), true)),
                UnderNode(&root),
            )
            .unwrap()
            .clone();
        let images = t
            .insert(
                Node::new(ResourceTreeItem::Folder("Images".to_owned(), true)),
                UnderNode(&root),
            )
            .unwrap()
            .clone();

        t.insert(
            Node::new(ResourceTreeItem::ResourceInfo(ResourceInfo::new(
                r::Resource::graph("base", None),
                ResourceCategory::Graph,
            ))),
            UnderNode(&graphs),
        )
        .unwrap();

        ResourceTree {
            tree: t,
            root,
            graphs,
            images,
        }
    }
}

impl ResourceTree {
    pub fn insert_graph(&mut self, graph: r::Resource<r::Graph>) {
        let rinfo = ResourceInfo::new(graph, ResourceCategory::Graph);
        self.tree
            .insert(
                id_tree::Node::new(ResourceTreeItem::ResourceInfo(rinfo)),
                id_tree::InsertBehavior::UnderNode(&self.graphs),
            )
            .unwrap();
    }

    fn find_resource<T: 'static + PartialEq>(
        &self,
        res: &r::Resource<T>,
    ) -> Option<id_tree::NodeId> {
        self.tree
            .traverse_level_order_ids(&self.root)
            .unwrap()
            .find(|n| self.tree.get(n).unwrap().data().represents_resource(res))
    }

    pub fn insert_node(&mut self, node: r::Resource<r::Node>) {
        let parent = node.node_graph();
        let rinfo = ResourceInfo::new(node, ResourceCategory::Node);

        if let Some(p) = self.find_resource(&parent) {
            self.tree
                .insert(
                    id_tree::Node::new(ResourceTreeItem::ResourceInfo(rinfo)),
                    id_tree::InsertBehavior::UnderNode(&p),
                )
                .unwrap();
        }
    }

    pub fn remove_resource_and_children<T: 'static + PartialEq>(&mut self, res: &r::Resource<T>) {
        if let Some(n) = self.find_resource(res) {
            self.tree
                .remove_node(n, id_tree::RemoveBehavior::DropChildren)
                .unwrap();
        }
    }

    pub fn rename_resource<T: 'static + PartialEq + r::Scheme>(
        &mut self,
        from: &r::Resource<T>,
        to: &r::Resource<T>,
    ) {
        if let Some(n) = self.find_resource(from) {
            let data = self.tree.get_mut(&n).unwrap().data_mut();
            let cat = data.category().unwrap();
            let rinfo = ResourceInfo::new(to.clone(), cat);
            *data = ResourceTreeItem::ResourceInfo(rinfo);
        }
    }

    pub fn get_tree(&self) -> &id_tree::Tree<ResourceTreeItem> {
        &self.tree
    }

    pub fn get_resource_info(&self, node: &id_tree::NodeId) -> &ResourceTreeItem {
        self.tree.get(node).unwrap().data()
    }

    pub fn get_resource_info_mut(&mut self, node: &id_tree::NodeId) -> &mut ResourceTreeItem {
        self.tree.get_mut(node).unwrap().data_mut()
    }

    pub fn expandable(&self, node: &id_tree::NodeId) -> bool {
        let can_expand = match self.tree.get(node).unwrap().data() {
            ResourceTreeItem::ResourceInfo(i) => i.category.expandable(),
            _ => true,
        };
        let has_children = self.tree.children(node).unwrap().next().is_some();

        can_expand && has_children
    }
}