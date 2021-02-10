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
        viewport,
        parameter_section,
        graph_section,

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
            &self.app_state.addable_operators,
        )
        .parent(self.ids.edit_canvas)
        .wh_of(self.ids.edit_canvas)
        .middle_of(self.ids.edit_canvas)
        .set(self.ids.node_editor, ui);
    }

    /// Updates the layer stack widget
    fn layer_stack(&mut self, ui: &mut UiCell) {
        use super::util::*;
        use strum::VariantNames;
        use widgets::layer_row;
        use widgets::tree;

        let graphs = &mut self.app_state.graphs;
        let lang = &self.language;

        for _press in icon_button(IconName::SOLID, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.)
            .wh([32., 32.0])
            .top_left_with_margin(8.0)
            .parent(self.ids.edit_canvas)
            .set(self.ids.layer_new_fill, ui)
        {
            self.app_state.add_layer_modal = Some(LayerFilter::Layer(LayerType::Fill));
        }

        for _press in icon_button(IconName::FX, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.edit_canvas)
            .set(self.ids.layer_new_fx, ui)
        {
            self.app_state.add_layer_modal = Some(LayerFilter::Layer(LayerType::Fx));
        }

        let active_collection = match graphs.get_active_collection_mut() {
            NodeCollection::Layers(l) => l,
            _ => panic!("Layers UI built for graph"),
        };

        if let Some((is_base, active_layer)) =
            self.app_state.active_layer_element.clone().map(|node_id| {
                (
                    active_collection.is_base_layer(&node_id),
                    active_collection
                        .layers
                        .get_mut(&node_id)
                        .unwrap()
                        .data_mut(),
                )
            })
        {
            for _press in icon_button(IconName::TRASH, self.fonts.icon_font)
                .label_font_size(14)
                .label_color(color::WHITE)
                .color(color::DARK_CHARCOAL)
                .border(0.)
                .wh([32., 32.0])
                .top_right_with_margin(8.0)
                .parent(self.ids.edit_canvas)
                .set(self.ids.layer_delete, ui)
            {
                self.sender
                    .send(if active_layer.is_mask {
                        Lang::UserLayersEvent(UserLayersEvent::RemoveMask(
                            active_layer.resource.clone(),
                        ))
                    } else {
                        Lang::UserLayersEvent(UserLayersEvent::RemoveLayer(
                            active_layer.resource.clone(),
                        ))
                    })
                    .unwrap();
                self.app_state.active_layer_element = None;
            }

            if !is_base && !active_layer.is_mask {
                for _press in icon_button(IconName::MASK, self.fonts.icon_font)
                    .label_font_size(14)
                    .label_color(color::WHITE)
                    .color(color::DARK_CHARCOAL)
                    .border(0.)
                    .wh([32., 32.0])
                    .left(8.0)
                    .parent(self.ids.edit_canvas)
                    .set(self.ids.layer_new_mask, ui)
                {
                    self.app_state.add_layer_modal =
                        Some(LayerFilter::Mask(active_layer.resource.clone()));
                }
            }

            if let Some(new_selection) =
                widget::DropDownList::new(BlendMode::VARIANTS, Some(active_layer.blend_mode))
                    .label_font_size(10)
                    .down_from(self.ids.layer_new_fill, 8.0)
                    .padded_w_of(self.ids.edit_canvas, 8.0)
                    .h(16.0)
                    .parent(self.ids.edit_canvas)
                    .set(self.ids.layer_blend_mode, ui)
            {
                use strum::IntoEnumIterator;

                active_layer.blend_mode = new_selection;

                self.sender
                    .send(Lang::UserLayersEvent(UserLayersEvent::SetBlendMode(
                        active_layer.resource.clone(),
                        BlendMode::iter().nth(new_selection).unwrap(),
                    )))
                    .unwrap();
            }

            if let Some(new_value) = widget::Slider::new(active_layer.opacity, 0.0, 1.0)
                .label(&lang.get_message("opacity"))
                .label_font_size(10)
                .down(8.0)
                .padded_w_of(self.ids.edit_canvas, 8.0)
                .h(16.0)
                .parent(self.ids.edit_canvas)
                .set(self.ids.layer_opacity, ui)
            {
                active_layer.opacity = new_value;

                self.sender
                    .send(Lang::UserLayersEvent(UserLayersEvent::SetOpacity(
                        active_layer.resource.clone(),
                        new_value,
                    )))
                    .unwrap();
            }
        } else {
            widget::DropDownList::new(BlendMode::VARIANTS, Some(0))
                .enabled(false)
                .label_font_size(10)
                .down_from(self.ids.layer_new_fill, 8.0)
                .padded_w_of(self.ids.edit_canvas, 8.0)
                .h(16.0)
                .parent(self.ids.edit_canvas)
                .set(self.ids.layer_blend_mode, ui);

            widget::Slider::new(1.0, 0.0, 1.0)
                .enabled(false)
                .label(&lang.get_message("opacity"))
                .label_font_size(10)
                .down(8.0)
                .padded_w_of(self.ids.edit_canvas, 8.0)
                .h(16.0)
                .parent(self.ids.edit_canvas)
                .set(self.ids.layer_opacity, ui);
        }

        let (mut rows, scrollbar) = tree::Tree::without_root(&active_collection.layers)
            .parent(self.ids.edit_canvas)
            .item_size(48.0)
            .padded_w_of(self.ids.edit_canvas, 8.0)
            .middle_of(self.ids.edit_canvas)
            .h(512.0)
            .down(8.0)
            .scrollbar_on_top()
            .set(self.ids.layer_list, ui);

        while let Some(row) = rows.next(ui) {
            let node_id = row.node_id.clone();
            let toggleable = !active_collection.is_base_layer(&node_id);
            let expandable = active_collection.expandable(&node_id);
            let data = &mut active_collection
                .layers
                .get_mut(&node_id)
                .unwrap()
                .data_mut();

            let widget = layer_row::LayerRow::new(
                data,
                Some(row.node_id) == self.app_state.active_layer_element,
            )
            .toggleable(toggleable)
            .expandable(expandable)
            .icon_font(self.fonts.icon_font);

            if let Some(event) = row.item.set(widget, ui) {
                match event {
                    layer_row::Event::ActiveElement => {
                        self.app_state.active_layer_element = Some(node_id);
                    }
                    layer_row::Event::Retitled(new) => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::SetTitle(
                                data.resource.to_owned(),
                                new,
                            )))
                            .unwrap();
                    }
                    layer_row::Event::ToggleEnabled => {
                        data.enabled = !data.enabled;
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::SetEnabled(
                                data.resource.to_owned(),
                                data.enabled,
                            )))
                            .unwrap();
                    }
                    layer_row::Event::ToggleExpanded => {
                        data.toggle_expanded();
                    }
                    layer_row::Event::MoveUp => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::MoveUp(
                                data.resource.clone(),
                            )))
                            .unwrap();
                    }
                    layer_row::Event::MoveDown => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::MoveDown(
                                data.resource.clone(),
                            )))
                            .unwrap();
                    }
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }

        if let Some(filter) = self.app_state.add_layer_modal.as_ref().cloned() {
            use widgets::modal;

            let mut operators = self
                .app_state
                .addable_operators
                .iter()
                .filter(|o| match filter {
                    LayerFilter::Layer(LayerType::Fill) => o.inputs().is_empty(),
                    LayerFilter::Layer(LayerType::Fx) => !o.inputs().is_empty(),
                    LayerFilter::Mask(..) => o.is_mask(),
                });

            match modal::Modal::new(
                widget::List::flow_down(operators.clone().count())
                    .item_size(50.0)
                    .scrollbar_on_top(),
            )
            .padding(32.0)
            .wh_of(self.ids.edit_canvas)
            .middle_of(self.ids.edit_canvas)
            .graphics_for(self.ids.edit_canvas)
            .set(self.ids.add_layer_modal, ui)
            {
                modal::Event::ChildEvent(((mut items, scrollbar), _)) => {
                    while let Some(item) = items.next(ui) {
                        let op = operators.next().unwrap();
                        let label = op.title();
                        let button = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::CHARCOAL);
                        for _press in item.set(button, ui) {
                            self.app_state.add_layer_modal = None;

                            self.sender
                                .send(match &filter {
                                    LayerFilter::Layer(filter) => {
                                        Lang::UserLayersEvent(UserLayersEvent::PushLayer(
                                            self.app_state.graphs.get_active().clone(),
                                            *filter,
                                            op.clone(),
                                        ))
                                    }
                                    LayerFilter::Mask(for_layer) => Lang::UserLayersEvent(
                                        UserLayersEvent::PushMask(for_layer.clone(), op.clone()),
                                    ),
                                })
                                .unwrap();
                        }
                    }

                    if let Some(s) = scrollbar {
                        s.set(ui)
                    }
                }
                modal::Event::Hide => self.app_state.add_layer_modal = None,
            }
        }
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
        use super::util::*;
        use widgets::{export_row, param_box};

        for ev in param_box::ParamBox::new(&mut self.app_state.surface_params, &(), &self.language)
            .parent(self.ids.surface_settings_canvas)
            .w_of(self.ids.surface_settings_canvas)
            .mid_top()
            .set(self.ids.surface_param_box, ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new(&self.label_text("export-spec"))
            .parent(self.ids.surface_settings_canvas)
            .mid_top_with_margin(96.0)
            .color(color::WHITE)
            .font_size(12)
            .set(self.ids.export_label, ui);

        for _ev in icon_button(IconName::PLUS, self.fonts.icon_font)
            .parent(self.ids.surface_settings_canvas)
            .top_right_with_margins(96.0, 16.0)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .label_font_size(12)
            .wh([20.0, 16.0])
            .set(self.ids.export_add, ui)
        {
            self.app_state.add_export_entry();
        }

        let (mut rows, scrollbar) = widget::List::flow_down(self.app_state.export_entries.len())
            .parent(self.ids.surface_settings_canvas)
            .padded_w_of(self.ids.surface_settings_canvas, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0)
            .scrollbar_on_top()
            .set(self.ids.export_list, ui);

        while let Some(row) = rows.next(ui) {
            let widget = export_row::ExportRow::new(
                &self.app_state.export_entries[row.i],
                &self.app_state.registered_sockets,
                &self.language,
            );
            let mut updated_spec = false;
            match row.set(widget, ui) {
                Some(export_row::Event::ChangeToRGB) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .set_has_alpha(false);
                    updated_spec = true;
                }
                Some(export_row::Event::ChangeToRGBA) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .set_has_alpha(true);
                    updated_spec = true;
                }
                Some(export_row::Event::ChangeToGrayscale) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Grayscale);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelR(spec)) => {
                    self.app_state.export_entries[row.i].1.set_red(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelG(spec)) => {
                    self.app_state.export_entries[row.i].1.set_green(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelB(spec)) => {
                    self.app_state.export_entries[row.i].1.set_blue(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelA(spec)) => {
                    self.app_state.export_entries[row.i].1.set_alpha(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::Rename(new)) => {
                    // TODO: renaming two specs to the same name causes discrepancies with the backend
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::RenameExport(
                            self.app_state.export_entries[row.i].0.clone(),
                            new.clone(),
                        )))
                        .unwrap();
                    self.app_state.export_entries[row.i].0 = new;
                }
                None => {}
            }

            if updated_spec {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::DeclareExport(
                        self.app_state.export_entries[row.i].0.clone(),
                        self.app_state.export_entries[row.i].1.clone(),
                    )))
                    .unwrap();
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }

    /// Updates the resource browser
    fn resource_browser(&mut self, ui: &mut UiCell) {
        use crate::ui::widgets::{resource_row, tree};

        let (mut rows, scrollbar) = tree::Tree::new(self.app_state.resource_tree.get_tree())
            .parent(self.ids.resources_canvas)
            .middle_of(self.ids.resources_canvas)
            .padded_w_of(self.ids.resources_canvas, 8.0)
            .padded_h_of(self.ids.resources_canvas, 8.0)
            .scrollbar_on_top()
            .set(self.ids.resource_tree, ui);

        while let Some(row) = rows.next(ui) {
            let expandable = self.app_state.resource_tree.expandable(&row.node_id);
            let data = self
                .app_state
                .resource_tree
                .get_resource_info_mut(&row.node_id);

            let widget = resource_row::ResourceRow::new(&data, row.level)
                .expandable(expandable)
                .icon_font(self.fonts.icon_font)
                .h(32.0);

            match row.item.set(widget, ui) {
                None => {}
                Some(resource_row::Event::ToggleExpanded) => {
                    data.toggle_expanded();
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }
}
