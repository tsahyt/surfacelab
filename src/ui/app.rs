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

        // Main Views
        node_graph,
        render_view,
        add_modal,
        render_modal,

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
                        self.app_state.render_image = Some(id);
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
                self.app_state.graphs.rename_graph(from, to);
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
                self.app_state.graphs.add_node(super::graph::NodeData::new(
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

    pub fn update_gui(&mut self, ui: &mut UiCell) {
        use super::tabs;

        widget::Canvas::new()
            .border(0.0)
            .color(PANEL_COLOR)
            .flow_down(&[
                (
                    self.ids.top_bar_canvas,
                    widget::Canvas::new()
                        .length(48.0)
                        .border(PANEL_GAP)
                        .color(color::CHARCOAL),
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
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP),
                            ),
                            (
                                self.ids.drawing_canvas,
                                widget::Canvas::new().color(PANEL_COLOR).border(PANEL_GAP),
                            ),
                            (
                                self.ids.sidebar_canvas,
                                widget::Canvas::new()
                                    .length_weight(0.4)
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP),
                            ),
                        ]),
                ),
            ])
            .set(self.ids.window_canvas, ui);

        tabs::Tabs::new(&[
            (self.ids.parameter_canvas, "Parameters"),
            (self.ids.graph_settings_canvas, "Graph"),
            (self.ids.surface_settings_canvas, "Surface"),
        ])
        .color(PANEL_COLOR)
        .label_color(color::WHITE)
        .label_font_size(10)
        .parent(self.ids.sidebar_canvas)
        .wh_of(self.ids.sidebar_canvas)
        .middle()
        .set(self.ids.sidebar_tabs, ui);

        self.top_bar(ui);
        self.node_graph(ui);
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
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.open_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new("Select a surface file")
                .title("Open Surface")
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
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.save_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new("Select a surface file")
                .title("Save Surface")
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
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.export_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new("Select a base name")
                .title("Export Surface")
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
            if let Some(graph) = self.app_state.graphs.get_graph_resource(selection).cloned() {
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
            .wh([32., 32.0])
            .left(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.graph_add, ui)
        {
            self.sender
                .send(Lang::UserGraphEvent(UserGraphEvent::AddGraph))
                .unwrap()
        }
    }

    fn node_graph(&mut self, ui: &mut UiCell) {
        use super::graph;
        for event in graph::Graph::new(&self.app_state.graphs.get_active_graph().graph)
            .parent(self.ids.edit_canvas)
            .wh_of(self.ids.edit_canvas)
            .middle()
            .set(self.ids.node_graph, ui)
        {
            match event {
                graph::Event::NodeDrag(idx, x, y) => {
                    let mut node = self
                        .app_state
                        .graphs
                        .get_active_graph_mut()
                        .graph
                        .node_weight_mut(idx)
                        .unwrap();
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
                    let from_res = self
                        .app_state
                        .graphs
                        .get_active_graph()
                        .graph
                        .node_weight(from)
                        .unwrap()
                        .resource
                        .node_socket(&from_socket);
                    let to_res = self
                        .app_state
                        .graphs
                        .get_active_graph()
                        .graph
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
                            self.app_state
                                .graphs
                                .get_active_graph()
                                .graph
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .clone(),
                        )))
                        .unwrap();
                }
                graph::Event::SocketClear(idx, socket) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                            self.app_state
                                .graphs
                                .get_active_graph()
                                .graph
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .node_socket(&socket),
                        )))
                        .unwrap();
                }
                graph::Event::ActiveElement(idx) => {
                    self.app_state.active_element = Some(idx);
                }
                graph::Event::AddModal(pt) => {
                    self.app_state.add_modal = Some(pt);
                }
            }
        }

        if let Some(insertion_pt) = self.app_state.add_modal {
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
            .set(self.ids.add_modal, ui)
            {
                modal::Event::ChildEvent(((mut items, scrollbar), _)) => {
                    while let Some(item) = items.next(ui) {
                        let i = item.i;
                        let label = operators[i].title();
                        let button = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::LIGHT_CHARCOAL);
                        for _press in item.set(button, ui) {
                            self.app_state.add_modal = None;

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
                    self.app_state.add_modal = None;
                }
            }
        }
    }

    fn render_view(&mut self, ui: &mut UiCell) {
        use super::renderview::*;

        let renderer_id = self.ids.render_view.index() as u64;

        // If there is a known render image, create a render view for it
        match self.app_state.render_image {
            Some(render_image) => {
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
            None => {
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
            }
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
                    for ev in
                        param_box::ParamBox::new(&mut self.app_state.render_params, &renderer_id)
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

        if let Some((description, resource)) = self.app_state.active_parameters() {
            for ev in ParamBox::new(description, resource)
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

        for ev in param_box::ParamBox::new(
            self.app_state.graphs.get_graph_parameters_mut(),
            &active_graph,
        )
        .parent(self.ids.graph_settings_canvas)
        .w_of(self.ids.graph_settings_canvas)
        .mid_top()
        .set(self.ids.graph_param_box, ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new("Exposed Parameters")
            .parent(self.ids.graph_settings_canvas)
            .color(color::WHITE)
            .font_size(12)
            .mid_top_with_margin(96.0)
            .set(self.ids.exposed_param_title, ui);

        let exposed_params = self.app_state.graphs.get_exposed_parameters_mut();

        let (mut rows, scrollbar) = widget::List::flow_down(exposed_params.len())
            .parent(self.ids.graph_settings_canvas)
            .item_size(160.0)
            .padded_w_of(self.ids.graph_settings_canvas, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0)
            .scrollbar_on_top()
            .set(self.ids.exposed_param_list, ui);

        while let Some(row) = rows.next(ui) {
            let widget = exposed_param_row::ExposedParamRow::new(&mut exposed_params[row.i].1)
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

        for ev in param_box::ParamBox::new(&mut self.app_state.surface_params, &())
            .parent(self.ids.surface_settings_canvas)
            .w_of(self.ids.surface_settings_canvas)
            .mid_top()
            .set(self.ids.surface_param_box, ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new("Export Specification")
            .parent(self.ids.surface_settings_canvas)
            .mid_top_with_margin(96.0)
            .color(color::WHITE)
            .font_size(12)
            .set(self.ids.export_label, ui);

        for _ev in icon_button(IconName::PLUS, self.fonts.icon_font)
            .parent(self.ids.surface_settings_canvas)
            .top_right_with_margins(96.0, 16.0)
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
