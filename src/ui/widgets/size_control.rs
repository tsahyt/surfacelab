use crate::lang::OperatorSize;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct SizeControl {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    size: OperatorSize,
    allow_relative: bool,
    parent_size: Option<u32>,
}

impl SizeControl {
    pub fn new(size: OperatorSize) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            parent_size: None,
            allow_relative: true,
            size,
        }
    }

    builder_methods! {
        pub parent_size { parent_size = Some(u32) }
        pub allow_relative { allow_relative = bool }
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
                debug_assert!(self.allow_relative);
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

                let lower_limit = self
                    .parent_size
                    .map(|x| 5 - (x as f32).log(2.) as i32)
                    .unwrap_or(-6);
                let upper_limit = self
                    .parent_size
                    .map(|x| 14 - (x as f32).log(2.) as i32)
                    .unwrap_or(6);
                let s_ = s.clamp(lower_limit, upper_limit) as f32;

                let lbl = if let Some(parent) = self.parent_size {
                    let s_abs = self.size.absolute(parent);
                    format!("{} × {} (Relative {})", s_abs, s_abs, s_)
                } else {
                    format!("Relative {}", s_)
                };

                if let Some(new) =
                    widget::Slider::new(s_ as f32, lower_limit as f32, upper_limit as f32)
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
                if self.allow_relative {
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
                }

                let lbl = format!("{} x {}", s, s);

                let mut ctrl = widget::Slider::new(s as f32, 32., 16384.)
                    .label(&lbl)
                    .label_font_size(10)
                    .h(16.);

                if self.allow_relative {
                    ctrl = ctrl.padded_w_of(id, 20.).right(8.);
                } else {
                    ctrl = ctrl.w_of(id).mid_left_of(id);
                }

                if let Some(new) = ctrl.set(state.ids.absolute_slider, ui) {
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
