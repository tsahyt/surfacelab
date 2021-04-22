use crate::ui::util::*;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct FilteredList<'a, T: 'a, I>
where
    I: Iterator<Item = &'a T>,
{
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    items: I,
    style: Style,
}

impl<'a, T, I> FilteredList<'a, T, I> where I: Iterator<Item = &'a T> {
    pub fn new(items: I) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            items,
            style: Style::default(),
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
}

widget_ids! {
    pub struct Ids {
        buttons[]
    }
}

pub struct State {
    ids: Ids,
}

impl<'a, T, I> Widget for FilteredList<'a, T, I>
where
    I: Iterator<Item = &'a T>,
{
    type State = State;
    type Style = Style;
    type Event = Option<&'a T>;

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
