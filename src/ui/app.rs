use super::util;
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
        render_view
    }
);

pub struct App {
    pub graph: petgraph::Graph<&'static str, (usize, usize)>,
    pub graph_layout: super::graph::Layout<petgraph::graph::NodeIndex>,
    pub render_image: Option<image::Id>,

    pub broker_sender: BrokerSender<Lang>,
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
    render_view(ui, ids, fonts, app);
}

pub fn node_graph(ui: &mut UiCell, ids: &Ids, fonts: &AppFonts, app: &mut App) {
    use super::graph::*;

    let session = {
        // An identifier for each node in the graph.
        let node_indices = app.graph.node_indices();
        // Describe each edge in the graph as NodeSocket -> NodeSocket.
        let edges = app.graph.raw_edges().iter().map(|e| {
            let start = NodeSocket {
                id: e.source(),
                socket_index: e.weight.0,
            };
            let end = NodeSocket {
                id: e.target(),
                socket_index: e.weight.1,
            };
            (start, end)
        });
        Graph::new(node_indices, edges, &app.graph_layout)
            .background_color(color::TRANSPARENT)
            .wh_of(ids.node_graph_canvas)
            .middle_of(ids.node_graph_canvas)
            .set(ids.node_graph, ui)
    };

    for event in session.events() {
        match event {
            Event::Node(event) => match event {
                NodeEvent::Remove(node_id) => {}
                NodeEvent::Dragged { node_id, to, .. } => {
                    *app.graph_layout.get_mut(&node_id).unwrap() = to;
                }
            },
            Event::Edge(event) => match event {
                EdgeEvent::AddStart(node_socket) => {}
                EdgeEvent::Add { start, end } => {}
                EdgeEvent::Cancelled(node_socket) => {}
                EdgeEvent::Remove { start, end } => {}
            },
        }
    }

    // Instantiate a widget for each node within the graph.

    let mut session = session.next();
    for node in session.nodes() {
        // Each `Node` contains:
        //
        // `id` - The unique node identifier for this node.
        // `point` - The position at which this node will be set.
        // `inputs`
        // `outputs`
        //
        // Calling `node.widget(some_widget)` returns a `NodeWidget`, which contains:
        //
        // `wiget_id` - The widget identifier for the widget that will represent this node.
        let node_id = node.node_id();
        let inputs = app
            .graph
            .neighbors_directed(node_id, petgraph::Incoming)
            .count();
        let outputs = app
            .graph
            .neighbors_directed(node_id, petgraph::Outgoing)
            .count();
        let button = util::icon_button(util::IconName::CONTENT_SAVE, fonts);
        let widget = Node::new(button)
            .inputs(inputs)
            .outputs(outputs)
            .w_h(100.0, 60.0);
        for _click in node.widget(widget).set(ui).widget_event {
            println!("{} was clicked!", &app.graph[node_id]);
        }
    }

    // Instantiate a widget for each edge within the graph.

    let mut session = session.next();
    for edge in session.edges() {
        let (a, b) = node::edge_socket_rects(&edge, ui);
        let line = widget::Line::abs(a.xy(), b.xy())
            .color(conrod_core::color::LIGHT_CHARCOAL)
            .thickness(3.0);

        // Each edge contains:
        //
        // `start` - The unique node identifier for the node at the start of the edge with point.
        // `end` - The unique node identifier for the node at the end of the edge with point.
        // `widget_id` - The wiget identifier for this edge.
        edge.widget(line).set(ui);
    }
}

pub fn render_view(ui: &mut UiCell, ids: &Ids, fonts: &AppFonts, app: &mut App) {
    use super::renderview::*;

    // If there is a known render image, create a render view for it
    match app.render_image {
        Some(render_image) => {
            RenderView::new(render_image)
                .parent(ids.drawing_canvas)
                .wh_of(ids.drawing_canvas)
                .middle()
                .set(ids.render_view, ui);
        }
        None => {
            // Otherwise create one by notifying the render component
            let [w, h] = ui.wh_of(ids.drawing_canvas).unwrap();
            app.broker_sender
               .send(Lang::UIEvent(UIEvent::RendererRequested(
                   ids.render_view.index() as u64,
                   w as u32,
                   h as u32,
                   RendererType::Renderer3D,
               )))
               .expect("Error contacting renderer backend");
        }
    }
}
