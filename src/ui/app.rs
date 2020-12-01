use crate::{broker::BrokerSender, lang::*};
use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};

use super::app_state::*;

const PANEL_COLOR: Color = color::DARK_CHARCOAL;
const PANEL_GAP: Scalar = 0.5;

// TODO: Unify margins and paddings somehow in UI

widget_ids!(
    pub struct Ids {
        // Main Areas
        window_canvas,
        top_bar_canvas,
        main_canvas,
        edit_canvas,
        drawing_canvas,
        sidebar_canvas,
        parameter_canvas,
        graph_settings_canvas,
        surface_settings_canvas,

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
    }
);

pub struct Gui<B: crate::gpu::Backend> {
    ids: Ids,
    fonts: AppFonts,
    app_state: App,
    sender: BrokerSender<Lang>,
    image_map: image::Map<crate::gpu::ui::Image<B>>,
}

impl<B> Gui<B>
where
    B: crate::gpu::Backend,
{
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
        }
    }

    pub fn label_text(&self, id: &'static str) -> std::borrow::Cow<str> {
        self.app_state.language.get_message(id)
    }

    pub fn image_map(&self) -> &image::Map<crate::gpu::ui::Image<B>> {
        &self.image_map
    }

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
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::R)),
                    );
                }
                ImageType::Rgb => {
                    self.app_state.registered_sockets.push(
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::R)),
                    );
                    self.app_state.registered_sockets.push(
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::G)),
                    );
                    self.app_state.registered_sockets.push(
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::B)),
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

    fn handle_graph_event(&mut self, event: &GraphEvent) {
        match event {
            GraphEvent::GraphAdded(res) => {
                self.app_state.graphs.add_graph(res.clone());
                self.app_state
                    .registered_operators
                    .push(Operator::ComplexOperator(ComplexOperator::new(res.clone())));
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
            }
            GraphEvent::NodeAdded(res, op, pbox, position, _size) => {
                self.app_state.graphs.add_node(NodeData::new(
                    res.clone(),
                    position.map(|(x, y)| [x, y]),
                    &op,
                    pbox.clone(),
                ));
            }
            GraphEvent::NodeRemoved(res) => {
                self.app_state.graphs.remove_node(res);
            }
            GraphEvent::NodeRenamed(from, to) => {
                self.app_state.graphs.rename_node(from, to);
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

    fn handle_layers_event(&mut self, event: &LayersEvent) {
        match event {
            LayersEvent::LayersAdded(res, _) => {
                self.app_state.graphs.add_layers(res.clone());
                self.app_state
                    .registered_operators
                    .push(Operator::ComplexOperator(ComplexOperator::new(res.clone())));
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

    pub fn update_gui(&mut self, ui: &mut UiCell) {
        use super::tabs;

        let [lw, mw, rw] = match self.app_state.graphs.get_active_collection() {
            NodeCollection::Graph(_) => [1., 1., 0.5],
            NodeCollection::Layers(_) => [0.5, 1.5, 0.5],
        };

        widget::Canvas::new()
            .border(0.0)
            .color(PANEL_COLOR)
            .flow_down(&[
                (
                    self.ids.top_bar_canvas,
                    widget::Canvas::new()
                        .length(48.0)
                        .border(PANEL_GAP)
                        .color(color::DARK_CHARCOAL),
                ),
                (
                    self.ids.main_canvas,
                    widget::Canvas::new()
                        .border(PANEL_GAP)
                        .color(PANEL_COLOR)
                        .flow_right(&[
                            (
                                self.ids.edit_canvas,
                                widget::Canvas::new()
                                    .scroll_kids()
                                    .length_weight(lw)
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP),
                            ),
                            (
                                self.ids.drawing_canvas,
                                widget::Canvas::new()
                                    .length_weight(mw)
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP),
                            ),
                            (
                                self.ids.sidebar_canvas,
                                widget::Canvas::new()
                                    .length_weight(rw)
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP),
                            ),
                        ]),
                ),
            ])
            .set(self.ids.window_canvas, ui);

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
        .parent(self.ids.sidebar_canvas)
        .wh_of(self.ids.sidebar_canvas)
        .middle()
        .set(self.ids.sidebar_tabs, ui);

        self.top_bar(ui);
        match self.app_state.graphs.get_active_collection() {
            NodeCollection::Graph(_) => self.node_graph(ui),
            NodeCollection::Layers(_) => self.layer_stack(ui),
        };
        self.render_view(ui);
        self.parameter_section(ui);
        self.graph_section(ui);
        self.surface_section(ui);
    }

    fn top_bar(&mut self, ui: &mut UiCell) {
        use super::util::*;

        for _press in icon_button(IconName::FOLDER_PLUS, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .mid_left_with_margin(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.new_surface, ui)
        {
            self.sender
                .send(Lang::UserIOEvent(UserIOEvent::NewSurface))
                .unwrap();
        }

        for _press in icon_button(IconName::FOLDER_OPEN, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.open_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new(self.label_text("surface-file-select"))
                .title(self.label_text("surface-open-title"))
                .mode(FileSelectionMode::Open)
                .show()
            {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::OpenSurface(
                        std::path::PathBuf::from(path),
                    )))
                    .unwrap();
                self.app_state.graphs.clear_all();
            }
        }

        for _press in icon_button(IconName::CONTENT_SAVE, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.save_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new(self.label_text("surface-file-select"))
                .title(self.label_text("surface-save-title"))
                .mode(FileSelectionMode::Save)
                .show()
            {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::SaveSurface(
                        std::path::PathBuf::from(path),
                    )))
                    .unwrap();
            }
        }

        for _press in icon_button(IconName::EXPORT, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.export_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new(self.label_text("base-name-select"))
                .title(self.label_text("surface-export-title"))
                .mode(FileSelectionMode::Save)
                .show()
            {
                let e_path = std::path::PathBuf::from(&path);
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::RunExports(e_path)))
                    .unwrap();
            }
        }

        if let Some(selection) =
            widget::DropDownList::new(&self.app_state.graphs.list_collection_names(), Some(0))
                .label_font_size(12)
                .parent(self.ids.top_bar_canvas)
                .mid_right_with_margin(8.0)
                .w(256.0)
                .set(self.ids.graph_selector, ui)
        {
            if let Some(graph) = self
                .app_state
                .graphs
                .get_collection_resource(selection)
                .cloned()
            {
                self.sender
                    .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
                        graph.clone(),
                    )))
                    .unwrap();
                self.app_state.graphs.set_active(graph);
                self.app_state.addable_operators = self
                    .app_state
                    .registered_operators
                    .iter()
                    .filter(|o| !o.is_graph(self.app_state.graphs.get_active()))
                    .cloned()
                    .collect();
            }
        }

        for _press in icon_button(IconName::GRAPH, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .left(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.graph_add, ui)
        {
            self.sender
                .send(Lang::UserGraphEvent(UserGraphEvent::AddGraph))
                .unwrap()
        }

        for _press in icon_button(IconName::LAYERS, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .left(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.layers_add, ui)
        {
            self.sender
                .send(Lang::UserLayersEvent(UserLayersEvent::AddLayers))
                .unwrap()
        }
    }

    fn node_graph(&mut self, ui: &mut UiCell) {
        use super::graph;

        let active = match self.app_state.graphs.get_active_collection_mut() {
            NodeCollection::Graph(g) => &mut g.graph,
            _ => panic!("Node Graph UI built for non-graph"),
        };

        for event in graph::Graph::new(&active)
            .parent(self.ids.edit_canvas)
            .wh_of(self.ids.edit_canvas)
            .middle()
            .set(self.ids.node_graph, ui)
        {
            match event {
                graph::Event::NodeDrag(idx, x, y) => {
                    let mut node = active.node_weight_mut(idx).unwrap();
                    node.position[0] += x;
                    node.position[1] += y;

                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                            node.resource.clone(),
                            (node.position[0], node.position[1]),
                        )))
                        .unwrap();
                }
                graph::Event::ConnectionDrawn(from, from_socket, to, to_socket) => {
                    let from_res = active
                        .node_weight(from)
                        .unwrap()
                        .resource
                        .node_socket(&from_socket);
                    let to_res = active
                        .node_weight(to)
                        .unwrap()
                        .resource
                        .node_socket(&to_socket);
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(
                            from_res, to_res,
                        )))
                        .unwrap();
                }
                graph::Event::NodeDelete(idx) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::RemoveNode(
                            active.node_weight(idx).unwrap().resource.clone(),
                        )))
                        .unwrap();
                }
                graph::Event::SocketClear(idx, socket) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                            active
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .node_socket(&socket),
                        )))
                        .unwrap();
                }
                graph::Event::ActiveElement(idx) => {
                    self.app_state.active_node_element = Some(idx);
                }
                graph::Event::AddModal(pt) => {
                    self.app_state.add_node_modal = Some(pt);
                }
            }
        }

        if let Some(insertion_pt) = self.app_state.add_node_modal {
            use super::modal;

            let operators = &self.app_state.addable_operators;

            match modal::Modal::new(
                widget::List::flow_down(operators.len())
                    .item_size(50.0)
                    .scrollbar_on_top(),
            )
            .wh_of(self.ids.edit_canvas)
            .middle_of(self.ids.edit_canvas)
            .graphics_for(self.ids.edit_canvas)
            .set(self.ids.add_node_modal, ui)
            {
                modal::Event::ChildEvent(((mut items, scrollbar), _)) => {
                    while let Some(item) = items.next(ui) {
                        let i = item.i;
                        let label = operators[i].title();
                        let button = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::CHARCOAL);
                        for _press in item.set(button, ui) {
                            self.app_state.add_node_modal = None;

                            self.sender
                                .send(Lang::UserNodeEvent(UserNodeEvent::NewNode(
                                    self.app_state.graphs.get_active().clone(),
                                    operators[i].clone(),
                                    (insertion_pt[0], insertion_pt[1]),
                                )))
                                .unwrap();
                        }
                    }

                    if let Some(s) = scrollbar {
                        s.set(ui)
                    }
                }
                modal::Event::Hide => {
                    self.app_state.add_node_modal = None;
                }
            }
        }
    }

    fn layer_stack(&mut self, ui: &mut UiCell) {
        use super::layer_row;
        use super::util::*;
        use strum::VariantNames;

        let graphs = &mut self.app_state.graphs;
        let lang = &self.app_state.language;

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

        if let Some((is_base, active_layer)) = self.app_state.active_layer_element.map(|idx| {
            (
                active_collection.layers.len() - 1 == idx,
                &mut active_collection.layers[idx],
            )
        }) {
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
                    .send(Lang::UserLayersEvent(UserLayersEvent::RemoveLayer(
                        active_layer.resource.clone(),
                    )))
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

        let nrows = active_collection.rows();
        let (mut rows, scrollbar) = widget::List::flow_down(nrows)
            .parent(self.ids.edit_canvas)
            .item_size(48.0)
            .padded_w_of(self.ids.edit_canvas, 8.0)
            .h(512.0)
            .down(8.0)
            .scrollbar_on_top()
            .set(self.ids.layer_list, ui);

        while let Some(row) = rows.next(ui) {
            let widget = layer_row::LayerRow::new(
                &mut active_collection.layers[row.i],
                Some(row.i) == self.app_state.active_layer_element,
            )
            .toggleable(row.i != nrows - 1)
            .icon_font(self.fonts.icon_font);

            if let Some(event) = row.set(widget, ui) {
                match event {
                    layer_row::Event::ActiveElement => {
                        self.app_state.active_layer_element = Some(row.i);
                    }
                    layer_row::Event::Retitled(new) => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::SetTitle(
                                active_collection.layers[row.i].resource.to_owned(),
                                new,
                            )))
                            .unwrap();
                    }
                    layer_row::Event::ToggleEnabled => {
                        active_collection.layers[row.i].enabled =
                            !active_collection.layers[row.i].enabled;
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::SetEnabled(
                                active_collection.layers[row.i].resource.to_owned(),
                                active_collection.layers[row.i].enabled,
                            )))
                            .unwrap();
                    }
                    layer_row::Event::MoveUp => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::MoveUp(
                                active_collection.layers[row.i].resource.clone(),
                            )))
                            .unwrap();
                    }
                    layer_row::Event::MoveDown => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::MoveDown(
                                active_collection.layers[row.i].resource.clone(),
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
            use super::modal;

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

    fn render_view(&mut self, ui: &mut UiCell) {
        use super::renderview::*;

        let renderer_id = self.ids.render_view.index() as u64;

        // If there is a known render image, create a render view for it
        match self.app_state.render_image {
            RenderImage::Image(render_image) => {
                let rv = RenderView::new(render_image, self.app_state.monitor_resolution)
                    .parent(self.ids.drawing_canvas)
                    .wh_of(self.ids.drawing_canvas)
                    .middle()
                    .set(self.ids.render_view, ui);

                // The widget itself does not communicate with the backend. Process
                // events here
                match rv {
                    Some(Event::Resized(w, h)) => self
                        .sender
                        .send(Lang::UIEvent(UIEvent::RendererResize(renderer_id, w, h)))
                        .unwrap(),
                    Some(Event::Rotate(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Rotate(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(Event::Pan(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Pan(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(Event::LightPan(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::LightMove(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(Event::Zoom(delta)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Zoom(
                            renderer_id,
                            delta,
                        )))
                        .unwrap(),
                    Some(Event::OpenModal) => {
                        self.app_state.render_modal = true;
                    }
                    _ => {}
                }
            }
            RenderImage::None => {
                // Otherwise create one by notifying the render component
                let [w, h] = ui.wh_of(self.ids.drawing_canvas).unwrap();
                self.sender
                    .send(Lang::UIEvent(UIEvent::RendererRequested(
                        renderer_id,
                        (
                            self.app_state.monitor_resolution.0,
                            self.app_state.monitor_resolution.1,
                        ),
                        (w as u32, h as u32),
                        RendererType::Renderer3D,
                    )))
                    .expect("Error contacting renderer backend");
                self.app_state.render_image = RenderImage::Requested;
            }
            RenderImage::Requested => {}
        }

        if self.app_state.render_modal {
            use super::modal;
            use super::param_box;

            match modal::Modal::canvas()
                .wh_of(self.ids.drawing_canvas)
                .middle_of(self.ids.drawing_canvas)
                .graphics_for(self.ids.drawing_canvas)
                .set(self.ids.render_modal, ui)
            {
                modal::Event::ChildEvent((_, id)) => {
                    for ev in param_box::ParamBox::new(
                        &mut self.app_state.render_params,
                        &renderer_id,
                        &self.app_state.language,
                    )
                    .parent(id)
                    .w_of(id)
                    .mid_top()
                    .icon_font(self.fonts.icon_font)
                    .set(self.ids.render_params, ui)
                    {
                        if let param_box::Event::ChangeParameter(lang) = ev {
                            self.sender.send(lang).unwrap()
                        }
                    }
                }
                modal::Event::Hide => {
                    self.app_state.render_modal = false;
                }
            }
        }
    }

    fn parameter_section(&mut self, ui: &mut UiCell) {
        use super::param_box::*;

        let lang = &self.app_state.language;
        let graphs = &mut self.app_state.graphs;

        if let Some((description, resource)) = graphs.active_parameters(
            self.app_state.active_node_element,
            self.app_state.active_layer_element,
        ) {
            for ev in ParamBox::new(description, resource, lang)
                .parent(self.ids.parameter_canvas)
                .w_of(self.ids.parameter_canvas)
                .mid_top()
                .icon_font(self.fonts.icon_font)
                .set(self.ids.node_param_box, ui)
            {
                let resp = match ev {
                    Event::ChangeParameter(event) => event,
                    Event::ExposeParameter(field, name, control) => Lang::UserGraphEvent({
                        let p_res = resource.clone().node_parameter(&field);
                        UserGraphEvent::ExposeParameter(p_res, field, name, control)
                    }),
                    Event::ConcealParameter(field) => Lang::UserGraphEvent(
                        UserGraphEvent::ConcealParameter(resource.clone().node_graph(), field),
                    ),
                };

                self.sender.send(resp).unwrap();
            }
        }
    }

    fn graph_section(&mut self, ui: &mut UiCell) {
        use super::exposed_param_row;
        use super::param_box;

        let active_graph = self.app_state.graphs.get_active().clone();

        let mut offset = 0.0;

        if self
            .app_state
            .graphs
            .get_active_collection_mut()
            .as_layers_mut()
            .is_some()
        {
            offset = 32.0;

            for _click in widget::Button::new()
                .label(&self.label_text("convert-to-graph"))
                .label_font_size(10)
                .parent(self.ids.graph_settings_canvas)
                .padded_w_of(self.ids.graph_settings_canvas, 16.0)
                .h(16.0)
                .mid_top_with_margin(16.0)
                .set(self.ids.layer_convert, ui)
            {
                self.sender
                    .send(Lang::UserLayersEvent(UserLayersEvent::Convert(
                        active_graph.clone(),
                    )))
                    .unwrap();
            }
        }

        for ev in param_box::ParamBox::new(
            self.app_state.graphs.get_collection_parameters_mut(),
            &active_graph,
            &self.app_state.language,
        )
        .parent(self.ids.graph_settings_canvas)
        .w_of(self.ids.graph_settings_canvas)
        .mid_top_with_margin(32.0)
        .set(self.ids.graph_param_box, ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new(&self.label_text("exposed-parameters"))
            .parent(self.ids.graph_settings_canvas)
            .color(color::WHITE)
            .font_size(12)
            .mid_top_with_margin(96.0 + offset)
            .set(self.ids.exposed_param_title, ui);

        let exposed_params = self.app_state.graphs.get_exposed_parameters_mut();

        let (mut rows, scrollbar) = widget::List::flow_down(exposed_params.len())
            .parent(self.ids.graph_settings_canvas)
            .item_size(160.0)
            .padded_w_of(self.ids.graph_settings_canvas, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0 + offset)
            .scrollbar_on_top()
            .set(self.ids.exposed_param_list, ui);

        while let Some(row) = rows.next(ui) {
            let widget = exposed_param_row::ExposedParamRow::new(
                &mut exposed_params[row.i].1,
                &self.app_state.language,
            )
            .icon_font(self.fonts.icon_font);

            if let Some(ev) = row.set(widget, ui) {
                match ev {
                    exposed_param_row::Event::ConcealParameter => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::ConcealParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                            )))
                            .unwrap();
                    }
                    exposed_param_row::Event::UpdateTitle => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::RetitleParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                                exposed_params[row.i].1.title.to_owned(),
                            )))
                            .unwrap();
                    }
                    exposed_param_row::Event::UpdateField => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::RefieldParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                                exposed_params[row.i].1.graph_field.to_owned(),
                            )))
                            .unwrap();
                    }
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }

    fn surface_section(&mut self, ui: &mut UiCell) {
        use super::{export_row, param_box, util::*};

        for ev in param_box::ParamBox::new(
            &mut self.app_state.surface_params,
            &(),
            &self.app_state.language,
        )
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
                &self.app_state.language,
            );
            let mut updated_spec = false;
            match row.set(widget, ui) {
                Some(export_row::Event::ChangeToRGB) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .alpha(false);
                    updated_spec = true;
                }
                Some(export_row::Event::ChangeToRGBA) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .alpha(true);
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
                    self.app_state.export_entries[row.i].1.set_r(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelG(spec)) => {
                    self.app_state.export_entries[row.i].1.set_g(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelB(spec)) => {
                    self.app_state.export_entries[row.i].1.set_b(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelA(spec)) => {
                    self.app_state.export_entries[row.i].1.set_a(spec);
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
}
