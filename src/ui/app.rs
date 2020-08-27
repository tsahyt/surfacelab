use crate::{broker::BrokerSender, lang::*};
use conrod_core::*;

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
    pub render_image: Option<image::Id>,

    pub broker_sender: BrokerSender<Lang>,
    pub monitor_resolution: (u32, u32),
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
}

pub fn node_graph(ui: &mut UiCell, ids: &Ids, fonts: &AppFonts, app: &mut App) {
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
