use conrod_core::*;

widget_ids!(
    pub struct Ids {
        window_canvas,
        top_bar_canvas,
        main_canvas,
        node_graph_canvas,
        drawing_canvas,
        parameter_canvas,

        title_text
    }
);

pub struct App {
    clicks: u32,
}

impl Default for App {
    fn default() -> Self {
        App { clicks: 0 }
    }
}

pub fn gui(ui: &mut UiCell, ids: &Ids, app: &mut App) {
    widget::Canvas::new()
        .border(0.0)
        .color(color::DARK_CHARCOAL)
        .flow_down(&[
            (
                ids.top_bar_canvas,
                widget::Canvas::new()
                    .length(32.0)
                    .border(0.5)
                    .color(color::CHARCOAL),
            ),
            (
                ids.main_canvas,
                widget::Canvas::new()
                    .border(0.0)
                    .color(color::DARK_CHARCOAL)
                    .flow_right(&[
                        (
                            ids.node_graph_canvas,
                            widget::Canvas::new()
                                .color(color::DARK_CHARCOAL)
                                .border(0.5),
                        ),
                        (
                            ids.drawing_canvas,
                            widget::Canvas::new()
                                .color(color::DARK_CHARCOAL)
                                .border(0.5),
                        ),
                        (
                            ids.parameter_canvas,
                            widget::Canvas::new()
                                .length_weight(0.4)
                                .color(color::DARK_CHARCOAL)
                                .border(0.5),
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
}
