use conrod_core::widget::triangles::{ColoredPoint, Triangle};
use conrod_core::*;
use palette::*;

#[derive(Copy, Clone, Debug, WidgetCommon)]
pub struct ColorPicker {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    color: [f32; 3],
}

impl ColorPicker {
    pub fn new(color: [f32; 3]) -> Self {
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
        triangles,
        red,
        green,
        blue,
        alpha
    }
}

#[derive(Debug)]
pub struct State {
    ids: Ids,
}

impl Widget for ColorPicker {
    type State = State;
    type Style = Style;
    type Event = Option<[f32; 3]>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        Self::Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut new_color = None;
        let xy = args.ui.xy_of(args.id).unwrap();
        let wh = args.ui.wh_of(args.id).unwrap();

        let hue = palette::Hsv::from(palette::LinSrgb::new(
            self.color[0],
            self.color[1],
            self.color[2],
        ))
        .hue;

        let bar_tris = color_strip(6, 24.0, wh[1] - 32.0, |x| {
            color::hsl(x as f32 * std::f32::consts::TAU, 1.0, 0.5).to_rgb()
        });
        let rect_tris = color_rect(4, wh[0] - 32.0, wh[1] - 32.0, |x, y| {
            let hsv = palette::Hsv::new::<f32>(hue.into(), x as f32, y as f32);
            let rgb = palette::LinSrgb::from(hsv);
            color::Rgba(rgb.red, rgb.green, rgb.blue, 1.0)
        });

        let triangles = bar_tris
            .iter()
            .map(|t| t.add([xy[0] + wh[0] / 2.0 - 12.0, xy[1]]))
            .chain(rect_tris.iter().map(|t| t.add([xy[0] - 16.0, xy[1]])));

        widget::Triangles::multi_color(triangles)
            .with_bounding_rect(args.rect)
            .parent(args.id)
            .w_h(wh[0] / 4.0, 16.0)
            .middle()
            .set(args.state.ids.triangles, args.ui);

        for red in widget::NumberDialer::new(self.color[0], 0.0, 1.0, 4)
            .parent(args.id)
            .bottom_left()
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.red, args.ui)
        {
            new_color = new_color.or(Some(self.color)).map(|c| [red, c[1], c[2]]);
        }

        for green in widget::NumberDialer::new(self.color[1], 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.green, args.ui)
        {
            new_color = new_color.or(Some(self.color)).map(|c| [c[0], green, c[2]]);
        }

        for blue in widget::NumberDialer::new(self.color[2], 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.blue, args.ui)
        {
            new_color = new_color.or(Some(self.color)).map(|c| [c[0], c[1], blue]);
        }

        new_color
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
    let right = width / 2.0;

    for _ in 0..(2 as u16).pow(k as _) {
        // Current color
        let c = color(x);

        // Sample next color
        let xn = x + step;
        let cn = color(xn);

        // X coordinates scaled for width
        if width > height {
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
        } else {
            let yw = bottom + x * height;
            let ynw = bottom + xn * height;

            tris.push(Triangle([
                ([left, yw], c),
                ([left, ynw], cn),
                ([right, ynw], cn),
            ]));
            tris.push(Triangle([
                ([left, yw], c),
                ([right, ynw], cn),
                ([right, yw], c),
            ]));
        }

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

            tris.push(Triangle([([l, b], c_bl), ([l, t], c_tl), ([r, t], c_tr)]));
            tris.push(Triangle([([l, b], c_bl), ([r, t], c_tr), ([r, b], c_br)]));
            x += step;
        }
        y += step;
    }

    tris
}
