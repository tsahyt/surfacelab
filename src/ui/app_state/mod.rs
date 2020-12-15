use crate::lang::*;

use conrod_core::{image, text, Point};

pub mod collection;
pub mod graph;
pub mod layers;

pub use collection::*;
pub use graph::*;
pub use layers::*;

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
    pub graphs: NodeCollections,
    pub active_node_element: Option<petgraph::graph::NodeIndex>,
    pub active_layer_element: Option<id_tree::NodeId>,

    pub render_image: RenderImage,
    pub monitor_resolution: (u32, u32),

    pub add_node_modal: Option<Point>,
    pub add_layer_modal: Option<LayerFilter>,
    pub render_modal: bool,

    pub render_params: ParamBoxDescription<RenderField>,
    pub surface_params: ParamBoxDescription<SurfaceField>,

    pub registered_operators: Vec<Operator>,
    pub addable_operators: Vec<Operator>,
    pub registered_sockets: Vec<crate::ui::widgets::export_row::RegisteredSocket>,
    pub export_entries: Vec<(String, ExportSpec)>,
}

impl App {
    pub fn new(monitor_size: (u32, u32)) -> Self {
        Self {
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
