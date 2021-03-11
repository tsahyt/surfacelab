use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{components, i18n::Language, widgets::tabs};
use std::sync::Arc;

use crate::ui::app_state::*;

use conrod_core::*;

pub struct ApplicationData<B: crate::gpu::Backend> {
    sender: BrokerSender<Lang>,
    image_map: image::Map<crate::gpu::ui::Image<B>>,
    language: Language,
    monitor_resolution: (u32, u32),
}

impl<B> ApplicationData<B>
where
    B: crate::gpu::Backend,
{
    /// Create a new ApplicationData instance
    pub fn new(
        sender: BrokerSender<Lang>,
        image_map: image::Map<crate::gpu::ui::Image<B>>,
        monitor_resolution: (u32, u32),
    ) -> Self {
        Self {
            sender,
            image_map,
            language: Language::default(),
            monitor_resolution,
        }
    }

    /// Obtain a reference to the image map
    pub fn image_map(&self) -> &image::Map<crate::gpu::ui::Image<B>> {
        &self.image_map
    }
}

#[derive(WidgetCommon)]
pub struct Application<'a, B: crate::gpu::Backend> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    app_data: &'a mut ApplicationData<B>,
    event_buffer: Option<&'a [Arc<Lang>]>,
    renderer: &'a mut crate::gpu::ui::Renderer<B>,
    style: Style,
}

impl<'a, B> Application<'a, B>
where
    B: crate::gpu::Backend,
{
    pub fn new(
        app_data: &'a mut ApplicationData<B>,
        renderer: &'a mut crate::gpu::ui::Renderer<B>,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            app_data,
            event_buffer: None,
            renderer,
            style: Style::default(),
        }
    }

    pub fn event_buffer(mut self, buffer: &'a [Arc<Lang>]) -> Self {
        self.event_buffer = Some(buffer);
        self
    }

    /// A method for specifying the `Font` used for displaying text.
    pub fn icon_font(mut self, font_id: text::font::Id) -> Self {
        self.style.icon_font = Some(Some(font_id));
        self
    }

    /// A method for specifying the `Font` used for displaying text.
    pub fn text_font(mut self, font_id: text::font::Id) -> Self {
        self.style.text_font = Some(Some(font_id));
        self
    }

    builder_method!(pub panel_color { style.panel_color = Some(Color) });
    builder_method!(pub panel_gap { style.panel_gap = Some(Scalar) });
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id")]
    pub icon_font: Option<Option<text::font::Id>>,
    #[conrod(default = "theme.font_id")]
    pub text_font: Option<Option<text::font::Id>>,
    #[conrod(default = "theme.background_color")]
    pub panel_color: Option<color::Color>,
    #[conrod(default = "theme.border_width")]
    pub panel_gap: Option<Scalar>,
}

widget_ids! {
    pub struct Ids {
        // Main Areas
        window_canvas,
        top_bar_canvas,
        main_canvas,
        main_inner_canvas,
        edit_canvas,
        drawing_canvas,
        sidebar_canvas,
        settings_canvas,
        resources_canvas,
        parameter_canvas,
        graph_settings_canvas,
        surface_settings_canvas,

        // Components
        top_bar,
        node_editor,
        layer_editor,
        viewport,
        parameter_section,
        graph_section,
        surface_section,
        resource_browser,

        // Sidebar
        sidebar_tabs,
    }
}

pub struct State {
    ids: Ids,
    graphs: NodeCollections,
    image_resources: Vec<Resource<Img>>,
}

impl<'a, B> Widget for Application<'a, B>
where
    B: crate::gpu::Backend,
{
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            graphs: NodeCollections::new(),
            image_resources: Vec::new(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(mut self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { state, ui, .. } = args;

        if let Some(ev_buf) = self.event_buffer {
            for ev in ev_buf {
                self.handle_event(state, ev);
            }
        }

        let edit_width = match state.graphs.get_active_collection() {
            NodeCollection::Graph(_) => None,
            NodeCollection::Layers(_) => Some(384.0),
        };

        // Main canvasses
        widget::Canvas::new()
            .border(0.0)
            .color(self.style.panel_color.unwrap())
            .flow_right(&[
                (
                    state.ids.main_canvas,
                    widget::Canvas::new()
                        .border(self.style.panel_gap.unwrap())
                        .color(self.style.panel_color.unwrap())
                        .flow_down(&[
                            (
                                state.ids.top_bar_canvas,
                                widget::Canvas::new()
                                    .color(self.style.panel_color.unwrap())
                                    .border(self.style.panel_gap.unwrap())
                                    .length(48.0),
                            ),
                            (
                                state.ids.main_inner_canvas,
                                widget::Canvas::new()
                                    .color(self.style.panel_color.unwrap())
                                    .border(self.style.panel_gap.unwrap())
                                    .flow_right(&[
                                        (state.ids.edit_canvas, {
                                            let mut w = widget::Canvas::new()
                                                .color(self.style.panel_color.unwrap())
                                                .border(self.style.panel_gap.unwrap());
                                            if let Some(x) = edit_width {
                                                w = w.length(x);
                                            }
                                            w
                                        }),
                                        (
                                            state.ids.drawing_canvas,
                                            widget::Canvas::new()
                                                .color(self.style.panel_color.unwrap())
                                                .border(self.style.panel_gap.unwrap()),
                                        ),
                                    ]),
                            ),
                        ]),
                ),
                (
                    state.ids.sidebar_canvas,
                    widget::Canvas::new()
                        .border(self.style.panel_gap.unwrap())
                        .color(self.style.panel_color.unwrap())
                        .length(384.0)
                        .flow_down(&[
                            (
                                state.ids.settings_canvas,
                                widget::Canvas::new()
                                    .border(self.style.panel_gap.unwrap())
                                    .color(self.style.panel_color.unwrap())
                                    .length_weight(0.66),
                            ),
                            (
                                state.ids.resources_canvas,
                                widget::Canvas::new()
                                    .border(self.style.panel_gap.unwrap())
                                    .color(self.style.panel_color.unwrap())
                                    .length_weight(0.33),
                            ),
                        ]),
                ),
            ])
            .set(state.ids.window_canvas, ui);

        // Side tabs
        tabs::Tabs::new(&[
            (
                state.ids.parameter_canvas,
                &self.app_data.language.get_message("parameters-tab"),
            ),
            (
                state.ids.graph_settings_canvas,
                &self.app_data.language.get_message("graph-tab"),
            ),
            (
                state.ids.surface_settings_canvas,
                &self.app_data.language.get_message("surface-tab"),
            ),
        ])
        .color(self.style.panel_color.unwrap())
        .label_color(color::WHITE)
        .label_font_size(10)
        .bar_thickness(48.0)
        .border(self.style.panel_gap.unwrap())
        .parent(state.ids.settings_canvas)
        .wh_of(state.ids.settings_canvas)
        .middle()
        .set(state.ids.sidebar_tabs, ui);

        // Call update functions for each part of the UI
        self.update_top_bar(state, ui);
        match state.graphs.get_active_collection() {
            NodeCollection::Graph(_) => self.update_node_graph(state, ui),
            NodeCollection::Layers(_) => self.update_layer_stack(state, ui),
        };
        self.update_viewport(state, ui);
        self.update_parameter_section(state, ui);
        self.update_graph_section(state, ui);
        self.update_surface_section(state, ui);
        self.update_resource_browser(state, ui);
    }
}

impl<'a, B> Application<'a, B>
where
    B: crate::gpu::Backend,
{
    /// Handle UI event
    fn handle_event(&mut self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::ComputeEvent(ComputeEvent::ThumbnailCreated(res, thmb)) => {
                if let Some(t) = thmb.clone().to::<B>() {
                    if let Some(img) = self.renderer.create_image(t, 128, 128) {
                        let id = self.app_data.image_map.insert(img);
                        state.update(|state| state.graphs.register_thumbnail(&res, id));
                    }
                }
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailDestroyed(res)) => {
                state.update(|state| {
                    if let Some(id) = state.graphs.unregister_thumbnail(&res) {
                        self.app_data.image_map.remove(id);
                    }
                });
            }
            Lang::ComputeEvent(ComputeEvent::ImageResourceAdded(res)) => {
                dbg!("updating");
                state.update(|state| {
                    state.image_resources.push(res.clone());
                });
            }
            Lang::GraphEvent(ev) => self.handle_graph_event(state, ev),
            Lang::LayersEvent(ev) => self.handle_layers_event(state, ev),
            _ => {}
        }
    }
    /// Handle Graph Events
    fn handle_graph_event(&self, state: &mut widget::State<State>, event: &GraphEvent) {
        match event {
            GraphEvent::GraphAdded(res) => {
                state.update(|state| state.graphs.add_graph(res.clone()));
            }
            GraphEvent::GraphRemoved(res) => {
                state.update(|state| state.graphs.remove_graph(res));
                self.app_data
                    .sender
                    .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
                        state.graphs.get_active().clone(),
                    )))
                    .unwrap();
            }
            GraphEvent::GraphRenamed(from, to) => {
                state.update(|state| state.graphs.rename_collection(from, to));
            }
            GraphEvent::NodeAdded(res, op, pbox, position, _size) => {
                state.update(|state| {
                    state.graphs.add_node(NodeData::new(
                        res.clone(),
                        position.map(|(x, y)| [x, y]),
                        &op,
                        pbox.clone(),
                    ))
                });
            }
            GraphEvent::NodeRemoved(res) => {
                state.update(|state| state.graphs.remove_node(res));
            }
            GraphEvent::NodeRenamed(from, to) => {
                state.update(|state| state.graphs.rename_node(from, to));
            }
            GraphEvent::ComplexOperatorUpdated(node, op, pbox) => {
                state.update(|state| state.graphs.update_complex_operator(node, op, pbox));
            }
            GraphEvent::ConnectedSockets(from, to) => {
                state.update(|state| state.graphs.connect_sockets(from, to))
            }
            GraphEvent::DisconnectedSockets(from, to) => {
                state.update(|state| state.graphs.disconnect_sockets(from, to))
            }
            GraphEvent::SocketMonomorphized(socket, ty) => {
                state.update(|state| state.graphs.monomorphize_socket(socket, *ty));
            }
            GraphEvent::SocketDemonomorphized(socket) => {
                state.update(|state| state.graphs.demonomorphize_socket(socket))
            }
            GraphEvent::Cleared => {
                state.update(|state| state.graphs.clear_all());
            }
            GraphEvent::ParameterExposed(graph, param) => {
                state.update(|state| state.graphs.parameter_exposed(graph, param.clone()));
            }
            GraphEvent::ParameterConcealed(graph, field) => {
                state.update(|state| state.graphs.parameter_concealed(graph, field));
            }
            _ => {}
        }
    }

    /// Handle layer events
    fn handle_layers_event(&self, state: &mut widget::State<State>, event: &LayersEvent) {
        match event {
            LayersEvent::LayersAdded(res, _) => {
                state.update(|state| state.graphs.add_layers(res.clone()));
            }
            LayersEvent::LayersRemoved(res) => {
                state.update(|state| state.graphs.remove_layers(res));
                self.app_data
                    .sender
                    .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
                        state.graphs.get_active().clone(),
                    )))
                    .unwrap();
            }
            LayersEvent::LayerPushed(res, ty, title, _, bmode, opacity, pbox, _) => {
                let layer = Layer::layer(
                    res.clone(),
                    *ty,
                    title,
                    pbox.clone(),
                    *bmode as usize,
                    *opacity,
                );
                state.update(|state| state.graphs.push_layer(layer));
            }
            LayersEvent::LayerRemoved(res) => {
                state.update(|state| state.graphs.remove_layer(res));
            }
            LayersEvent::MaskPushed(for_layer, res, title, _, bmode, opacity, pbox, _) => {
                let layer =
                    Layer::mask(res.clone(), title, pbox.clone(), *bmode as usize, *opacity);
                state.update(|state| state.graphs.push_layer_under(layer, for_layer));
            }
            LayersEvent::MovedUp(res) => {
                state.update(|state| state.graphs.move_layer_up(res));
            }
            LayersEvent::MovedDown(res) => {
                state.update(|state| state.graphs.move_layer_down(res));
            }
        }
    }
    /// Updates the top bar
    fn update_top_bar(&self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::top_bar;

        state.update(|state| {
            top_bar::TopBar::new(
                &self.app_data.language,
                &self.app_data.sender,
                &mut state.graphs,
            )
            .icon_font(self.style.icon_font.unwrap().unwrap())
            .parent(state.ids.top_bar_canvas)
            .wh_of(state.ids.top_bar_canvas)
            .middle_of(state.ids.top_bar_canvas)
            .set(state.ids.top_bar, ui)
        });
    }

    /// Updates the node graph widget
    fn update_node_graph(&self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::node_editor;

        state.update(|state| {
            node_editor::NodeEditor::new(&self.app_data.sender, &mut state.graphs)
                .event_buffer(self.event_buffer.unwrap())
                .parent(state.ids.edit_canvas)
                .wh_of(state.ids.edit_canvas)
                .middle_of(state.ids.edit_canvas)
                .set(state.ids.node_editor, ui)
        });
    }

    // /// Updates the layer stack widget
    fn update_layer_stack(&self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::layer_editor;

        state.update(|state| {
            layer_editor::LayerEditor::new(
                &self.app_data.language,
                &self.app_data.sender,
                &mut state.graphs,
            )
            .event_buffer(self.event_buffer.unwrap())
            .icon_font(self.style.icon_font.unwrap().unwrap())
            .parent(state.ids.edit_canvas)
            .wh_of(state.ids.edit_canvas)
            .middle_of(state.ids.edit_canvas)
            .set(state.ids.layer_editor, ui)
        });
    }

    // /// Updates the viewport
    fn update_viewport(&mut self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::viewport;

        state.update(|state| {
            viewport::Viewport::new_3d(
                &self.app_data.language,
                &self.app_data.sender,
                &mut self.renderer,
                &mut self.app_data.image_map,
            )
            .event_buffer(self.event_buffer.unwrap())
            .icon_font(self.style.icon_font.unwrap().unwrap())
            .monitor_resolution(self.app_data.monitor_resolution)
            .parent(state.ids.drawing_canvas)
            .wh_of(state.ids.drawing_canvas)
            .middle_of(state.ids.drawing_canvas)
            .set(state.ids.viewport, ui)
        });
    }

    /// Updates the parameter section of the sidebar
    fn update_parameter_section(&self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::parameter_section;

        state.update(|state| {
            if let Some((resource, description)) = state.graphs.active_parameters() {
                parameter_section::ParameterSection::new(
                    &self.app_data.language,
                    &self.app_data.sender,
                    description,
                    resource,
                )
                .image_resources(&state.image_resources)
                .icon_font(self.style.icon_font.unwrap().unwrap())
                .parent(state.ids.parameter_canvas)
                .wh_of(state.ids.parameter_canvas)
                .middle_of(state.ids.parameter_canvas)
                .set(state.ids.parameter_section, ui);
            }
        });
    }

    /// Updates the graph section of the sidebar
    fn update_graph_section(&self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::graph_section;

        state.update(|state| {
            graph_section::GraphSection::new(
                &self.app_data.language,
                &self.app_data.sender,
                &mut state.graphs,
            )
            .icon_font(self.style.icon_font.unwrap().unwrap())
            .parent(state.ids.graph_settings_canvas)
            .wh_of(state.ids.graph_settings_canvas)
            .middle_of(state.ids.graph_settings_canvas)
            .set(state.ids.graph_section, ui)
        });
    }

    /// Updates the surface section of the sidebar
    fn update_surface_section(&self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::surface_section;

        state.update(|state| {
            surface_section::SurfaceSection::new(&self.app_data.language, &self.app_data.sender)
                .event_buffer(self.event_buffer.unwrap())
                .icon_font(self.style.icon_font.unwrap().unwrap())
                .parent(state.ids.surface_settings_canvas)
                .wh_of(state.ids.surface_settings_canvas)
                .middle_of(state.ids.surface_settings_canvas)
                .set(state.ids.surface_section, ui)
        });
    }

    /// Updates the resource browser
    fn update_resource_browser(&self, state: &mut widget::State<State>, ui: &mut UiCell) {
        use components::resource_browser;

        state.update(|state| {
            resource_browser::ResourceBrowser::new(
                &self.app_data.language,
                &self.app_data.sender,
                &mut state.graphs,
            )
            .event_buffer(self.event_buffer.unwrap())
            .icon_font(self.style.icon_font.unwrap().unwrap())
            .parent(state.ids.resources_canvas)
            .wh_of(state.ids.resources_canvas)
            .middle_of(state.ids.resources_canvas)
            .set(state.ids.resource_browser, ui)
        });
    }
}
