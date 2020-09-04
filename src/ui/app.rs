use crate::{broker::BrokerSender, lang::*};
use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};
use std::collections::HashMap;

const PANEL_COLOR: Color = color::DARK_CHARCOAL;
const PANEL_GAP: Scalar = 0.5;

widget_ids!(
    pub struct Ids {
        window_canvas,
        top_bar_canvas,
        main_canvas,
        node_graph_canvas,
        drawing_canvas,
        parameter_canvas,

        new_surface,
        open_surface,
        save_surface,

        node_graph,
        render_view,

        add_modal_canvas,
        operator_list,

        operator_param_box,
        node_param_box,
    }
);

pub struct App {
    pub graph_resources: HashMap<Resource, petgraph::graph::NodeIndex>,
    pub graph: super::graph::NodeGraph,
    pub active_element: Option<petgraph::graph::NodeIndex>,
    pub render_image: Option<image::Id>,

    pub broker_sender: BrokerSender<Lang>,
    pub monitor_resolution: (u32, u32),

    pub add_modal: bool,
}

impl App {
    pub fn new(sender: BrokerSender<Lang>, monitor_size: (u32, u32)) -> Self {
        Self {
            graph: petgraph::Graph::new(),
            graph_resources: HashMap::new(),
            active_element: None,
            render_image: None,
            broker_sender: sender,
            monitor_resolution: (monitor_size.0, monitor_size.1),
            add_modal: false,
        }
    }

    pub fn active_parameters(
        &mut self,
    ) -> Option<(
        &mut ParamBoxDescription<Field>,
        &mut ParamBoxDescription<ResourceField>,
        &Resource,
    )> {
        let ae = self.active_element?;
        let node = self.graph.node_weight_mut(ae)?;
        Some((
            &mut node.operator_param_box,
            &mut node.node_param_box,
            &node.resource,
        ))
    }

    pub fn handle_graph_event(&mut self, event: &GraphEvent) {
        match event {
            GraphEvent::GraphAdded(_) => {}
            GraphEvent::NodeAdded(res, op, pbox, position, _size) => {
                let idx = self.graph.add_node(super::graph::NodeData::new(
                    res.clone(),
                    position.map(|(x, y)| [x, y]),
                    &op,
                    pbox.clone(),
                ));
                self.graph_resources.insert(res.clone(), idx);
            }
            GraphEvent::NodeRemoved(res) => {
                if let Some(idx) = self.graph_resources.get(res) {
                    self.graph.remove_node(*idx);
                }
                self.graph_resources.remove(res);
            }
            GraphEvent::NodeRenamed(from, to) => {
                if let Some(idx) = self.graph_resources.get(from).copied() {
                    let node = self.graph.node_weight_mut(idx).unwrap();
                    node.resource = to.clone();
                    self.graph_resources.insert(to.clone(), idx);
                    self.graph_resources.remove(from);
                }
            }
            GraphEvent::ConnectedSockets(from, to) => {
                let from_idx = self.graph_resources.get(&from.drop_fragment()).unwrap();
                let to_idx = self.graph_resources.get(&to.drop_fragment()).unwrap();
                self.graph.add_edge(
                    *from_idx,
                    *to_idx,
                    (
                        from.fragment().unwrap().to_string(),
                        to.fragment().unwrap().to_string(),
                    ),
                );
            }
            GraphEvent::DisconnectedSockets(from, to) => {
                use petgraph::visit::EdgeRef;

                let from_idx = self.graph_resources.get(&from.drop_fragment()).unwrap();
                let to_idx = self.graph_resources.get(&to.drop_fragment()).unwrap();

                // Assuming that there's only ever one edge connecting two sockets.
                if let Some(e) = self
                    .graph
                    .edges_connecting(*from_idx, *to_idx)
                    .filter(|e| {
                        (e.weight().0.as_str(), e.weight().1.as_str())
                            == (from.fragment().unwrap(), to.fragment().unwrap())
                    })
                    .map(|e| e.id())
                    .next()
                {
                    self.graph.remove_edge(e);
                }
            }
            GraphEvent::SocketMonomorphized(socket, ty) => {
                let idx = self.graph_resources.get(&socket.drop_fragment()).unwrap();
                let node = self.graph.node_weight_mut(*idx).unwrap();
                let var = type_variable_from_socket_iter(
                    node.inputs.iter().chain(node.outputs.iter()),
                    socket.fragment().unwrap(),
                )
                .unwrap();
                node.set_type_variable(var, Some(*ty))
            }
            GraphEvent::SocketDemonomorphized(socket) => {
                let idx = self.graph_resources.get(&socket.drop_fragment()).unwrap();
                let node = self.graph.node_weight_mut(*idx).unwrap();
                let var = type_variable_from_socket_iter(
                    node.inputs.iter().chain(node.outputs.iter()),
                    socket.fragment().unwrap(),
                )
                .unwrap();
                node.set_type_variable(var, None)
            }
            GraphEvent::Report(nodes, edges) => {
                for (res, op, pbox, pos) in nodes {
                    let idx = self.graph.add_node(super::graph::NodeData::new(
                        res.clone(),
                        Some([pos.0, pos.1]),
                        &op,
                        pbox.clone(),
                    ));
                    self.graph_resources.insert(res.clone(), idx);
                }

                for (source, sink) in edges {
                    let source_idx = self.graph_resources.get(&source.drop_fragment()).unwrap();
                    let sink_idx = self.graph_resources.get(&sink.drop_fragment()).unwrap();
                    self.graph.add_edge(
                        *source_idx,
                        *sink_idx,
                        (
                            source.fragment().unwrap().to_string(),
                            sink.fragment().unwrap().to_string(),
                        ),
                    );
                }
            }
            GraphEvent::Cleared => {
                self.graph.clear();
            }
            _ => {}
        }
    }

    pub fn register_thumbnail(&mut self, resource: &Resource, thumbnail: image::Id) {
        if let Some(idx) = self.graph_resources.get(resource) {
            let node = self.graph.node_weight_mut(*idx).unwrap();
            node.thumbnail = Some(thumbnail);
        }
    }
}

pub struct AppFonts {
    pub text_font: text::font::Id,
    pub icon_font: text::font::Id,
}

pub fn gui(ui: &mut UiCell, ids: &Ids, fonts: &AppFonts, app: &mut App) {
    widget::Canvas::new()
        .border(0.0)
        .color(PANEL_COLOR)
        .flow_down(&[
            (
                ids.top_bar_canvas,
                widget::Canvas::new()
                    .length(48.0)
                    .border(PANEL_GAP)
                    .color(color::CHARCOAL),
            ),
            (
                ids.main_canvas,
                widget::Canvas::new()
                    .border(PANEL_GAP)
                    .color(PANEL_COLOR)
                    .flow_right(&[
                        (
                            ids.node_graph_canvas,
                            widget::Canvas::new()
                                .scroll_kids()
                                .color(PANEL_COLOR)
                                .border(PANEL_GAP),
                        ),
                        (
                            ids.drawing_canvas,
                            widget::Canvas::new().color(PANEL_COLOR).border(PANEL_GAP),
                        ),
                        (
                            ids.parameter_canvas,
                            widget::Canvas::new()
                                .length_weight(0.4)
                                .scroll_kids_vertically()
                                .color(PANEL_COLOR)
                                .border(PANEL_GAP),
                        ),
                    ]),
            ),
        ])
        .set(ids.window_canvas, ui);

    top_bar(ui, ids, fonts, app);
    node_graph(ui, ids, fonts, app);
    render_view(ui, ids, app);
    parameter_section(ui, ids, fonts, app);
}

pub fn top_bar(ui: &mut UiCell, ids: &Ids, fonts: &AppFonts, app: &mut App) {
    use super::util::*;

    for _press in icon_button(IconName::FOLDER_PLUS, fonts)
        .label_font_size(14)
        .label_color(color::WHITE)
        .color(color::DARK_CHARCOAL)
        .wh([32., 32.0])
        .mid_left_with_margin(8.0)
        .parent(ids.top_bar_canvas)
        .set(ids.new_surface, ui)
    {
        app.broker_sender
            .send(Lang::UserIOEvent(UserIOEvent::NewSurface))
            .unwrap();
    }

    for _press in icon_button(IconName::FOLDER_OPEN, fonts)
        .label_font_size(14)
        .label_color(color::WHITE)
        .color(color::DARK_CHARCOAL)
        .wh([32., 32.0])
        .right(8.0)
        .parent(ids.top_bar_canvas)
        .set(ids.open_surface, ui)
    {
        match FileSelection::new("Select a surface file")
            .title("Open Surface")
            .mode(FileSelectionMode::Open)
            .show()
        {
            Ok(Some(path)) => {
                app.broker_sender
                    .send(Lang::UserIOEvent(UserIOEvent::OpenSurface(
                        std::path::PathBuf::from(path),
                    )))
                    .unwrap();
            }
            _ => {}
        }
    }

    for _press in icon_button(IconName::CONTENT_SAVE, fonts)
        .label_font_size(14)
        .label_color(color::WHITE)
        .color(color::DARK_CHARCOAL)
        .wh([32., 32.0])
        .right(8.0)
        .parent(ids.top_bar_canvas)
        .set(ids.save_surface, ui)
    {
        match FileSelection::new("Select a surface file")
            .title("Save Surface")
            .mode(FileSelectionMode::Save)
            .show()
        {
            Ok(Some(path)) => {
                app.broker_sender
                    .send(Lang::UserIOEvent(UserIOEvent::SaveSurface(
                        std::path::PathBuf::from(path),
                    )))
                    .unwrap();
            }
            _ => {}
        }
    }
}

pub fn node_graph(ui: &mut UiCell, ids: &Ids, _fonts: &AppFonts, app: &mut App) {
    use super::graph::*;
    for event in Graph::new(&app.graph)
        .parent(ids.node_graph_canvas)
        .wh_of(ids.node_graph_canvas)
        .middle()
        .set(ids.node_graph, ui)
    {
        match event {
            Event::NodeDrag(idx, x, y) => {
                let mut node = app.graph.node_weight_mut(idx).unwrap();
                node.position[0] += x;
                node.position[1] += y;

                app.broker_sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                        node.resource.clone(),
                        (node.position[0], node.position[1]),
                    )))
                    .unwrap();
            }
            Event::ConnectionDrawn(from, from_socket, to, to_socket) => {
                let from_res = app
                    .graph
                    .node_weight(from)
                    .unwrap()
                    .resource
                    .extend_fragment(&from_socket);
                let to_res = app
                    .graph
                    .node_weight(to)
                    .unwrap()
                    .resource
                    .extend_fragment(&to_socket);
                app.broker_sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(
                        from_res, to_res,
                    )))
                    .unwrap();
            }
            Event::NodeDelete(idx) => {
                app.broker_sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::RemoveNode(
                        app.graph.node_weight(idx).unwrap().resource.clone(),
                    )))
                    .unwrap();
            }
            Event::SocketClear(idx, socket) => {
                app.broker_sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                        app.graph
                            .node_weight(idx)
                            .unwrap()
                            .resource
                            .extend_fragment(&socket),
                    )))
                    .unwrap();
            }
            Event::ActiveElement(idx) => {
                app.active_element = Some(idx);
            }
            Event::AddModal => {
                app.add_modal = true;
            }
        }
    }

    if app.add_modal {
        widget::Canvas::new()
            .wh_of(ids.node_graph_canvas)
            .middle_of(ids.node_graph_canvas)
            .color(color::Color::Rgba(0., 0., 0., 0.9))
            .set(ids.add_modal_canvas, ui);

        let operators = crate::lang::AtomicOperator::all_default();
        let (mut items, scrollbar) = widget::List::flow_down(operators.len())
            .item_size(50.0)
            .scrollbar_on_top()
            .middle_of(ids.node_graph_canvas)
            .padded_wh_of(ids.node_graph_canvas, 256.0)
            .set(ids.operator_list, ui);

        while let Some(item) = items.next(ui) {
            let i = item.i;
            let label = operators[i].title();
            let toggle = widget::Button::new()
                .label(&label)
                .label_color(conrod_core::color::WHITE)
                .label_font_size(12)
                .color(conrod_core::color::LIGHT_CHARCOAL);
            for _press in item.set(toggle, ui) {
                app.add_modal = false;

                app.broker_sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::NewNode(
                        Resource::graph("base", None), // TODO: current graph in UI
                        Operator::AtomicOperator(operators[i].clone()),
                    )))
                    .unwrap();
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui)
        }

        for _press in ui
            .widget_input(ids.add_modal_canvas)
            .clicks()
            .button(input::MouseButton::Left)
        {
            app.add_modal = false;
        }
    }
}

pub fn render_view(ui: &mut UiCell, ids: &Ids, app: &mut App) {
    use super::renderview::*;

    let renderer_id = ids.render_view.index() as u64;

    // If there is a known render image, create a render view for it
    match app.render_image {
        Some(render_image) => {
            let rv = RenderView::new(render_image, app.monitor_resolution)
                .parent(ids.drawing_canvas)
                .wh_of(ids.drawing_canvas)
                .middle()
                .set(ids.render_view, ui);

            // The widget itself does not communicate with the backend. Process
            // events here
            match rv {
                Some(Event::Resized(w, h)) => app
                    .broker_sender
                    .send(Lang::UIEvent(UIEvent::RendererResize(renderer_id, w, h)))
                    .unwrap(),
                Some(Event::Rotate(x, y)) => app
                    .broker_sender
                    .send(Lang::UserRenderEvent(UserRenderEvent::Rotate(
                        renderer_id,
                        x,
                        y,
                    )))
                    .unwrap(),
                Some(Event::Pan(x, y)) => app
                    .broker_sender
                    .send(Lang::UserRenderEvent(UserRenderEvent::Pan(
                        renderer_id,
                        x,
                        y,
                    )))
                    .unwrap(),
                Some(Event::LightPan(x, y)) => app
                    .broker_sender
                    .send(Lang::UserRenderEvent(UserRenderEvent::LightMove(
                        renderer_id,
                        x,
                        y,
                    )))
                    .unwrap(),
                Some(Event::Zoom(delta)) => app
                    .broker_sender
                    .send(Lang::UserRenderEvent(UserRenderEvent::Zoom(
                        renderer_id,
                        delta,
                    )))
                    .unwrap(),
                _ => {}
            }
        }
        None => {
            // Otherwise create one by notifying the render component
            let [w, h] = ui.wh_of(ids.drawing_canvas).unwrap();
            app.broker_sender
                .send(Lang::UIEvent(UIEvent::RendererRequested(
                    renderer_id,
                    (app.monitor_resolution.0, app.monitor_resolution.1),
                    (w as u32, h as u32),
                    RendererType::Renderer3D,
                )))
                .expect("Error contacting renderer backend");
        }
    }
}

pub fn parameter_section(ui: &mut UiCell, ids: &Ids, _fonts: &AppFonts, app: &mut App) {
    use super::param_box::*;

    if let Some((op_description, node_description, resource)) = app.active_parameters() {
        let mut node_evs = ParamBox::new(node_description, resource)
            .parent(ids.parameter_canvas)
            .w_of(ids.parameter_canvas)
            .h(256.0)
            .mid_top()
            .set(ids.node_param_box, ui);
        let mut op_evs = ParamBox::new(op_description, resource)
            .parent(ids.parameter_canvas)
            .w_of(ids.parameter_canvas)
            .set(ids.operator_param_box, ui);
        for Event::ChangeParameter(event) in node_evs.drain(0..).chain(op_evs.drain(0..)) {
            app.broker_sender.send(event).unwrap();
        }
    }
}
