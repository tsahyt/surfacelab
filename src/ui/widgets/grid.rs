use conrod_core::*;

const DEFAULT_RESOLUTION: u32 = 32;

#[derive(WidgetCommon)]
pub struct Grid {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    resolution: u32,
}

impl Grid {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            resolution: DEFAULT_RESOLUTION,
        }
    }

    builder_methods! {
        pub resolution { resolution = u32 }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        triangles
    }
}

pub struct State {
    area: Rect,
    resolution: u32,
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
            resolution: DEFAULT_RESOLUTION,
            tris: vec![],
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
        if state.area != rect || state.resolution != self.resolution {
            state.update(|state| {
                state.area = rect;
                state.resolution = self.resolution;
                state.tris = build_triangles_for(state.area, state.resolution)
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

fn build_triangles_for(area: Rect, resolution: u32) -> Vec<widget::triangles::Triangle<Point>> {
    use widget::line::triangles;

    let mut tris = Vec::new();
    let xy = area.bottom_left();
    let width = area.w();
    let height = area.h();

    for x in (0..width as u32).step_by(resolution as usize) {
        let thickness = if x % (resolution * 8) == 0 { 2.0 } else { 0.5 };
        let line = triangles([x as f64, 0.], [x as f64, height], thickness);
        tris.push(line[0].add(xy));
        tris.push(line[1].add(xy));
    }

    for y in (0..height as u32).step_by(resolution as usize) {
        let thickness = if y % (resolution * 8) == 0 { 2.0 } else { 0.5 };
        let line = triangles([0., y as f64], [width, y as f64], thickness);
        tris.push(line[0].add(xy));
        tris.push(line[1].add(xy));
    }

    tris
}
