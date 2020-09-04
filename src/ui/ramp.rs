use super::colorpicker::ColorPicker;
use conrod_core::widget::triangles::{ColoredPoint, Triangle};
use conrod_core::*;

#[derive(Clone, WidgetCommon)]
pub struct ColorRamp<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    ramp: &'a [[f32; 4]],
}

impl<'a> ColorRamp<'a> {
    pub fn new(ramp: &'a [[f32; 4]]) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            ramp,
        }
    }
}

widget_ids! {
    pub struct Ids {
        triangles,
        colorpicker,
        add_step,
        delete_step,
        step_dialer,
    }
}

pub struct State {
    ids: Ids,
    selected: usize,
}

pub enum Event {
    ChangeStep(usize, [f32; 4]),
    AddStep(usize, [f32; 4]),
    DeleteStep(usize),
}

#[derive(Default, Debug, PartialEq, Clone, WidgetStyle)]
pub struct Style {}

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
        Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let xy = args.ui.xy_of(args.id).unwrap();
        let wh = args.ui.wh_of(args.id).unwrap();

        let gradient_tris = gradient_strip(self.ramp, wh[0], 24.0);
        let gradient_pos = [xy[0], xy[1] + wh[1] / 2.0 - 12.0];

        widget::Triangles::multi_color(gradient_tris.iter().map(|t| t.add(gradient_pos)))
            .with_bounding_rect(args.rect)
            .set(args.state.ids.triangles, args.ui);

        let selected_step = self.ramp[args.state.selected];
        let selected_position = selected_step[3];
        let selected_color = palette::Hsv::from(palette::LinSrgb::new(
            selected_step[0],
            selected_step[1],
            selected_step[2],
        ));

        let mut event = None;

        let button_width = (wh[0] - (8.0 * 2.0)) / 3.0;
        for _press in widget::Button::new()
            .label("Add")
            .parent(args.id)
            .label_font_size(10)
            .top_left_with_margins(32.0, 0.0)
            .w(button_width)
            .h(16.0)
            .set(args.state.ids.add_step, args.ui)
        {}

        for _press in widget::Button::new()
            .label("Delete")
            .parent(args.id)
            .label_font_size(10)
            .w(button_width)
            .h(16.0)
            .right(8.0)
            .set(args.state.ids.delete_step, args.ui)
        {}

        for new_pos in widget::NumberDialer::new(selected_position, 0.0, 1.0, 4)
            .parent(args.id)
            .label_font_size(10)
            .right(8.0)
            .w(button_width)
            .h(16.0)
            .set(args.state.ids.step_dialer, args.ui)
        {
            event = Some(Event::ChangeStep(
                args.state.selected,
                [
                    selected_step[0],
                    selected_step[1],
                    selected_step[2],
                    new_pos,
                ],
            ));
        }

        for new_color in ColorPicker::new(selected_color)
            .wh([wh[0], wh[1] - 40.0])
            .parent(args.id)
            .mid_bottom()
            .set(args.state.ids.colorpicker, args.ui)
        {
            let rgb = palette::LinSrgb::from(new_color);
            event = Some(Event::ChangeStep(
                args.state.selected,
                [rgb.red, rgb.green, rgb.blue, selected_position],
            ));
        }

        event
    }
}

/// Produce a colored strip from the given gradient with RGB steps, centered on
/// (0,0). Assumes there is at least one step given!
fn gradient_strip(steps: &[[f32; 4]], width: f64, height: f64) -> Vec<Triangle<ColoredPoint>> {
    assert!(steps.len() > 0);

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
