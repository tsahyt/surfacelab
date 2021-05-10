use crate::lang::*;

use conrod_core::image;

pub mod collection;
pub mod graph;
pub mod layers;
pub mod resources;
pub mod undo;

pub use collection::*;
pub use graph::*;
pub use layers::*;
pub use resources::*;
pub use undo::*;

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
