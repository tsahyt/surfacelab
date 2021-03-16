use crate::ui::app_state::Layer;
use crate::ui::util;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct LayerRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    layer: &'a mut Layer,
    active: bool,
    style: Style,
    toggleable: bool,
    expandable: bool,
}

impl<'a> LayerRow<'a> {
    pub fn new(layer: &'a mut Layer, active: bool) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            layer,
            active,
            style: Style::default(),
            toggleable: true,
            expandable: false,
        }
    }

    builder_methods! {
        pub toggleable { toggleable = bool }
        pub expandable { expandable = bool }
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub icon_size { style.icon_size = Some(FontSize) }
        pub icon_size_large { style.icon_size_large = Some(FontSize) }
        pub title_size { style.title_size = Some(FontSize) }
        pub color { style.color = Some(Color) }
        pub selection_color { style.selection_color = Some(Color) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
    #[conrod(default = "theme.font_size_small")]
    icon_size: Option<FontSize>,
    #[conrod(default = "theme.font_size_medium")]
    icon_size_large: Option<FontSize>,
    #[conrod(default = "theme.font_size_small")]
    title_size: Option<FontSize>,
    #[conrod(default = "theme.label_color")]
    color: Option<Color>,
    #[conrod(default = "Color::Rgba(0.9, 0.4, 0.15, 1.0)")]
    selection_color: Option<Color>,
}

widget_ids! {
    pub struct Ids {
        visibility_button,
        expander_button,
        thumbnail,
        layer_type,
        title,
        title_edit,
        background,
        move_up,
        move_down,
    }
}

pub struct State {
    ids: Ids,
    editing_title: bool,
}

pub enum Event {
    ActiveElement,
    Retitled(String),
    ToggleEnabled,
    ToggleExpanded,
    MoveUp,
    MoveDown,
}

impl<'a> Widget for LayerRow<'a> {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
            editing_title: false,
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
            rect,
            ..
        } = args;
        let mut event = None;

        widget::Rectangle::fill([rect.w(), rect.h()])
            .color(color::rgba(
                0.,
                0.,
                0.,
                if self.layer.is_mask { 0.25 } else { 0.0 },
            ))
            .middle()
            .parent(id)
            .graphics_for(id)
            .set(state.ids.background, ui);

        if self.toggleable {
            for _press in util::icon_button(
                if self.layer.enabled {
                    util::IconName::EYE
                } else {
                    util::IconName::EYEOFF
                },
                style.icon_font(&ui.theme),
            )
            .color(color::TRANSPARENT)
            .label_font_size(style.icon_size(&ui.theme))
            .label_color(style.color(&ui.theme))
            .border(0.0)
            .w_h(32.0, 32.0)
            .mid_left_with_margin(8.0)
            .parent(id)
            .set(state.ids.visibility_button, ui)
            {
                event = Some(Event::ToggleEnabled);
            }
        }

        if let Some(image_id) = self.layer.thumbnail {
            widget::Image::new(image_id)
                .w_h(32.0, 32.0)
                .top_left_with_margins(8.0, 48.0)
                .parent(id)
                .graphics_for(id)
                .set(state.ids.thumbnail, ui);
        }

        if state.editing_title {
            for ev in widget::TextBox::new(&self.layer.title)
                .font_size(style.title_size(&ui.theme))
                .mid_left_with_margin(88.0)
                .parent(id)
                .h(16.0)
                .w(rect.w() - 128.0)
                .set(state.ids.title_edit, ui)
            {
                match ev {
                    widget::text_box::Event::Update(new) => {
                        event = Some(Event::Retitled(new.clone()));
                        self.layer.title = new
                    }
                    widget::text_box::Event::Enter => {
                        state.update(|state| state.editing_title = false)
                    }
                }
            }
        } else {
            widget::Text::new(&self.layer.title)
                .color(if self.active {
                    style.selection_color(&ui.theme)
                } else {
                    style.color(&ui.theme)
                })
                .font_size(style.title_size(&ui.theme))
                .mid_left_with_margin(88.0)
                .parent(id)
                .set(state.ids.title, ui);
        }

        for _dblclick in ui
            .widget_input(state.ids.title)
            .events()
            .filter(|ev| matches!(ev, event::Widget::DoubleClick(_)))
        {
            state.update(|state| state.editing_title = true)
        }

        for _click in util::icon_button(util::IconName::UP, style.icon_font(&ui.theme))
            .color(color::TRANSPARENT)
            .label_font_size(style.icon_size(&ui.theme))
            .label_color(style.color(&ui.theme))
            .border(0.0)
            .w_h(16.0, 16.0)
            .top_right_with_margin(8.0)
            .parent(id)
            .set(state.ids.move_up, ui)
        {
            event = Some(Event::MoveUp);
        }

        for _click in util::icon_button(util::IconName::DOWN, style.icon_font(&ui.theme))
            .color(color::TRANSPARENT)
            .label_font_size(style.icon_size(&ui.theme))
            .label_color(style.color(&ui.theme))
            .border(0.0)
            .w_h(16.0, 16.0)
            .bottom_right_with_margin(8.0)
            .parent(id)
            .set(state.ids.move_down, ui)
        {
            event = Some(Event::MoveDown);
        }

        widget::Text::new(self.layer.icon.0)
            .color(style.color(&ui.theme))
            .font_size(style.icon_size_large(&ui.theme))
            .font_id(style.icon_font(&ui.theme))
            .mid_right_with_margin(32.0)
            .parent(id)
            .set(state.ids.layer_type, ui);

        for _click in ui.widget_input(id).clicks() {
            event = Some(Event::ActiveElement);
        }

        if self.expandable {
            for _click in util::icon_button(
                if self.layer.expanded {
                    util::IconName::DOWN
                } else {
                    util::IconName::RIGHT
                },
                style.icon_font(&ui.theme),
            )
            .color(color::TRANSPARENT)
            .label_font_size(style.icon_size_large(&ui.theme))
            .label_color(style.color(&ui.theme))
            .border(0.0)
            .w_h(32.0, 32.0)
            .mid_right_with_margin(64.0)
            .parent(id)
            .set(state.ids.expander_button, ui)
            {
                event = Some(Event::ToggleExpanded);
            }
        }

        event
    }
}
