use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    app_state::{LocationStatus, NodeCollections, ResourceCategory, ResourceTree},
    i18n::Language,
    util::IconName,
    widgets::{resource_row, toolbar, tree},
};

use std::sync::Arc;

use conrod_core::*;

use dialog::{DialogBox, FileSelection, FileSelectionMode};

#[derive(WidgetCommon)]
pub struct ResourceBrowser<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut NodeCollections,
    event_buffer: Option<&'a [Arc<Lang>]>,
    style: Style,
}

impl<'a> ResourceBrowser<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        graphs: &'a mut NodeCollections,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            graphs,
            event_buffer: None,
            style: Style::default(),
        }
    }

    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub selected_color { style.selected_color = Some(Color) }
        pub event_buffer { event_buffer = Some(&'a [Arc<Lang>]) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
    #[conrod(default = "color::Color::Rgba(0.9, 0.4, 0.15, 1.0)")]
    selected_color: Option<Color>,
}

widget_ids! {
    pub struct Ids {
        main_toolbar,
        tree
    }
}

pub struct State {
    ids: Ids,
    tree: ResourceTree,
}

#[derive(Clone, Copy)]
pub enum CollectionTool {
    NewGraph,
    NewStack,
    NewImage,
    NewSvg,
}

impl<'a> Widget for ResourceBrowser<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        let mut tree = ResourceTree::default();
        tree.insert_graph(Resource::graph("base"));

        State {
            ids: Ids::new(id_gen),
            tree,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            ..
        } = args;

        if let Some(ev_buf) = self.event_buffer {
            for ev in ev_buf {
                self.handle_event(state, ev);
            }
        }

        match toolbar::Toolbar::flow_right(
            [
                (IconName::GRAPH, CollectionTool::NewGraph),
                (IconName::LAYERS, CollectionTool::NewStack),
                (IconName::IMAGE, CollectionTool::NewImage),
                (IconName::SVG, CollectionTool::NewSvg),
            ]
            .iter()
            .copied(),
        )
        .icon_font(style.icon_font(&ui.theme))
        .icon_color(color::WHITE)
        .button_color(color::DARK_CHARCOAL)
        .parent(args.id)
        .h(32.0)
        .top_left_with_margins(8.0, 0.0)
        .set(state.ids.main_toolbar, ui)
        {
            Some(CollectionTool::NewGraph) => self
                .sender
                .send(Lang::UserGraphEvent(UserGraphEvent::AddGraph))
                .unwrap(),
            Some(CollectionTool::NewStack) => self
                .sender
                .send(Lang::UserLayersEvent(UserLayersEvent::AddLayers))
                .unwrap(),
            Some(CollectionTool::NewImage) => {
                match FileSelection::new(self.language.get_message("image-select"))
                    .title(self.language.get_message("image-select-title"))
                    .mode(FileSelectionMode::Open)
                    .show()
                {
                    Ok(Some(path)) => self
                        .sender
                        .send(Lang::UserIOEvent(UserIOEvent::AddImageResource(
                            std::path::PathBuf::from(path),
                        )))
                        .unwrap(),
                    Err(e) => log::error!("Error during file selection {}", e),
                    _ => {}
                }
            }
            Some(CollectionTool::NewSvg) => {
                match FileSelection::new(self.language.get_message("svg-select"))
                    .title(self.language.get_message("svg-select-title"))
                    .mode(FileSelectionMode::Open)
                    .show()
                {
                    Ok(Some(path)) => self
                        .sender
                        .send(Lang::UserIOEvent(UserIOEvent::AddSvgResource(
                            std::path::PathBuf::from(path),
                        )))
                        .unwrap(),
                    Err(e) => log::error!("Error during file selection {}", e),
                    _ => {}
                }
            }
            _ => {}
        }

        let selected_color = style.selected_color(&ui.theme);

        let (mut rows, scrollbar) = tree::Tree::new(state.tree.get_tree())
            .parent(id)
            .mid_top_with_margin(40.0)
            .padded_w_of(id, 8.0)
            .h(ui.h_of(id).unwrap() - 48.0)
            .scrollbar_on_top()
            .set(state.ids.tree, ui);

        while let Some(row) = rows.next(ui) {
            let expandable = state.tree.expandable(&row.node_id);
            let data = state.tree.get_resource_info(&row.node_id);

            let active = self
                .graphs
                .get_active_element()
                .map(|r| data.represents_resource(r))
                .unwrap_or(false);

            let widget = resource_row::ResourceRow::new(&data, row.level)
                .expandable(expandable)
                .active(active)
                .icon_font(style.icon_font(&ui.theme))
                .icon_size(14)
                .text_size(10)
                .selected_color(selected_color)
                .color(color::WHITE)
                .h(32.0);

            match row.item.set(widget, ui) {
                None => {}
                Some(resource_row::Event::ToggleExpanded) => {
                    state.update(|state| {
                        state
                            .tree
                            .get_resource_info_mut(&row.node_id)
                            .toggle_expanded();
                    });
                }
                Some(resource_row::Event::Clicked) => {
                    if let Some(collection) = data.get_resource() {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
                                collection.clone(),
                            )))
                            .unwrap();
                        self.graphs.set_active_collection(collection);
                    }

                    if let Some(node) = data.get_resource() {
                        self.graphs.set_active_element(node);
                    }
                }
                Some(resource_row::Event::DeleteRequested) => {
                    if let Some(collection) = data.get_resource() {
                        match data.category() {
                            Some(ResourceCategory::Graph) => {
                                self.sender
                                    .send(Lang::UserGraphEvent(UserGraphEvent::DeleteGraph(
                                        collection.clone(),
                                    )))
                                    .unwrap();
                            }
                            Some(ResourceCategory::Stack) => {
                                self.sender
                                    .send(Lang::UserLayersEvent(UserLayersEvent::DeleteLayers(
                                        collection.clone(),
                                    )))
                                    .unwrap();
                            }
                            _ => {}
                        }
                    }

                    if let Some(node) = data.get_resource() {
                        match data.category() {
                            Some(ResourceCategory::Node) => {
                                self.sender
                                    .send(Lang::UserNodeEvent(UserNodeEvent::RemoveNode(
                                        node.clone(),
                                    )))
                                    .unwrap();
                            }
                            Some(ResourceCategory::Layer) => {
                                self.sender
                                    .send(Lang::UserLayersEvent(UserLayersEvent::RemoveLayer(
                                        node.clone(),
                                    )))
                                    .unwrap();
                            }
                            _ => {}
                        }
                    }

                    if let Some(img) = data.get_resource::<Img>() {
                        if let Some(ResourceCategory::Image) = data.category() {
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::RemoveImageResource(
                                    img.clone(),
                                )))
                                .unwrap();
                        }
                    }

                    if let Some(svg) = data.get_resource::<resource::Svg>() {
                        if let Some(ResourceCategory::Svg) = data.category() {
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::RemoveSvgResource(
                                    svg.clone(),
                                )))
                                .unwrap();
                        }
                    }
                }
                Some(resource_row::Event::PackRequested) => {
                    if let Some(image) = data.get_resource() {
                        if let Some(ResourceCategory::Image) = data.category() {
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::PackImage(image.clone())))
                                .unwrap()
                        }
                    }

                    if let Some(svg) = data.get_resource() {
                        if let Some(ResourceCategory::Svg) = data.category() {
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::PackSvg(svg.clone())))
                                .unwrap()
                        }
                    }
                }
                Some(resource_row::Event::ReloadRequested) => {
                    if let Some(image) = data.get_resource() {
                        if let Some(ResourceCategory::Image) = data.category() {
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::ReloadImageResource(
                                    image.clone(),
                                )))
                                .unwrap()
                        }
                    }

                    if let Some(svg) = data.get_resource() {
                        if let Some(ResourceCategory::Svg) = data.category() {
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::ReloadSvgResource(
                                    svg.clone(),
                                )))
                                .unwrap()
                        }
                    }
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }
}

impl<'a> ResourceBrowser<'a> {
    fn handle_event(&self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::GraphEvent(GraphEvent::GraphAdded(res)) => {
                state.update(|state| state.tree.insert_graph(res.clone()));
            }
            Lang::GraphEvent(GraphEvent::GraphRemoved(res)) => {
                state.update(|state| state.tree.remove_resource_and_children(res));
            }
            Lang::GraphEvent(GraphEvent::GraphRenamed(from, to)) => {
                state.update(|state| state.tree.rename_resource(from, to));
            }
            Lang::GraphEvent(GraphEvent::NodeAdded(res, _, _, _, _)) => {
                state.update(|state| state.tree.insert_node(res.clone()));
            }
            Lang::GraphEvent(GraphEvent::NodeRemoved(res, _, _)) => {
                state.update(|state| state.tree.remove_resource_and_children(res));
            }
            Lang::GraphEvent(GraphEvent::NodeRenamed(from, to)) => {
                state.update(|state| state.tree.rename_resource(from, to));
            }
            Lang::GraphEvent(GraphEvent::Cleared) => {
                state.update(|state| state.tree.clear_graphs());
            }
            Lang::ComputeEvent(ComputeEvent::Cleared) => {
                state.update(|state| {
                    state.tree.clear_images();
                    state.tree.clear_svgs();
                });
            }
            Lang::LayersEvent(LayersEvent::LayersAdded(res, _, _)) => {
                state.update(|state| state.tree.insert_stack(res.clone()));
            }
            Lang::LayersEvent(LayersEvent::LayersRemoved(res)) => {
                state.update(|state| state.tree.remove_resource_and_children(res));
            }
            Lang::LayersEvent(LayersEvent::LayerPushed(res, _, _, _, _, _, _, _)) => {
                state.update(|state| state.tree.insert_layer(res.clone()));
            }
            Lang::LayersEvent(LayersEvent::LayerRemoved(res)) => {
                state.update(|state| state.tree.remove_resource_and_children(res));
            }
            Lang::ComputeEvent(ComputeEvent::ImageResourceAdded(res, _, packed)) => {
                state.update(|state| state.tree.insert_image(res.clone(), *packed));
            }
            Lang::ComputeEvent(ComputeEvent::ImagePacked(res)) => state.update(|state| {
                state
                    .tree
                    .set_location_status(res, Some(LocationStatus::Packed))
            }),
            Lang::ComputeEvent(ComputeEvent::ImageResourceRemoved(res, _)) => {
                state.update(|state| state.tree.remove_resource_and_children(res));
            }
            Lang::ComputeEvent(ComputeEvent::SvgResourceAdded(res, packed)) => {
                state.update(|state| state.tree.insert_svg(res.clone(), *packed));
            }
            Lang::ComputeEvent(ComputeEvent::SvgResourceRemoved(res, _)) => {
                state.update(|state| state.tree.remove_resource_and_children(res));
            }
            Lang::ComputeEvent(ComputeEvent::SvgPacked(res)) => state.update(|state| {
                state
                    .tree
                    .set_location_status(res, Some(LocationStatus::Packed))
            }),
            _ => {}
        }
    }
}
