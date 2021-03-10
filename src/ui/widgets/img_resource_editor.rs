use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ImageResourceEditor {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
}

impl ImageResourceEditor {}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event {}

impl Widget for ImageResourceEditor {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        None
    }
}
