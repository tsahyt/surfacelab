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
}

impl Expandable for ResourceTreeItem {
    fn expanded(&self) -> bool {
        match self {
            ResourceTreeItem::ResourceInfo(i) => i.expanded,
            ResourceTreeItem::Folder(_, e) => *e,
        }
    }
}

pub struct ResourceTree(id_tree::Tree<ResourceTreeItem>);

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

        t.insert(
            Node::new(ResourceTreeItem::Folder("Graphs".to_owned(), true)),
            UnderNode(&root),
        )
        .unwrap();
        t.insert(
            Node::new(ResourceTreeItem::Folder("Images".to_owned(), true)),
            UnderNode(&root),
        )
        .unwrap();

        ResourceTree(t)
    }
}

impl ResourceTree {
    pub fn get_tree(&self) -> &id_tree::Tree<ResourceTreeItem> {
        &self.0
    }

    pub fn get_resource_info(&self, node: &id_tree::NodeId) -> &ResourceTreeItem {
        self.0.get(node).unwrap().data()
    }

    pub fn get_resource_info_mut(&mut self, node: &id_tree::NodeId) -> &mut ResourceTreeItem {
        self.0.get_mut(node).unwrap().data_mut()
    }

    pub fn expandable(&self, node: &id_tree::NodeId) -> bool {
        let can_expand = match self.0.get(node).unwrap().data() {
            ResourceTreeItem::ResourceInfo(i) => i.category.expandable(),
            _ => true,
        };
        let has_children = self.0.children(node).unwrap().next().is_some();

        can_expand && has_children
    }
}
