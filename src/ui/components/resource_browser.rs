use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    app_state::ResourceTree,
    i18n::Language,
    widgets::{resource_row, tree},
};

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ResourceBrowser<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    resource_tree: &'a mut ResourceTree,
    style: Style,
}

impl<'a> ResourceBrowser<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        resource_tree: &'a mut ResourceTree,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            resource_tree,
            style: Style::default(),
        }
    }

    pub fn icon_font(mut self, font_id: text::font::Id) -> Self {
        self.style.icon_font = Some(Some(font_id));
        self
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id")]
    icon_font: Option<Option<text::font::Id>>,
}

widget_ids! {
    pub struct Ids {
        tree
    }
}

impl<'a> Widget for ResourceBrowser<'a> {
    type State = Ids;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let (mut rows, scrollbar) = tree::Tree::new(self.resource_tree.get_tree())
            .parent(args.id)
            .middle_of(args.id)
            .padded_wh_of(args.id, 8.0)
            .scrollbar_on_top()
            .set(args.state.tree, args.ui);

        while let Some(row) = rows.next(args.ui) {
            let expandable = self.resource_tree.expandable(&row.node_id);
            let data = self.resource_tree.get_resource_info_mut(&row.node_id);

            let widget = resource_row::ResourceRow::new(&data, row.level)
                .expandable(expandable)
                .icon_font(self.style.icon_font.unwrap().unwrap())
                .h(32.0);

            match row.item.set(widget, args.ui) {
                None => {}
                Some(resource_row::Event::ToggleExpanded) => {
                    data.toggle_expanded();
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(args.ui);
        }
    }
}
