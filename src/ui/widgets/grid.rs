use conrod_core::*;

#[derive(WidgetCommon)]
pub struct Grid {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    zoom: Scalar,
    pan: Point,
}

impl Grid {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            zoom: 1.0,
            pan: [0., 0.],
        }
    }

    builder_methods! {
        pub zoom { zoom = Scalar }
        pub pan { pan = Point }
        pub minor_ticks { style.minor_ticks = Some(u32) }
        pub major_ticks { style.major_ticks = Some(u32) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "32")]
    minor_ticks: Option<u32>,
    #[conrod(default = "128")]
    major_ticks: Option<u32>,
}

widget_ids! {
    pub struct Ids {
        triangles
    }
}

pub struct State {
    area: Rect,
    zoom: Scalar,
    pan: Point,
    tris: Vec<widget::triangles::Triangle<Point>>,
    ids: Ids,
}

impl Widget for Grid {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            area: Rect::from_corners([0., 0.], [100., 100.]),
            tris: vec![],
            zoom: 1.0,
            pan: [0., 0.],
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            rect,
            id,
            ui,
            state,
            style,
            ..
        } = args;

        // Update triangles if required
        if state.area != rect || state.zoom != self.zoom || state.pan != self.pan {
            state.update(|state| {
                state.area = rect;
                state.zoom = self.zoom;
                state.pan = self.pan;
                state.tris =
                    build_triangles_for(state.area, state.zoom, state.pan, &style, &ui.theme)
            });
        }

        widget::Triangles::single_color(color::WHITE.alpha(0.02), state.tris.iter().copied())
            .calc_bounding_rect()
            .parent(id)
            .middle()
            .graphics_for(id)
            .set(state.ids.triangles, ui);
    }
}

fn build_triangles_for(
    area: Rect,
    zoom: f64,
    pan: Point,
    style: &Style,
    theme: &Theme,
) -> Vec<widget::triangles::Triangle<Point>> {
    use widget::line::triangles;

    let mut tris = Vec::new();

    let minor_ticks = style.minor_ticks(theme) as f64;
    let major_ticks = style.major_ticks(theme) as f64;

    let wh = area.w_h();
    let bl = area.bottom_left();

    // Vertical gridlines
    let vertical_count = (wh.0 / minor_ticks) as i32;
    for i in 0..vertical_count {
        let x = ((-pan[0] / minor_ticks).ceil() + i as f64) * minor_ticks;
        let thickness = if x % major_ticks == 0. { 2. } else { 0.25 };
        let line = triangles(
            [x + bl[0] + pan[0], 0. + bl[1]],
            [x + bl[0] + pan[0], wh.1 + bl[1]],
            thickness,
        );
        tris.push(line[0]);
        tris.push(line[1]);
    }

    // Horizontal gridlines
    let horizontal_count = (wh.1 / minor_ticks) as i32;
    for i in 0..horizontal_count {
        let y = ((-pan[1] / minor_ticks).ceil() + i as f64) * minor_ticks;
        let thickness = if y % major_ticks == 0. { 2. } else { 0.25 };
        let line = triangles(
            [0. + bl[0], y + bl[1] + pan[1]],
            [wh.0 + bl[0], y + bl[1] + pan[1]],
            thickness,
        );
        tris.push(line[0]);
        tris.push(line[1]);
    }

    tris
}
