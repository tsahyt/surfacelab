use crate::ui::app_state::resources::*;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ResourceRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    res_item: &'a ResourceTreeItem,
    expandable: bool,
}

impl<'a> ResourceRow<'a> {
    pub fn new(res_item: &'a ResourceTreeItem) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            res_item,
            expandable: false,
        }
    }

    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = expandable;
        self
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        icon,
        resource_name,
        expander,
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event {
    ToggleExpander,
}

impl<'a> Widget for ResourceRow<'a> {
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
        let mut res = None;

        widget::Text::new(self.res_item.resource_string())
            .parent(args.id)
            .color(color::WHITE)
            .font_size(10)
            .middle()
            .set(args.state.ids.resource_name, args.ui);

        res
    }
}
