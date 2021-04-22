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

    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

widget_ids! {
    pub struct Ids {
        list,
        filter_display,
        filter_icon,
        filter_canvas,
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
        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            ..
        } = args;
        let FilteredList { items, .. } = self;

        let mut ret = None;
        let mut picked = false;

        // Listen to all input globally
        for ev in ui.global_input().events().ui() {
            match ev {
                event::Ui::Text(_, event::Text { string, modifiers })
                    if !modifiers.contains(input::ModifierKey::CTRL) =>
                {
                    state.update(|state| state.filter_string.push_str(string));
                }
                event::Ui::Press(
                    _,
                    event::Press {
                        button: event::Button::Keyboard(input::Key::Backspace),
                        ..
                    },
                ) => {
                    state.update(|state| {
                        state.filter_string.pop();
                    });
                }
                event::Ui::Press(
                    _,
                    event::Press {
                        button: event::Button::Keyboard(input::Key::X),
                        modifiers: input::ModifierKey::CTRL,
                    },
                ) => {
                    state.update(|state| state.filter_string.clear());
                }
                event::Ui::Press(
                    _,
                    event::Press {
                        button: event::Button::Keyboard(input::Key::Return),
                        ..
                    },
                ) => {
                    picked = true;
                    ret = items
                        .clone()
                        .filter(|item| item.filter(&state.filter_string))
                        .next();
                }
                _ => {}
            }
        }

        widget::Canvas::new()
            .parent(id)
            .color(color::BLACK.alpha(0.15))
            .border(0.)
            .mid_top()
            .h(32.)
            .set(state.ids.filter_canvas, ui);

        widget::Text::new(IconName::SEARCH.0)
            .parent(state.ids.filter_canvas)
            .font_id(style.icon_font(&ui.theme))
            .font_size(12)
            .color(color::WHITE)
            .mid_left_with_margin(8.)
            .set(state.ids.filter_icon, ui);

        widget::Text::new(&state.filter_string)
            .parent(state.ids.filter_canvas)
            .font_size(10)
            .color(color::WHITE)
            .right(8.)
            .set(state.ids.filter_display, ui);

        let mut filtered = items
            .filter(|item| item.filter(&state.filter_string))
            .take(self.limit.unwrap_or(usize::MAX));

        let (mut list_items, scrollbar) = widget::list::List::flow_down(filtered.clone().count())
            .parent(id)
            .w_of(id)
            .padded_h_of(id, 20.)
            .mid_top_with_margin(40.)
            .item_size(32.)
            .scrollbar_on_top()
            .instantiate_all_items()
            .set(state.ids.list, ui);

        while let Some(list_item) = list_items.next(ui) {
            let item = filtered.next().unwrap();
            let label = item.display();

            let button = widget::Button::new()
                .left_justify_label()
                .label_x(position::Relative::Align(position::Align::Start))
                .label(&label)
                .label_color(conrod_core::color::WHITE)
                .label_font_size(12)
                .color(color::DARK_CHARCOAL)
                .border(0.)
                .color(conrod_core::color::DARK_CHARCOAL);
            for _press in list_item.set(button, ui) {
                picked = true;
                ret = Some(item);
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui)
        }

        if picked {
            state.update(|state| state.filter_string.clear());
        }

        ret
    }
}
