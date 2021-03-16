//! A simple widget drawing a bezier curve

use conrod_core::*;
use widget::primitive::line::Pattern;

/// A simple widget drawing a bezier curve
#[derive(Debug, WidgetCommon)]
pub struct Bezier {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    start: Point,
    control_1: Point,
    end: Point,
    control_2: Point,
    resolution: usize,
}

impl Bezier {
    pub fn new(start: Point, control_1: Point, end: Point, control_2: Point) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            start,
            control_1,
            end,
            control_2,
            resolution: 4,
        }
    }

    builder_methods! {
        pub pattern { style.pattern = Some(Pattern) }
        pub thickness { style.thickness = Some(Scalar) }
        pub color { style.color = Some(Color) }
    }
}

widget_ids! {
    pub struct Ids {
        point_path
    }
}

pub struct State {
    ids: Ids,
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "Pattern::Solid")]
    pattern: Option<Pattern>,
    #[conrod(default = "1.0")]
    thickness: Option<Scalar>,
    #[conrod(default = "color::BLACK")]
    color: Option<Color>,
}

impl Widget for Bezier {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            ..
        } = args;

        let num_points = 2_u32.pow(self.resolution as u32);
        let bezier_points = (0..num_points + 1).map(|r| {
            let t = r as f64 / num_points as f64;
            let x = (1. - t).powi(3) * self.start[0]
                + 3. * (1. - t).powi(2) * t * self.control_1[0]
                + 3. * (1. - t) * t.powi(2) * self.control_2[0]
                + t.powi(3) * self.end[0];
            let y = (1. - t).powi(3) * self.start[1]
                + 3. * (1. - t).powi(2) * t * self.control_1[1]
                + 3. * (1. - t) * t.powi(2) * self.control_2[1]
                + t.powi(3) * self.end[1];
            [x, y]
        });

        widget::PointPath::abs_styled(
            bezier_points,
            widget::primitive::line::Style {
                maybe_pattern: Some(style.pattern(&ui.theme)),
                maybe_color: Some(style.color(&ui.theme)),
                maybe_thickness: Some(style.thickness(&ui.theme)),
                maybe_cap: None,
            },
        )
        .parent(id)
        .middle()
        .graphics_for(id)
        .set(state.ids.point_path, ui);
    }
}
