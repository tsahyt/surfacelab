use crate::lang::OperatorSize;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct SizeControl {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    size: OperatorSize,
    parent_size: Option<u32>,
}

impl SizeControl {
    pub fn new(size: OperatorSize) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            parent_size: None,
            size,
        }
    }

    builder_methods! {
        pub parent_size { parent_size = Some(u32) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        absolute_toggle,
        relative_slider,
        absolute_slider,
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event {
    ToAbsolute,
    ToRelative,
    NewSize(OperatorSize),
}

impl Widget for SizeControl {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut ev = None;
        let widget::UpdateArgs { ui, id, state, .. } = args;

        match self.size {
            OperatorSize::RelativeToParent(s) => {
                for _click in widget::Toggle::new(false)
                    .parent(id)
                    .mid_left_of(id)
                    .wh([32., 16.])
                    .label("Abs")
                    .label_font_size(10)
                    .color(color::WHITE)
                    .set(state.ids.absolute_toggle, ui)
                {
                    ev = Some(Event::ToAbsolute);
                }

                let lbl = if let Some(parent) = self.parent_size {
                    let s_abs = self.size.absolute(parent);
                    format!("{} × {} (Relative {})", s_abs, s_abs, s)
                } else {
                    format!("Relative {}", s)
                };

                let lower_limit = self
                    .parent_size
                    .map(|x| 5 - (x as f32).log(2.) as i32)
                    .unwrap_or(-6);
                let upper_limit = self
                    .parent_size
                    .map(|x| 14 - (x as f32).log(2.) as i32)
                    .unwrap_or(6);

                if let Some(new) =
                    widget::Slider::new(s as f32, lower_limit as f32, upper_limit as f32)
                        .label(&lbl)
                        .label_font_size(10)
                        .padded_w_of(id, 20.)
                        .right(8.)
                        .h(16.)
                        .set(state.ids.relative_slider, ui)
                {
                    let new = new as i32;
                    if new != s {
                        ev = Some(Event::NewSize(OperatorSize::RelativeToParent(new)));
                    }
                }
            }
            OperatorSize::AbsoluteSize(s) => {
                for _click in widget::Toggle::new(true)
                    .parent(id)
                    .mid_left_of(id)
                    .wh([32., 16.])
                    .label("Abs")
                    .label_font_size(10)
                    .color(color::WHITE)
                    .set(state.ids.absolute_toggle, ui)
                {
                    ev = Some(Event::ToRelative)
                }

                if let Some(new) = widget::Slider::new(s as f32, 32., 16384.)
                    .label(&format!("{} x {}", s, s))
                    .label_font_size(10)
                    .padded_w_of(id, 20.)
                    .right(8.)
                    .h(16.)
                    .set(state.ids.absolute_slider, ui)
                {
                    let new = OperatorSize::abs_nearest(new);
                    if new != self.size {
                        ev = Some(Event::NewSize(new));
                    }
                }
            }
        }

        ev
    }
}
