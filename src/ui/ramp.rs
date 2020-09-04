use conrod_core::widget::triangles::{ColoredPoint, Triangle};
use conrod_core::*;
use super::colorpicker::ColorPicker;

#[derive(Clone, WidgetCommon)]
pub struct ColorRamp {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
}

impl ColorRamp {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default(),
        }
    }
}

widget_ids! {
    pub struct Ids {
        triangles,
        colorpicker,
    }
}

pub struct State {
    ids: Ids,
}

#[derive(Default, Debug, PartialEq, Clone, WidgetStyle)]
pub struct Style {}

impl Widget for ColorRamp {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let xy = args.ui.xy_of(args.id).unwrap();
        let wh = args.ui.wh_of(args.id).unwrap();

        let gradient_tris = gradient_strip(&[[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 1.0]], wh[0], 24.0);
        let gradient_pos = [xy[0], xy[1] + wh[1] / 2.0 - 12.0 ];

        widget::Triangles::multi_color(gradient_tris.iter().map(|t| t.add(gradient_pos)))
            .with_bounding_rect(args.rect)
            .set(args.state.ids.triangles, args.ui);

        ColorPicker::new(palette::Hsv::new(180.0, 0.9, 0.9))
            .wh([wh[0], wh[1] - 24.0])
            .parent(args.id)
            .mid_bottom()
            .set(args.state.ids.colorpicker, args.ui);
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
        let next_x = left + step[1] as f64 * width;

        tris.push(
            Triangle([
                ([x, bottom], color),
                ([x, top], color),
                ([next_x, top], next_color),
            ])
        );
        tris.push(
            Triangle([
                ([x, bottom], color),
                ([next_x, top], next_color),
                ([next_x, bottom], next_color),
            ])
        );

        color = next_color;
        x = next_x;
    }

    tris.push(
        Triangle([
            ([x, bottom], color),
            ([x, top], color),
            ([left + width, top], color),
        ])
    );
    tris.push(
        Triangle([
            ([x, bottom], color),
            ([left + width, top], color),
            ([left + width, bottom], color),
        ])
    );

    tris
}
