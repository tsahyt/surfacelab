use crate::{broker::BrokerSender, lang::*};
use conrod_core::*;
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

        title_text,
        node_graph,
        render_view,

        add_modal_canvas,
        operator_list,

        param_box
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

    pub fn active_parameters(&mut self) -> Option<(&mut ParamBoxDescription<Field>, &Resource)> {
        let ae = self.active_element?;
        let node = self.graph.node_weight_mut(ae)?;
        Some((&mut node.param_box, &node.resource))
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
                    .length(32.0)
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

    widget::Text::new("SurfaceLab")
        .parent(ids.top_bar_canvas)
        .middle()
        .font_size(12)
        .color(color::WHITE)
        .set(ids.title_text, ui);

    node_graph(ui, ids, fonts, app);
    render_view(ui, ids, app);
    parameter_section(ui, ids, fonts, app);
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
                .color(conrod_core::color::LIGHT_BLUE);
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
    }
}

pub fn handle_graph_event(event: &GraphEvent, app: &mut App) {
    match event {
        GraphEvent::GraphAdded(_) => {}
        GraphEvent::NodeAdded(res, op, pbox, position, _size) => {
            let idx = app.graph.add_node(super::graph::NodeData::new(
                res.clone(),
                position.map(|(x, y)| [x, y]),
                op.clone(),
                pbox.clone(),
            ));
            app.graph_resources.insert(res.clone(), idx);
        }
        GraphEvent::NodeRemoved(res) => {
            if let Some(idx) = app.graph_resources.get(res) {
                app.graph.remove_node(*idx);
            }
            app.graph_resources.remove(res);
        }
        GraphEvent::NodeRenamed(from, to) => {
            if let Some(idx) = app.graph_resources.get(from).copied() {
                let node = app.graph.node_weight_mut(idx).unwrap();
                node.resource = to.clone();
                app.graph_resources.insert(to.clone(), idx);
                app.graph_resources.remove(from);
            }
        }
        GraphEvent::ConnectedSockets(from, to) => {
            let from_idx = app.graph_resources.get(&from.drop_fragment()).unwrap();
            let to_idx = app.graph_resources.get(&to.drop_fragment()).unwrap();
            app.graph.add_edge(
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

            let from_idx = app.graph_resources.get(&from.drop_fragment()).unwrap();
            let to_idx = app.graph_resources.get(&to.drop_fragment()).unwrap();

            // Assuming that there's only ever one edge connecting two sockets.
            if let Some(e) = app
                .graph
                .edges_connecting(*from_idx, *to_idx)
                .filter(|e| {
                    (e.weight().0.as_str(), e.weight().1.as_str())
                        == (from.fragment().unwrap(), to.fragment().unwrap())
                })
                .map(|e| e.id())
                .next()
            {
                app.graph.remove_edge(e);
            }
        }
        GraphEvent::SocketMonomorphized(socket, ty) => {
            let idx = app.graph_resources.get(&socket.drop_fragment()).unwrap();
            let node = app.graph.node_weight_mut(*idx).unwrap();
            let var = node
                .operator
                .type_variable_from_socket(socket.fragment().unwrap())
                .unwrap();
            node.set_type_variable(var, Some(*ty))
        }
        GraphEvent::SocketDemonomorphized(socket) => {
            let idx = app.graph_resources.get(&socket.drop_fragment()).unwrap();
            let node = app.graph.node_weight_mut(*idx).unwrap();
            let var = node
                .operator
                .type_variable_from_socket(socket.fragment().unwrap())
                .unwrap();
            node.set_type_variable(var, None)
        }
        GraphEvent::Report(nodes, edges) => {
            for (res, op, pbox, pos) in nodes {
                let idx = app.graph.add_node(super::graph::NodeData::new(
                    res.clone(),
                    Some([pos.0, pos.1]),
                    op.clone(),
                    pbox.clone(),
                ));
                app.graph_resources.insert(res.clone(), idx);
            }

            for (source, sink) in edges {
                let source_idx = app.graph_resources.get(&source.drop_fragment()).unwrap();
                let sink_idx = app.graph_resources.get(&sink.drop_fragment()).unwrap();
                app.graph.add_edge(
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
            app.graph.clear();
        }
        _ => {}
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

    if let Some((description, resource)) = app.active_parameters() {
        for Event::ChangeParameter(change) in ParamBox::new(description, resource)
            .parent(ids.parameter_canvas)
            .wh_of(ids.parameter_canvas)
            .middle()
            .set(ids.param_box, ui)
        {
            app.broker_sender.send(change).unwrap();
        }
    }
}
