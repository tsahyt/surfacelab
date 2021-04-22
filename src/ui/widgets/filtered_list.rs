use crate::ui::util::*;
use conrod_core::*;

pub trait FilteredListItem {
    fn filter(&self, filter_string: &str) -> bool;
    fn display(&self) -> &str;
}

#[derive(WidgetCommon)]
pub struct FilteredList<'a, T: 'a, I>
where
    I: Iterator<Item = &'a T> + Clone,
    T: FilteredListItem,
{
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    items: I,
    limit: Option<usize>,
    style: Style,
}

impl<'a, T, I> FilteredList<'a, T, I>
where
    I: Iterator<Item = &'a T> + Clone,
    T: FilteredListItem,
{
    pub fn new(items: I) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            items,
            limit: None,
            style: Style::default(),
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        list
    }
}

pub struct State {
    ids: Ids,
    filter_string: String,
}

impl<'a, T, I> Widget for FilteredList<'a, T, I>
where
    I: Iterator<Item = &'a T> + Clone,
    T: FilteredListItem,
{
    type State = State;
    type Style = Style;
    type Event = Option<&'a T>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            filter_string: String::new(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { state, ui, id, .. } = args;
        let FilteredList { items, .. } = self;

        let mut ret = None;

        let mut filtered = items
            .filter(|item| item.filter(&state.filter_string))
            .take(self.limit.unwrap_or(usize::MAX));

        let (mut list_items, scrollbar) = widget::list::List::flow_down(filtered.clone().count())
            .parent(id)
            .middle()
            .item_size(40.)
            .scrollbar_on_top()
            .instantiate_all_items()
            .set(state.ids.list, ui);

        while let Some(list_item) = list_items.next(ui) {
            let item = filtered.next().unwrap();
            let label = item.display();

            let button = widget::Button::new()
                .label(&label)
                .label_color(conrod_core::color::WHITE)
                .label_font_size(12)
                .color(conrod_core::color::CHARCOAL);
            for _press in list_item.set(button, ui) {
                ret = Some(item);
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui)
        }

        ret
    }
}
