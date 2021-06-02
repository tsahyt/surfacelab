use crate::ui::app_state::Layer;
use crate::ui::{util, widgets::toolbar};
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
        toolbar,
    }
}

pub struct State {
    ids: Ids,
    editing_title: bool,
    dragging: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum ContextAction {
    Delete,
    AddMask,
    ToggleExpanded,
}

fn context_actions(
    is_mask: bool,
    base_layer: bool,
    expanded: Option<bool>,
) -> Vec<(util::IconName, ContextAction)> {
    let mut actions = Vec::new();

    if let Some(expanded) = expanded {
        actions.push((
            if expanded {
                util::IconName::DOWN
            } else {
                util::IconName::RIGHT
            },
            ContextAction::ToggleExpanded,
        ))
    }
    if !is_mask && !base_layer {
        actions.push((util::IconName::MASK, ContextAction::AddMask));
    }

    actions.push((util::IconName::TRASH, ContextAction::Delete));

    actions
}

pub enum Event {
    ActiveElement,
    Retitled(String, String),
    ToggleEnabled,
    ToggleExpanded,
    Drag(Point),
    Drop,
    AddMask,
    Delete,
}

impl<'a> Widget for LayerRow<'a> {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
            editing_title: false,
            dragging: false,
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

        let hovering = rect.is_over(ui.global_input().current.mouse.xy);

        let background_alpha = {
            let dragging_alpha: f32 = if state.dragging { 0.35 } else { 0. };
            let mask_alpha: f32 = if self.layer.is_mask { 0.25 } else { 0. };
            dragging_alpha.max(mask_alpha)
        };

        let background_color = if state.dragging {
            color::DARK_RED
        } else {
            color::BLACK
        }
        .with_alpha(background_alpha);

        widget::Rectangle::fill([rect.w(), rect.h()])
            .color(background_color)
            .middle()
            .parent(id)
            .set(state.ids.background, ui);

        for ev in ui.widget_input(state.ids.background).events() {
            match ev {
                event::Widget::Release(r) if r.mouse().is_some() && state.dragging => {
                    state.update(|state| state.dragging = false);
                    event = Some(Event::Drop);
                }
                event::Widget::Click(_) => {
                    event = Some(Event::ActiveElement);
                }
                event::Widget::DoubleClick(_) => {
                    state.update(|state| state.editing_title = true);
                }
                event::Widget::Drag(d) => {
                    event = Some(Event::Drag(d.total_delta_xy));
                    state.update(|state| state.dragging = true);
                }
                _ => {}
            }
        }

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
            .parent(state.ids.background)
            .set(state.ids.visibility_button, ui)
            {
                event = Some(Event::ToggleEnabled);
            }
        }

        if let Some(image_id) = self.layer.thumbnail {
            widget::Image::new(image_id)
                .w_h(32.0, 32.0)
                .top_left_with_margins(8.0, 48.0)
                .parent(state.ids.background)
                .graphics_for(state.ids.background)
                .set(state.ids.thumbnail, ui);
        }

        if state.editing_title {
            for ev in widget::TextBox::new(&self.layer.title)
                .font_size(style.title_size(&ui.theme))
                .mid_left_with_margin(88.0)
                .parent(state.ids.background)
                .h(16.0)
                .w(rect.w() - 128.0)
                .set(state.ids.title_edit, ui)
            {
                match ev {
                    widget::text_box::Event::Update(new) => {
                        event = Some(Event::Retitled(self.layer.title.clone(), new.clone()));
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
                .parent(state.ids.background)
                .set(state.ids.title, ui);
        }

        widget::Text::new(self.layer.icon.0)
            .color(style.color(&ui.theme))
            .font_size(style.icon_size_large(&ui.theme))
            .font_id(style.icon_font(&ui.theme))
            .mid_right_with_margin(8.0)
            .parent(state.ids.background)
            .set(state.ids.layer_type, ui);

        for _click in ui.widget_input(id).clicks() {
            event = Some(Event::ActiveElement);
        }

        if hovering {
            match toolbar::Toolbar::flow_left(
                context_actions(
                    self.layer.is_mask,
                    !self.toggleable,
                    if self.expandable {
                        Some(self.layer.expanded)
                    } else {
                        None
                    },
                )
                .into_iter(),
            )
            .icon_font(style.icon_font(&ui.theme))
            .icon_color(style.color(&ui.theme))
            .button_color(color::TRANSPARENT)
            .button_size(16.0)
            .icon_size(style.icon_size(&ui.theme))
            .parent(id)
            .mid_right_with_margin(40.0)
            .h(16.0)
            .set(state.ids.toolbar, ui)
            {
                Some(ContextAction::AddMask) => event = Some(Event::AddMask),
                Some(ContextAction::Delete) => event = Some(Event::Delete),
                Some(ContextAction::ToggleExpanded) => event = Some(Event::ToggleExpanded),
                None => {}
            }
        } else if self.expandable && !self.layer.expanded {
            widget::Text::new(util::IconName::RIGHT.0)
                .color(style.color(&ui.theme))
                .font_size(style.icon_size(&ui.theme))
                .font_id(style.icon_font(&ui.theme))
                .mid_right_with_margin(40.0)
                .parent(state.ids.background)
                .set(state.ids.layer_type, ui);
        }

        event
    }
}
