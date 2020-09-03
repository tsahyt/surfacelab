use conrod_core::*;
use palette::*;

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
pub struct Style {

}

widget_ids! {
    #[derive(Debug)]
    pub struct Ids {
        triangles
    }
}

#[derive(Debug)]
pub struct State {
    ids: Ids
}

impl Widget for ColorPicker<Hsv> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        Self::Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
    }
}
