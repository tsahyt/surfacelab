use conrod_core::*;

#[derive(Clone, WidgetCommon)]
pub struct ColorRamp {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
}

impl ColorRamp {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default()
        }
    }
}


pub struct State {

}

#[derive(Default, Debug, PartialEq, Clone, WidgetStyle)]
pub struct Style {

}

impl Widget for ColorRamp {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
        }
    }

    fn style(&self) -> Self::Style {
        Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
    }
}
