use conrod_core::widget::triangles::{ColoredPoint, Triangle};
use conrod_core::*;
use palette::*;

pub use palette::Hsv;

const SLOWDOWN: f64 = 0.2;

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
    rect: Rect,
    color: LinSrgb,
    sv_triangles: Vec<Triangle<ColoredPoint>>,
    hue_triangles: Vec<Triangle<ColoredPoint>>,
}

impl Widget for ColorPicker<Hsv> {
    type State = State;
    type Style = Style;
    type Event = Option<Hsv>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
            rect: Rect::from_corners([0., 0.], [100., 100.]),
            color: LinSrgb::new(0., 0., 0.),
            sv_triangles: Vec::new(),
            hue_triangles: Vec::new(),
        }
    }

    fn style(&self) -> Self::Style {
        Self::Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            state,
            ui,
            id,
            rect,
            ..
        } = args;

        let mut new_hsv = None;
        let xy = rect.xy();
        let wh = rect.w_h();

        let control_margin = 64.0;
        let bar_size = [24.0, wh.1 - control_margin];
        let bar_middle = [xy[0] + wh.0 / 2.0 - 12.0, xy[1] + control_margin / 2.];
        let bar_rect = Rect::from_xy_dim(bar_middle, bar_size);

        let rect_size = [wh.0 - 32.0, wh.1 - control_margin];
        let rect_middle = [xy[0] - 16.0, xy[1] + control_margin / 2.];
        let rect_rect = Rect::from_xy_dim(rect_middle, rect_size);

        if rect != state.rect || LinSrgb::from(self.color) != state.color {
            state.update(|state| {
                state.hue_triangles = color_strip(6, bar_size[0], bar_size[1], |x| {
                    color::hsl(x as f32 * std::f32::consts::TAU, 1.0, 0.5).to_rgb()
                });

                state.sv_triangles = color_rect(4, rect_size[0], rect_size[1], |x, y| {
                    let hsv = palette::Hsv::new::<f32>(self.color.hue.into(), x as f32, y as f32);
                    let rgb = palette::LinSrgb::from(hsv);
                    color::Rgba(rgb.red, rgb.green, rgb.blue, 1.0)
                });

                state.rect = rect;
                state.color = LinSrgb::from(self.color);
            })
        }

        let triangles = state
            .hue_triangles
            .iter()
            .map(|t| t.add(bar_middle))
            .chain(state.sv_triangles.iter().map(|t| t.add(rect_middle)));

        widget::Triangles::multi_color(triangles)
            .with_bounding_rect(rect)
            .parent(id)
            .graphics_for(id)
            .set(state.ids.triangles, ui);

        let tri_input = ui.widget_input(id);

        let sv_pos = [
            rect_middle[0] - rect_size[0] / 2.0 + self.color.saturation as f64 * rect_size[0],
            rect_middle[1] - rect_size[1] / 2.0 + self.color.value as f64 * rect_size[1],
        ];

        let hue_pos = [
            bar_middle[0],
            bar_middle[1] - bar_size[1] / 2.0
                + self.color.hue.to_positive_radians() as f64 / std::f64::consts::TAU * bar_size[1],
        ];

        for mouse in tri_input
            .presses()
            .mouse()
            .button(input::MouseButton::Left)
            .map(|p| (p.0, false))
            .chain(
                tri_input
                    .drags()
                    .button(input::MouseButton::Left)
                    .map(|d| (d.to, d.modifiers == input::ModifierKey::SHIFT)),
            )
        {
            let pos = [mouse.0[0] + xy[0], mouse.0[1] + xy[1]];
            let slowdown = mouse.1;

            if bar_rect.is_over(pos) {
                let mut new_hsv_inner = self.color;

                let mut delta = pos[1] - hue_pos[1];
                if slowdown {
                    delta = delta.signum() * SLOWDOWN
                };

                let new_hue = (2.0 * (hue_pos[1] + delta - bar_middle[1]) * std::f64::consts::PI)
                    / bar_size[1]
                    - std::f64::consts::PI;
                new_hsv_inner.hue = RgbHue::from_radians(new_hue as f32);
                new_hsv = Some(new_hsv_inner);
            }

            if rect_rect.is_over(pos) {
                let mut new_hsv_inner = self.color;
                let mut delta = [pos[0] - sv_pos[0], pos[1] - sv_pos[1]];

                if slowdown {
                    let magnitude = delta[0].abs().max(delta[1].abs());
                    delta = [
                        SLOWDOWN * delta[0] / magnitude,
                        SLOWDOWN * delta[1] / magnitude,
                    ];
                }

                new_hsv_inner.saturation = (0.5
                    + (sv_pos[0] + delta[0] - rect_middle[0]) / rect_size[0])
                    .clamp(0.0, 1.0) as f32;
                new_hsv_inner.value = (0.5 + (sv_pos[1] + delta[1] - rect_middle[1]) / rect_size[1])
                    .clamp(0.0, 1.0) as f32;
                new_hsv = Some(new_hsv_inner);
            }
        }

        let rgb = LinSrgb::from(self.color);
        let pure_hue = LinSrgb::from(Hsv::new(self.color.hue, 1.0, 1.0));

        widget::Text::new("RGB")
            .parent(id)
            .bottom_left_with_margin(8.)
            .font_size(10)
            .color(color::WHITE)
            .w(20.0)
            .set(state.ids.rgb_label, ui);

        if let Some(red) = widget::NumberDialer::new(rgb.red, 0.0, 1.0, 4)
            .parent(id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh.0 / 4.0, 16.0)
            .set(state.ids.red, ui)
        {
            let mut new_rgb = rgb;
            new_rgb.red = red;
            new_hsv = Some(Hsv::from(new_rgb))
        }

        if let Some(green) = widget::NumberDialer::new(rgb.green, 0.0, 1.0, 4)
            .parent(id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh.0 / 4.0, 16.0)
            .set(state.ids.green, ui)
        {
            let mut new_rgb = rgb;
            new_rgb.green = green;
            new_hsv = Some(Hsv::from(new_rgb))
        }

        if let Some(blue) = widget::NumberDialer::new(rgb.blue, 0.0, 1.0, 4)
            .parent(id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh.0 / 4.0, 16.0)
            .set(state.ids.blue, ui)
        {
            let mut new_rgb = rgb;
            new_rgb.blue = blue;
            new_hsv = Some(Hsv::from(new_rgb))
        }

        widget::Text::new("HSV")
            .parent(id)
            .up_from(state.ids.rgb_label, 16.0)
            .font_size(10)
            .w(20.0)
            .color(color::WHITE)
            .set(state.ids.hsv_label, ui);

        if let Some(hue) = widget::NumberDialer::new(
            self.color.hue.to_positive_radians() / std::f32::consts::TAU,
            0.0,
            1.0,
            4,
        )
        .parent(id)
        .right(16.0)
        .label_font_size(10)
        .w_h(wh.0 / 4.0, 16.0)
        .set(state.ids.hue, ui)
        {
            let mut new_hsv_inner = self.color;
            new_hsv_inner.hue = RgbHue::from_radians(hue * std::f32::consts::TAU);
            new_hsv = Some(new_hsv_inner);
        }

        if let Some(saturation) = widget::NumberDialer::new(self.color.saturation, 0.0, 1.0, 4)
            .parent(id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh.0 / 4.0, 16.0)
            .set(state.ids.saturation, ui)
        {
            let mut new_hsv_inner = self.color;
            new_hsv_inner.saturation = saturation;
            new_hsv = Some(new_hsv_inner);
        }

        if let Some(value) = widget::NumberDialer::new(self.color.value, 0.0, 1.0, 4)
            .parent(id)
            .right(16.0)
            .label_font_size(10)
            .w_h(wh.0 / 4.0, 16.0)
            .set(state.ids.value, ui)
        {
            let mut new_hsv_inner = self.color;
            new_hsv_inner.value = value;
            new_hsv = Some(new_hsv_inner);
        }

        let sv_color = color::rgb(rgb.red, rgb.green, rgb.blue);

        widget::BorderedRectangle::new([12.0, 12.0])
            .color(sv_color)
            .border_color(sv_color.plain_contrast())
            .border(2.0)
            .graphics_for(id)
            .xy(sv_pos)
            .set(state.ids.svdot, ui);

        let hue_color = color::rgb(pure_hue.red, pure_hue.green, pure_hue.blue);

        widget::BorderedRectangle::new([20.0, 10.0])
            .color(hue_color)
            .border_color(hue_color.plain_contrast())
            .border(2.0)
            .graphics_for(id)
            .xy(hue_pos)
            .set(state.ids.huedot, ui);

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
    let mut tris = Vec::with_capacity((2_usize).pow(k as u32 + 1));
    let step = 1.0 / (k as f64).exp2();
    let mut x: f64 = 0.0;

    let top = height / 2.0;
    let bottom = -height / 2.0;
    let left = -width / 2.0;
    let right = width / 2.0;

    for _ in 0..(2_u16).pow(k as _) {
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
    let mut tris = Vec::with_capacity((4_usize).pow(k as _) * 2);
    let step = 1.0 / (k as f64).exp2();
    let mut x: f64;
    let mut y: f64 = 0.0;

    let rect_bottom = -height / 2.0;
    let rect_left = -width / 2.0;

    for _ in 0..(2_u16).pow(k as _) {
        x = 0.0;
        for _ in 0..(2_u16).pow(k as _) {
            let c_bl = color(x, y);
            let c_br = color(x + step, y);
            let c_tl = color(x, y + step);
            let c_tr = color(x + step, y + step);

            let left = rect_left + x * width;
            let right = rect_left + (x + step) * width;
            let bottom = rect_bottom + y * height;
            let top = rect_bottom + (y + step) * height;

            tris.push(Triangle([
                ([left, bottom], c_bl),
                ([left, top], c_tl),
                ([right, top], c_tr),
            ]));
            tris.push(Triangle([
                ([left, bottom], c_bl),
                ([right, top], c_tr),
                ([right, bottom], c_br),
            ]));
            x += step;
        }
        y += step;
    }

    tris
}
