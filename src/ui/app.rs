use crate::{broker::BrokerSender, lang::*};
use conrod_core::*;

use super::app_state::*;
use super::i18n::*;
use super::{components, widgets};

const PANEL_COLOR: Color = color::DARK_CHARCOAL;
const PANEL_GAP: Scalar = 0.5;

// TODO: Unify margins and paddings somehow in UI

widget_ids!(
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

        // Top Buttons
        new_surface,
        open_surface,
        save_surface,
        export_surface,
        graph_selector,
        graph_add,
        layers_add,

        // Main Views
        node_graph,
        render_view,
        add_node_modal,
        add_layer_modal,
        render_modal,

        // Layers
        layer_opacity,
        layer_blend_mode,
        layer_new_fill,
        layer_new_fx,
        layer_new_mask,
        layer_delete,
        layer_list,
        layer_convert,

        // Render Modal
        render_params,

        // Parameter Area
        node_param_box,
        graph_param_box,
        surface_param_box,
        exposed_param_title,
        exposed_param_list,

        // Exporting
        export_label,
        export_add,
        export_list,

        // Resources
        resource_tree,
    }
);

/// GUI container type. Contains everything required to run the UI, including UI
/// state and the broker sender.
pub struct Gui<B: crate::gpu::Backend> {
    ids: Ids,
    fonts: AppFonts,
    app_state: App,
    sender: BrokerSender<Lang>,
    image_map: image::Map<crate::gpu::ui::Image<B>>,
    language: Language,
}

impl<B> Gui<B>
where
    B: crate::gpu::Backend,
{
    /// Create a new GUI instance
    pub fn new(
        ids: Ids,
        fonts: AppFonts,
        sender: BrokerSender<Lang>,
        monitor_size: (u32, u32),
        image_map: image::Map<crate::gpu::ui::Image<B>>,
    ) -> Self {
        Self {
            ids,
            fonts,
            app_state: App::new(monitor_size),
            sender,
            image_map,
            language: Language::default(),
        }
    }

    /// Translate a message given a placeholder string. Uses the language currently defined in the GUI.
    pub fn label_text(&self, id: &'static str) -> std::borrow::Cow<str> {
        self.language.get_message(id)
    }

    /// Obtain a reference to the image map
    pub fn image_map(&self) -> &image::Map<crate::gpu::ui::Image<B>> {
        &self.image_map
    }

    /// Handle UI event
    pub fn handle_event(
        &mut self,
        ui: &mut Ui,
        renderer: &mut crate::gpu::ui::Renderer<B>,
        event: &Lang,
    ) {
        match event {
            Lang::RenderEvent(RenderEvent::RendererAdded(_id, view)) => {
                if let Some(view) = view.clone().to::<B>() {
                    if let Some(img) = renderer.create_image(
                        view,
                        self.app_state.monitor_resolution.0,
                        self.app_state.monitor_resolution.1,
                    ) {
                        let id = self.image_map.insert(img);
                        self.app_state.render_image = RenderImage::Image(id);
                    }
                }
            }
            Lang::RenderEvent(RenderEvent::RendererRedrawn(_id)) => {
                ui.needs_redraw();
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailCreated(res, thmb)) => {
                if let Some(t) = thmb.clone().to::<B>() {
                    if let Some(img) = renderer.create_image(t, 128, 128) {
                        let id = self.image_map.insert(img);
                        self.app_state.graphs.register_thumbnail(&res, id);
                    }
                }
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailDestroyed(res)) => {
                if let Some(id) = self.app_state.graphs.unregister_thumbnail(&res) {
                    self.image_map.remove(id);
                }
            }
            Lang::ComputeEvent(ComputeEvent::SocketCreated(res, ty)) => match ty {
                ImageType::Grayscale => {
                    self.app_state.registered_sockets.push(
                        widgets::export_row::RegisteredSocket::new((res.clone(), ImageChannel::R)),
                    );
                }
                ImageType::Rgb => {
                    self.app_state.registered_sockets.push(
                        widgets::export_row::RegisteredSocket::new((res.clone(), ImageChannel::R)),
                    );
                    self.app_state.registered_sockets.push(
                        widgets::export_row::RegisteredSocket::new((res.clone(), ImageChannel::G)),
                    );
                    self.app_state.registered_sockets.push(
                        widgets::export_row::RegisteredSocket::new((res.clone(), ImageChannel::B)),
                    );
                }
            },
            Lang::ComputeEvent(ComputeEvent::SocketDestroyed(res)) => {
                self.app_state
                    .registered_sockets
                    .drain_filter(|x| x.resource() == res);
            }
            Lang::GraphEvent(ev) => self.handle_graph_event(ev),
            Lang::LayersEvent(ev) => self.handle_layers_event(ev),
            Lang::SurfaceEvent(SurfaceEvent::ExportSpecLoaded(name, spec)) => {
                self.app_state
                    .export_entries
                    .push((name.clone(), spec.clone()));
            }
            _ => {}
        }
    }

    /// Handle Graph Events
    fn handle_graph_event(&mut self, event: &GraphEvent) {
        match event {
            GraphEvent::GraphAdded(res) => {
                self.app_state.graphs.add_graph(res.clone());
                self.app_state
                    .registered_operators
                    .push(Operator::ComplexOperator(ComplexOperator::new(res.clone())));
                self.app_state.resource_tree.insert_graph(res.clone())
            }
            GraphEvent::GraphRenamed(from, to) => {
                self.app_state.graphs.rename_collection(from, to);
                let old_op = Operator::ComplexOperator(ComplexOperator::new(from.clone()));
                self.app_state.registered_operators.remove(
                    self.app_state
                        .registered_operators
                        .iter()
                        .position(|x| x == &old_op)
                        .expect("Missing old operator"),
                );
                self.app_state
                    .registered_operators
                    .push(Operator::ComplexOperator(ComplexOperator::new(to.clone())));
                self.app_state.resource_tree.rename_resource(from, to);
            }
            GraphEvent::NodeAdded(res, op, pbox, position, _size) => {
                self.app_state.graphs.add_node(NodeData::new(
                    res.clone(),
                    position.map(|(x, y)| [x, y]),
                    &op,
                    pbox.clone(),
                ));
                self.app_state.resource_tree.insert_node(res.clone());
            }
            GraphEvent::NodeRemoved(res) => {
                self.app_state.graphs.remove_node(res);
                self.app_state
                    .resource_tree
                    .remove_resource_and_children(res);
            }
            GraphEvent::NodeRenamed(from, to) => {
                self.app_state.graphs.rename_node(from, to);
                self.app_state.resource_tree.rename_resource(from, to);
            }
            GraphEvent::ComplexOperatorUpdated(node, op, pbox) => {
                self.app_state
                    .graphs
                    .update_complex_operator(node, op, pbox);
            }
            GraphEvent::ConnectedSockets(from, to) => {
                self.app_state.graphs.connect_sockets(from, to)
            }
            GraphEvent::DisconnectedSockets(from, to) => {
                self.app_state.graphs.disconnect_sockets(from, to)
            }
            GraphEvent::SocketMonomorphized(socket, ty) => {
                self.app_state.graphs.monomorphize_socket(socket, *ty)
            }
            GraphEvent::SocketDemonomorphized(socket) => {
                self.app_state.graphs.demonomorphize_socket(socket)
            }
            GraphEvent::Cleared => {
                self.app_state.graphs.clear_all();
                self.app_state.export_entries.clear();
                self.app_state.registered_sockets.clear();
            }
            GraphEvent::ParameterExposed(graph, param) => {
                self.app_state
                    .graphs
                    .parameter_exposed(graph, param.clone());
            }
            GraphEvent::ParameterConcealed(graph, field) => {
                self.app_state.graphs.parameter_concealed(graph, field);
            }
            _ => {}
        }
    }

    /// Handle layer events
    fn handle_layers_event(&mut self, event: &LayersEvent) {
        match event {
            LayersEvent::LayersAdded(res, _) => {
                self.app_state.graphs.add_layers(res.clone());
                self.app_state
                    .registered_operators
                    .push(Operator::ComplexOperator(ComplexOperator::new(res.clone())));
                self.app_state.resource_tree.insert_graph(res.clone())
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
                self.app_state.graphs.push_layer(layer);
            }
            LayersEvent::LayerRemoved(res) => {
                self.app_state.graphs.remove_layer(res);
            }
            LayersEvent::MaskPushed(for_layer, res, title, _, bmode, opacity, pbox, _) => {
                let layer =
                    Layer::mask(res.clone(), title, pbox.clone(), *bmode as usize, *opacity);
                self.app_state.graphs.push_layer_under(layer, for_layer);
            }
            LayersEvent::MovedUp(res) => {
                self.app_state.graphs.move_layer_up(res);
            }
            LayersEvent::MovedDown(res) => {
                self.app_state.graphs.move_layer_down(res);
            }
        }
    }

    /// GUI update function. Called per frame. This function constructs the
    /// entire UI with updated parameters and processes events since the last
    /// frame. Widgets are however cached on the backend to help with
    /// performance.
    pub fn update_gui(&mut self, ui: &mut UiCell) {
        use widgets::tabs;

        let edit_width = match self.app_state.graphs.get_active_collection() {
            NodeCollection::Graph(_) => None,
            NodeCollection::Layers(_) => Some(384.0),
        };

        // Main canvasses
        widget::Canvas::new()
            .border(0.0)
            .color(PANEL_COLOR)
            .flow_right(&[
                (
                    self.ids.main_canvas,
                    widget::Canvas::new()
                        .border(PANEL_GAP)
                        .color(color::DARK_CHARCOAL)
                        .flow_down(&[
                            (
                                self.ids.top_bar_canvas,
                                widget::Canvas::new()
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP)
                                    .length(48.0),
                            ),
                            (
                                self.ids.main_inner_canvas,
                                widget::Canvas::new()
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP)
                                    .flow_right(&[
                                        (self.ids.edit_canvas, {
                                            let mut w = widget::Canvas::new()
                                                .color(PANEL_COLOR)
                                                .border(PANEL_GAP);
                                            if let Some(x) = edit_width {
                                                w = w.length(x);
                                            }
                                            w
                                        }),
                                        (
                                            self.ids.drawing_canvas,
                                            widget::Canvas::new()
                                                .color(PANEL_COLOR)
                                                .border(PANEL_GAP),
                                        ),
                                    ]),
                            ),
                        ]),
                ),
                (
                    self.ids.sidebar_canvas,
                    widget::Canvas::new()
                        .border(PANEL_GAP)
                        .color(PANEL_COLOR)
                        .length(384.0)
                        .flow_down(&[
                            (
                                self.ids.settings_canvas,
                                widget::Canvas::new()
                                    .border(PANEL_GAP)
                                    .color(PANEL_COLOR)
                                    .length_weight(0.66),
                            ),
                            (
                                self.ids.resources_canvas,
                                widget::Canvas::new()
                                    .border(PANEL_GAP)
                                    .color(PANEL_COLOR)
                                    .length_weight(0.33),
                            ),
                        ]),
                ),
            ])
            .set(self.ids.window_canvas, ui);

        // Side tabs
        tabs::Tabs::new(&[
            (
                self.ids.parameter_canvas,
                &self.label_text("parameters-tab"),
            ),
            (
                self.ids.graph_settings_canvas,
                &self.label_text("graph-tab"),
            ),
            (
                self.ids.surface_settings_canvas,
                &self.label_text("surface-tab"),
            ),
        ])
        .color(PANEL_COLOR)
        .label_color(color::WHITE)
        .label_font_size(10)
        .bar_thickness(48.0)
        .border(PANEL_GAP)
        .parent(self.ids.settings_canvas)
        .wh_of(self.ids.settings_canvas)
        .middle()
        .set(self.ids.sidebar_tabs, ui);

        // Call update functions for each part of the UI
        self.top_bar(ui);
        match self.app_state.graphs.get_active_collection() {
            NodeCollection::Graph(_) => self.node_graph(ui),
            NodeCollection::Layers(_) => self.layer_stack(ui),
        };
        self.render_view(ui);
        self.parameter_section(ui);
        self.graph_section(ui);
        self.surface_section(ui);
        self.resource_browser(ui);
    }

    /// Updates the top bar
    fn top_bar(&mut self, ui: &mut UiCell) {
        use components::top_bar;

        top_bar::TopBar::new(&self.language, &self.sender, &mut self.app_state.graphs)
            .icon_font(self.fonts.icon_font)
            .parent(self.ids.top_bar_canvas)
            .wh_of(self.ids.top_bar_canvas)
            .middle_of(self.ids.top_bar_canvas)
            .set(self.ids.top_bar, ui);
    }

    /// Updates the node graph widget
    fn node_graph(&mut self, ui: &mut UiCell) {
        use components::node_editor;

        node_editor::NodeEditor::new(
            &self.sender,
            &mut self.app_state.graphs,
            &mut self.app_state.active_node_element,
            &self.app_state.addable_operators,
        )
        .parent(self.ids.edit_canvas)
        .wh_of(self.ids.edit_canvas)
        .middle_of(self.ids.edit_canvas)
        .set(self.ids.node_editor, ui);
    }

    /// Updates the layer stack widget
    fn layer_stack(&mut self, ui: &mut UiCell) {
        use components::layer_editor;

        layer_editor::LayerEditor::new(
            &self.language,
            &self.sender,
            &mut self.app_state.graphs,
            &mut self.app_state.active_layer_element,
            &self.app_state.addable_operators,
        )
        .icon_font(self.fonts.icon_font)
        .parent(self.ids.edit_canvas)
        .wh_of(self.ids.edit_canvas)
        .middle_of(self.ids.edit_canvas)
        .set(self.ids.layer_editor, ui);
    }

    /// Updates a render view
    fn render_view(&mut self, ui: &mut UiCell) {
        use components::viewport;

        viewport::Viewport::new(
            &self.language,
            &self.sender,
            &mut self.app_state.render_image,
        )
        .icon_font(self.fonts.icon_font)
        .monitor_resolution(self.app_state.monitor_resolution)
        .parent(self.ids.drawing_canvas)
        .wh_of(self.ids.drawing_canvas)
        .middle_of(self.ids.drawing_canvas)
        .set(self.ids.viewport, ui);
    }

    /// Updates the parameter section of the sidebar
    fn parameter_section(&mut self, ui: &mut UiCell) {
        use components::parameter_section;

        if let Some((description, resource)) = self.app_state.graphs.active_parameters(
            self.app_state.active_node_element,
            self.app_state.active_layer_element.clone(),
        ) {
            parameter_section::ParameterSection::new(
                &self.language,
                &self.sender,
                description,
                resource,
            )
            .icon_font(self.fonts.icon_font)
            .parent(self.ids.parameter_canvas)
            .wh_of(self.ids.parameter_canvas)
            .middle_of(self.ids.parameter_canvas)
            .set(self.ids.parameter_section, ui);
        }
    }

    /// Updates the graph section of the sidebar
    fn graph_section(&mut self, ui: &mut UiCell) {
        use components::graph_section;

        graph_section::GraphSection::new(&self.language, &self.sender, &mut self.app_state.graphs)
            .parent(self.ids.graph_settings_canvas)
            .wh_of(self.ids.graph_settings_canvas)
            .middle_of(self.ids.graph_settings_canvas)
            .set(self.ids.graph_section, ui);
    }

    /// Updates the surface section of the sidebar
    fn surface_section(&mut self, ui: &mut UiCell) {
        use components::surface_section;

        surface_section::SurfaceSection::new(
            &self.language,
            &self.sender,
            &mut self.app_state.surface_params,
            &mut self.app_state.export_entries,
            &self.app_state.registered_sockets,
        )
        .icon_font(self.fonts.icon_font)
        .parent(self.ids.surface_settings_canvas)
        .wh_of(self.ids.surface_settings_canvas)
        .middle_of(self.ids.surface_settings_canvas)
        .set(self.ids.surface_section, ui);
    }

    /// Updates the resource browser
    fn resource_browser(&mut self, ui: &mut UiCell) {
        use components::resource_browser;

        resource_browser::ResourceBrowser::new(
            &self.language,
            &self.sender,
            &mut self.app_state.resource_tree,
        )
        .icon_font(self.fonts.icon_font)
        .parent(self.ids.resources_canvas)
        .wh_of(self.ids.resources_canvas)
        .middle_of(self.ids.resources_canvas)
        .set(self.ids.resource_browser, ui);
    }
}
