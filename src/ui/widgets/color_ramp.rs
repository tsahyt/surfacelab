use super::color_picker::ColorPicker;
use crate::ui::util::*;
use conrod_core::widget::triangles::{ColoredPoint, Triangle};
use conrod_core::*;
use smallvec::SmallVec;

#[derive(Clone, WidgetCommon)]
pub struct ColorRamp<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    ramp: &'a [[f32; 4]],
    style: Style,
}

impl<'a> ColorRamp<'a> {
    pub fn new(ramp: &'a [[f32; 4]]) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            ramp,
            style: Style::default(),
        }
    }

    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
    }
}

widget_ids! {
    pub struct Ids {
        gradient_triangles,
        colorpicker,
        add_step,
        delete_step,
        step_dialer,
        steps[],
    }
}

pub struct State {
    ids: Ids,
    selected: usize,
}

pub enum Event {
    ChangeStep(usize, [f32; 4]),
    AddStep(usize),
    DeleteStep(usize),
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

impl<'a> Widget for ColorRamp<'a> {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            selected: 0,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            id,
            ui,
            state,
            rect,
            style,
            ..
        } = args;

        let mut event = None;

        let xy = rect.xy();
        let wh = rect.w_h();

        if state.selected >= self.ramp.len() {
            state.update(|state| state.selected = 0)
        }

        let selected_step = self.ramp[state.selected];
        let selected_position = selected_step[3];
        let selected_color = palette::Hsv::from(palette::LinSrgb::new(
            selected_step[0],
            selected_step[1],
            selected_step[2],
        ));

        let mut display_ramp: SmallVec<[_; 16]> = self.ramp.iter().copied().collect();
        display_ramp.sort_by(|a, b| a[3].partial_cmp(&b[3]).unwrap());

        let gradient_tris = gradient_strip(display_ramp.as_slice(), wh.0 - 64., 24.0);
        let gradient_pos = [xy[0] - 32., xy[1] + wh.1 / 2.0 - 12.0];

        widget::Triangles::multi_color(gradient_tris.iter().map(|t| t.add(gradient_pos)))
            .with_bounding_rect(rect)
            .depth(32.)
            .set(state.ids.gradient_triangles, ui);

        if self.ramp.len() != state.ids.steps.len() {
            state.update(|state| {
                state
                    .ids
                    .steps
                    .resize(self.ramp.len(), &mut ui.widget_id_generator())
            });
        }

        for (i, step) in self.ramp.iter().enumerate() {
            let step_id = state.ids.steps[i];
            let step_pos = step[3] as f64 * (wh.0 - 64.);
            let step_color = color::rgb(step[0], step[1], step[2]);

            widget::BorderedRectangle::new([8., 48.])
                .border(if i == state.selected { 1.0 } else { 0.0 })
                .parent(id)
                .top_left_with_margins(0., step_pos - 2.)
                .color(step_color)
                .border_color(step_color.plain_contrast())
                .set(step_id, ui);

            for _click in ui.widget_input(step_id).clicks().left() {
                state.update(|state| state.selected = i);
            }

            for drag in ui.widget_input(step_id).drags().left() {
                state.update(|state| state.selected = i);
                let new_pos = (step[3] as f64 + drag.delta_xy[0] / (wh.0 - 64.)).clamp(0., 1.);
                event = Some(Event::ChangeStep(
                    i,
                    [step[0], step[1], step[2], new_pos as f32],
                ));
            }
        }

        for _press in icon_button(IconName::MINUS, style.icon_font(&ui.theme))
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .border(0.)
            .parent(id)
            .label_font_size(10)
            .w(24.0)
            .h(24.0)
            .top_right()
            .set(state.ids.delete_step, ui)
        {
            event = Some(Event::DeleteStep(state.selected));
        }

        for _press in icon_button(IconName::PLUS, style.icon_font(&ui.theme))
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .border(0.)
            .parent(id)
            .label_font_size(10)
            .w(24.0)
            .h(24.0)
            .left(8.0)
            .set(state.ids.add_step, ui)
        {
            event = Some(Event::AddStep(state.selected));
        }

        if let Some(new_pos) = widget::NumberDialer::new(selected_position, 0.0, 1.0, 4)
            .parent(id)
            .label_font_size(10)
            .right(8.0)
            .w(56.0)
            .h(16.0)
            .top_right_with_margins(32., 0.)
            .set(state.ids.step_dialer, ui)
        {
            event = Some(Event::ChangeStep(
                state.selected,
                [
                    selected_step[0],
                    selected_step[1],
                    selected_step[2],
                    new_pos,
                ],
            ));
        }

        if let Some(new_color) = ColorPicker::new(selected_color)
            .wh([wh.0, wh.1 - 56.0])
            .parent(id)
            .mid_bottom()
            .set(state.ids.colorpicker, ui)
        {
            let rgb = palette::LinSrgb::from(new_color);
            event = Some(Event::ChangeStep(
                state.selected,
                [rgb.red, rgb.green, rgb.blue, selected_position],
            ));
        }

        event
    }
}

/// Produce a colored strip from the given gradient with RGB steps, centered on
/// (0,0). Assumes there is at least one step given!
fn gradient_strip(steps: &[[f32; 4]], width: f64, height: f64) -> Vec<Triangle<ColoredPoint>> {
    assert!(!steps.is_empty());

    let mut tris = Vec::with_capacity(steps.len() * 2);

    let left = -width / 2.0;
    let bottom = -height / 2.0;
    let top = height / 2.0;

    let mut x: f64 = left;
    let mut color = color::Rgba(steps[0][0], steps[0][1], steps[0][2], 1.0);

    for step in steps {
        let next_color = color::Rgba(step[0], step[1], step[2], 1.0);
        let next_x = left + step[3] as f64 * width;

        tris.push(Triangle([
            ([x, bottom], color),
            ([x, top], color),
            ([next_x, top], next_color),
        ]));
        tris.push(Triangle([
            ([x, bottom], color),
            ([next_x, top], next_color),
            ([next_x, bottom], next_color),
        ]));

        color = next_color;
        x = next_x;
    }

    tris.push(Triangle([
        ([x, bottom], color),
        ([x, top], color),
        ([left + width, top], color),
    ]));
    tris.push(Triangle([
        ([x, bottom], color),
        ([left + width, top], color),
        ([left + width, bottom], color),
    ]));

    tris
}
