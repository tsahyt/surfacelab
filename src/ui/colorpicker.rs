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
        let bar_tris = color_strip(6, 256.0, 24.0, |x| {
            color::hsl(x as f32 * std::f32::consts::TAU, 1.0, 0.5).to_rgb()
        });
        let rect_tris = color_rect(4, 256.0, 256.0, |x,y| {
            let hsv = palette::Hsv::new::<f32>(150.0, x as f32, y as f32);
            let rgb = palette::LinSrgb::from(hsv);
            color::Rgba(rgb.red, rgb.green, rgb.blue, 1.0)
        });
        widget::Triangles::multi_color(rect_tris)
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
    let mut tris = Vec::with_capacity((2 as usize).pow(k as u32 + 1));
    let step = 1.0 / (k as f64).exp2();
    let mut x: f64 = 0.0;

    let top = height / 2.0;
    let bottom = -height / 2.0;
    let left = -width / 2.0;

    for _ in 0..(2 as u16).pow(k as _) {
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
    let mut tris = Vec::with_capacity((4 as usize).pow(k as _) * 2);
    let step = 1.0 / (k as f64).exp2();
    let mut x: f64;
    let mut y: f64 = 0.0;

    let bottom = -height / 2.0;
    let left = -width / 2.0;

    for _ in 0..(2 as u16).pow(k as _) {
        x = 0.0;
        for _ in 0..(2 as u16).pow(k as _) {
            let c_bl = color(x, y);
            let c_br = color(x + step, y);
            let c_tl = color(x, y + step);
            let c_tr = color(x + step, y + step);

            let l = left + x * width;
            let r = left + (x + step) * width;
            let b = bottom + y * height;
            let t = bottom + (y + step) * height;

            tris.push(Triangle([
                ([l,b], c_bl),
                ([l,t], c_tl),
                ([r,t], c_tr),
            ]));
            tris.push(Triangle([
                ([l,b], c_bl),
                ([r,t], c_tr),
                ([r,b], c_br),
            ]));
            x += step;
        }
        y += step;
    }

    tris
}
