use crate::ui::app_state::resources::*;
use crate::ui::widgets::tree::Expandable;
use crate::ui::util::*;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ResourceRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    res_item: &'a ResourceTreeItem,
    expandable: bool,
    level: usize,
}

impl<'a> ResourceRow<'a> {
    pub fn new(res_item: &'a ResourceTreeItem, level: usize) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            res_item,
            expandable: false,
            level,
        }
    }

    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = expandable;
        self
    }

    pub fn icon_font(mut self, font_id: text::font::Id) -> Self {
        self.style.icon_font = Some(Some(font_id));
        self
    }

    pub fn level_indent(mut self, indent: Scalar) -> Self {
        self.style.level_indent = Some(indent);
        self
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id")]
    icon_font: Option<Option<text::font::Id>>,
    #[conrod(default = "16.0")]
    level_indent: Option<Scalar>,
}

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
    ToggleExpanded,
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

        let icon = match self.res_item {
            ResourceTreeItem::ResourceInfo(i) => match i.category() {
                ResourceCategory::Graph => IconName::GRAPH,
                ResourceCategory::Node => IconName::NODE,
                ResourceCategory::Socket => IconName::SOCKET,
                ResourceCategory::Image => IconName::IMAGE,
                ResourceCategory::Input => IconName::INPUT,
                ResourceCategory::Output => IconName::OUTPUT,
            },
            ResourceTreeItem::Folder(_, _) => IconName::FOLDER,
        };

        let mut indent = self.level as f64 * self.style.level_indent.unwrap_or(16.0) + 8.0;

        widget::Text::new(icon.0)
            .parent(args.id)
            .color(color::WHITE)
            .font_size(14)
            .font_id(self.style.icon_font.unwrap().unwrap())
            .mid_left_with_margin(indent)
            .set(args.state.ids.icon, args.ui);

        if self.expandable {
            indent += 32.0;
            for _click in icon_button(
                if self.res_item.expanded() {
                    IconName::DOWN
                } else {
                    IconName::RIGHT
                },
                self.style.icon_font.unwrap().unwrap(),
            )
            .color(color::TRANSPARENT)
            .label_font_size(14)
            .label_color(color::WHITE)
            .border(0.0)
            .w_h(32.0, 32.0)
            .mid_left_with_margin(indent)
            .parent(args.id)
            .set(args.state.ids.expander, args.ui)
            {
                res = Some(Event::ToggleExpanded);
            }
        }
        indent += 32.0;

        widget::Text::new(self.res_item.resource_string())
            .parent(args.id)
            .color(color::WHITE)
            .font_size(10)
            .mid_left_with_margin(indent)
            .set(args.state.ids.resource_name, args.ui);

        res
    }
}
