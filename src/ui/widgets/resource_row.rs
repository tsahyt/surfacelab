use crate::ui::app_state::resources::*;
use crate::ui::util::*;
use crate::ui::widgets::{toolbar, tree::Expandable};
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ResourceRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    res_item: &'a ResourceTreeItem,
    expandable: bool,
    active: bool,
    level: usize,
}

impl<'a> ResourceRow<'a> {
    pub fn new(res_item: &'a ResourceTreeItem, level: usize) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            res_item,
            expandable: false,
            active: false,
            level,
        }
    }

    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = expandable;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
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

    pub fn selected_color(mut self, color: Color) -> Self {
        self.style.selected_color = Some(color);
        self
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id")]
    icon_font: Option<Option<text::font::Id>>,
    #[conrod(default = "16.0")]
    level_indent: Option<Scalar>,
    #[conrod(default = "color::YELLOW")]
    selected_color: Option<Color>,
}

widget_ids! {
    pub struct Ids {
        icon,
        resource_name,
        expander,
        toolbar
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event {
    ToggleExpanded,
    Clicked,
    DeleteRequested,
}

pub enum ContextAction {
    Delete,
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
                ResourceCategory::Stack => IconName::LAYERS,
                ResourceCategory::Node => IconName::NODE,
                ResourceCategory::Layer => IconName::NODE,
                ResourceCategory::Socket => IconName::SOCKET,
                ResourceCategory::Image => IconName::IMAGE,
                ResourceCategory::Input => IconName::INPUT,
                ResourceCategory::Output => IconName::OUTPUT,
            },
            ResourceTreeItem::Folder(_, _) => IconName::FOLDER,
        };

        let mut indent = self.level as f64 * self.style.level_indent.unwrap_or(16.0);

        if self.expandable {
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

        widget::Text::new(icon.0)
            .parent(args.id)
            .color(color::WHITE)
            .font_size(14)
            .font_id(self.style.icon_font.unwrap().unwrap())
            .mid_left_with_margin(indent)
            .set(args.state.ids.icon, args.ui);

        for _click in args.ui.widget_input(args.state.ids.icon).clicks() {
            res = Some(Event::Clicked)
        }

        indent += 32.0;

        let name_color = if self.active {
            self.style
                .selected_color
                .unwrap_or(color::Color::Rgba(0.9, 0.4, 0.15, 1.0))
        } else {
            color::WHITE
        };

        widget::Text::new(self.res_item.resource_string())
            .parent(args.id)
            .color(name_color)
            .font_size(10)
            .mid_left_with_margin(indent)
            .set(args.state.ids.resource_name, args.ui);

        for _click in args.ui.widget_input(args.state.ids.resource_name).clicks() {
            res = Some(Event::Clicked)
        }

        if self.active {
            if let Some(ContextAction::Delete) =
                toolbar::Toolbar::flow_left(&[(IconName::TRASH, ContextAction::Delete)])
                    .icon_font(self.style.icon_font.unwrap().unwrap())
                    .button_size(16.0)
                    .icon_size(10)
                    .parent(args.id)
                    .mid_right_of(args.id)
                    .h(16.0)
                    .set(args.state.ids.toolbar, args.ui)
            {
                res = Some(Event::DeleteRequested);
            }
        }

        res
    }
}
