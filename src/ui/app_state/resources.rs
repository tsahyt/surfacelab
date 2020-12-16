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

pub struct ResourceInfo {
    res: r::Resource<()>,
    res_str: String,
    res_ty: TypeId,
    category: ResourceCategory,
    expanded: bool,
}

impl ResourceInfo {
    pub fn new<T: 'static + r::Scheme>(resource: r::Resource<T>, category: ResourceCategory) -> Self {
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
}

impl Expandable for ResourceInfo {
    fn expanded(&self) -> bool {
        self.expanded
    }
}
