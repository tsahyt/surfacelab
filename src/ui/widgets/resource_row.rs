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

    builder_methods! {
        pub expandable { expandable = bool }
        pub active { active = bool }
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub level_indent { style.level_indent = Some(Scalar) }
        pub selected_color { style.selected_color = Some(Color) }
        pub icon_size { style.icon_size = Some(FontSize) }
        pub text_size { style.text_size = Some(FontSize) }
        pub color { style.color = Some(Color) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
    #[conrod(default = "theme.font_size_medium")]
    icon_size: Option<FontSize>,
    #[conrod(default = "theme.font_size_small")]
    text_size: Option<FontSize>,
    #[conrod(default = "16.0")]
    level_indent: Option<Scalar>,
    #[conrod(default = "color::YELLOW")]
    selected_color: Option<Color>,
    #[conrod(default = "theme.label_color")]
    color: Option<Color>,
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
        let widget::UpdateArgs {
            id,
            ui,
            state,
            style,
            ..
        } = args;
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

        let mut indent = self.level as f64 * style.level_indent(&ui.theme);

        if self.expandable {
            for _click in icon_button(
                if self.res_item.expanded() {
                    IconName::DOWN
                } else {
                    IconName::RIGHT
                },
                style.icon_font(&ui.theme),
            )
            .color(color::TRANSPARENT)
            .label_color(style.color(&ui.theme))
            .label_font_size(style.icon_size(&ui.theme))
            .border(0.0)
            .w_h(32.0, 32.0)
            .mid_left_with_margin(indent)
            .parent(id)
            .set(state.ids.expander, ui)
            {
                res = Some(Event::ToggleExpanded);
            }
        }

        indent += 32.0;

        widget::Text::new(icon.0)
            .parent(args.id)
            .color(style.color(&ui.theme))
            .font_size(style.icon_size(&ui.theme))
            .font_id(style.icon_font(&ui.theme))
            .mid_left_with_margin(indent)
            .set(state.ids.icon, ui);

        for _click in ui.widget_input(state.ids.icon).clicks() {
            res = Some(Event::Clicked)
        }

        indent += 32.0;

        let name_color = if self.active {
            style.selected_color(&ui.theme)
        } else {
            style.color(&ui.theme)
        };

        widget::Text::new(self.res_item.resource_string())
            .parent(args.id)
            .color(name_color)
            .font_size(style.text_size(&ui.theme))
            .mid_left_with_margin(indent)
            .set(state.ids.resource_name, ui);

        for _click in ui.widget_input(state.ids.resource_name).clicks() {
            res = Some(Event::Clicked)
        }

        if self.active {
            if let Some(ContextAction::Delete) =
                toolbar::Toolbar::flow_left(&[(IconName::TRASH, ContextAction::Delete)])
                    .icon_font(style.icon_font(&ui.theme))
                    .icon_color(style.color(&ui.theme))
                    .button_color(color::TRANSPARENT)
                    .button_size(16.0)
                    .icon_size(style.text_size(&ui.theme))
                    .parent(args.id)
                    .mid_right_of(args.id)
                    .h(16.0)
                    .set(state.ids.toolbar, ui)
            {
                res = Some(Event::DeleteRequested);
            }
        }

        res
    }
}
