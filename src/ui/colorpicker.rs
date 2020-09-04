use conrod_core::widget::triangles::{ColoredPoint, Triangle};
use conrod_core::*;
use palette::*;

pub use palette::Hsv;

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
        triangles,
        rgb_label,
        red,
        green,
        blue,
        hsv_label,
        hue,
        saturation,
        value,
        svdot,
        huedot,
    }
}

#[derive(Debug)]
pub struct State {
    ids: Ids,
}

impl Widget for ColorPicker<Hsv> {
    type State = State;
    type Style = Style;
    type Event = Option<Hsv>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        Self::Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut new_hsv = None;
        let xy = args.ui.xy_of(args.id).unwrap();
        let wh = args.ui.wh_of(args.id).unwrap();

        let bar_size = [24.0, wh[1] - 32.0];
        let bar_middle = [xy[0] + wh[0] / 2.0 - 12.0, xy[1]];
        let bar_tris = color_strip(6, bar_size[0], bar_size[1], |x| {
            color::hsl(x as f32 * std::f32::consts::TAU, 1.0, 0.5).to_rgb()
        });

        let rect_size = [wh[0] - 32.0, wh[1] - 32.0];
        let rect_middle = [xy[0] - 16.0, xy[1]];
        let rect_tris = color_rect(4, rect_size[0], rect_size[1], |x, y| {
            let hsv = palette::Hsv::new::<f32>(self.color.hue.into(), x as f32, y as f32);
            let rgb = palette::LinSrgb::from(hsv);
            color::Rgba(rgb.red, rgb.green, rgb.blue, 1.0)
        });

        let triangles = bar_tris
            .iter()
            .map(|t| t.add(bar_middle))
            .chain(rect_tris.iter().map(|t| t.add(rect_middle)));

        widget::Triangles::multi_color(triangles)
            .with_bounding_rect(args.rect)
            .parent(args.id)
            .w_h(wh[0] / 4.0, 16.0)
            .middle()
            .set(args.state.ids.triangles, args.ui);

        let rgb = LinSrgb::from(self.color);

        widget::Text::new("RGB")
            .parent(args.id)
            .bottom_left()
            .font_size(12)
            .color(color::WHITE)
            .w(20.0)
            .set(args.state.ids.rgb_label, args.ui);

        for red in widget::NumberDialer::new(rgb.red, 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.red, args.ui)
        {
            let mut new_rgb = rgb;
            new_rgb.red = red;
            new_hsv = Some(Hsv::from(new_rgb))
        }

        for green in widget::NumberDialer::new(rgb.green, 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.green, args.ui)
        {
            let mut new_rgb = rgb;
            new_rgb.green = green;
            new_hsv = Some(Hsv::from(new_rgb))
        }

        for blue in widget::NumberDialer::new(rgb.blue, 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.blue, args.ui)
        {
            let mut new_rgb = rgb;
            new_rgb.blue = blue;
            new_hsv = Some(Hsv::from(new_rgb))
        }

        widget::Text::new("HSV")
            .parent(args.id)
            .down_from(args.state.ids.rgb_label, 16.0)
            .font_size(12)
            .w(20.0)
            .color(color::WHITE)
            .set(args.state.ids.hsv_label, args.ui);

        for hue in widget::NumberDialer::new(self.color.hue.to_positive_radians() / std::f32::consts::TAU, 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.hue, args.ui)
        {
            let mut new_hsv_inner = self.color;
            new_hsv_inner.hue = RgbHue::from_radians(hue * std::f32::consts::TAU);
            new_hsv = Some(new_hsv_inner);
        }

        for saturation in widget::NumberDialer::new(self.color.saturation, 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.saturation, args.ui)
        {
            let mut new_hsv_inner = self.color;
            new_hsv_inner.saturation = saturation;
            new_hsv = Some(new_hsv_inner);
        }

        for value in widget::NumberDialer::new(self.color.value, 0.0, 1.0, 4)
            .parent(args.id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh[0] / 4.0, 16.0)
            .set(args.state.ids.value, args.ui)
        {
            let mut new_hsv_inner = self.color;
            new_hsv_inner.value = value;
            new_hsv = Some(new_hsv_inner);
        }

        let sv_pos = [
            rect_middle[0] - rect_size[0] / 2.0 + self.color.saturation as f64 * rect_size[0],
            rect_middle[1] - rect_size[1] / 2.0 + self.color.value as f64 * rect_size[1],
        ];

        widget::Circle::fill_with(6.0, color::WHITE)
            .xy(sv_pos)
            .set(args.state.ids.svdot, args.ui);

        for drag in args
            .ui
            .widget_input(args.state.ids.svdot)
            .drags()
            .button(input::MouseButton::Left)
        {
            let speed = if drag.modifiers == input::ModifierKey::SHIFT {
                0.01
            } else {
                1.0
            };

            let mut new_hsv_inner = self.color;
            new_hsv_inner.saturation =
                (0.5 + (sv_pos[0] + (speed * drag.to[0]) - rect_middle[0]) / rect_size[0]) as f32;
            new_hsv_inner.value =
                (0.5 + (sv_pos[1] + (speed * drag.to[1]) - rect_middle[1]) / rect_size[1]) as f32;
            new_hsv = Some(new_hsv_inner);
        }

        let hue_pos = [
            bar_middle[0],
            bar_middle[1] - bar_size[1] / 2.0
                + self.color.hue.to_positive_radians() as f64 / std::f64::consts::TAU * bar_size[1],
        ];

        widget::Circle::fill_with(6.0, color::WHITE)
            .xy(hue_pos)
            .set(args.state.ids.huedot, args.ui);

        for drag in args
            .ui
            .widget_input(args.state.ids.huedot)
            .drags()
            .button(input::MouseButton::Left)
        {
            let speed = if drag.modifiers == input::ModifierKey::SHIFT {
                0.01
            } else {
                1.0
            };

            let mut new_hsv_inner = self.color;
            new_hsv_inner.hue = RgbHue::from_radians(
                ((2.0 * (hue_pos[1] + (speed * drag.to[1]) - bar_middle[1]) * std::f64::consts::PI)
                    / bar_size[1]
                    - std::f64::consts::PI) as f32,
            );
            new_hsv = Some(new_hsv_inner);
        }

        new_hsv
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
