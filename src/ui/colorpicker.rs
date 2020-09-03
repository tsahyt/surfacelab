use conrod_core::widget::triangles::{ColoredPoint, Triangle};
use conrod_core::*;
use palette::*;

#[derive(Copy, Clone, Debug, WidgetCommon)]
pub struct ColorPicker<C> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    color: C,
}

impl<C> ColorPicker<C> {
    pub fn new(color: C) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            color,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

widget_ids! {
    #[derive(Debug)]
    pub struct Ids {
        triangles
    }
}

#[derive(Debug)]
pub struct State {
    ids: Ids,
}

impl Widget for ColorPicker<Hsv> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        Self::Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let tris = color_strip(4, 256.0, 24.0, |x| {
            color::hsl(x as f32 * std::f32::consts::TAU, 1.0, 0.5).to_rgb()
        });
        widget::Triangles::multi_color(tris)
            .with_bounding_rect(args.rect)
            .parent(args.id)
            .middle()
            .set(args.state.ids.triangles, args.ui);
    }
}

/// Produce a colored strip from the continuous function given, using k as a
/// tessellation factor. If the width is greater than the height, the color
/// field is interpreted horizontally, otherwise vertically.
///
/// The color field is sampled within the [0,1] interval.
fn color_strip<F: Fn(f64) -> color::Rgba>(
    k: u8,
    width: f64,
    height: f64,
    color: F,
) -> Vec<Triangle<ColoredPoint>> {
    let mut tris = Vec::new();
    let step = 1.0 / (k as f64).exp2();
    let mut x: f64 = 0.0;

    let top = height / 2.0;
    let bottom = -height / 2.0;
    let left = -width / 2.0;

    for _ in 1..(2 as u16).pow(k as _) {
        // Current color
        let c = color(x);

        // Sample next color
        let xn = x + step;
        let cn = color(xn);

        // X coordinates scaled for width
        let xw = left + x * width;
        let xnw = left + xn * width;

        tris.push(Triangle([
            ([xw, bottom], c),
            ([xw, top], c),
            ([xnw, top], cn),
        ]));
        tris.push(Triangle([
            ([xw, bottom], c),
            ([xnw, top], cn),
            ([xnw, bottom], cn),
        ]));

        x += step;
    }

    tris
}

/// Produce a colored rectangle from the continuous function given, using k as a
/// tessellation factor.
///
/// The color field is sampled within the [(0,0),(1,1)] interval.
fn color_rect<F: Fn(f64, f64) -> color::Rgba>(
    k: u8,
    width: f64,
    height: f64,
    color: F,
) -> Vec<Triangle<ColoredPoint>> {
    // TODO geometry construction with Z order curve
    todo!()
}
