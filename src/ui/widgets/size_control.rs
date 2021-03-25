use crate::lang::OperatorSize;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct SizeControl {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    size: OperatorSize,
}

impl SizeControl {
    pub fn new(size: OperatorSize) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            size,
        }
    }

    builder_methods! {}
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
    SetAbsolute,
    SetRelative,
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
                    .set(state.ids.absolute_toggle, ui)
                {
                    ev = Some(Event::SetAbsolute);
                }
            }
            OperatorSize::AbsoluteSize(s) => {
                for _click in widget::Toggle::new(true)
                    .parent(id)
                    .set(state.ids.absolute_toggle, ui)
                {
                    ev = Some(Event::SetRelative)
                }
            }
        }

        ev
    }
}
