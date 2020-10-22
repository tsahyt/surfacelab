use conrod_core::*;

#[derive(WidgetCommon)]
pub struct LayerRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    title: &'a str,
    style: Style,
}

impl<'a> LayerRow<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            title,
            style: Style::default(),
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
    }
}

pub struct State {
    ids: Ids,
}

impl<'a> Widget for LayerRow<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
    }
}
