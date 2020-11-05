use super::app_state::Layer;
use super::util;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct LayerRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    layer: &'a mut Layer,
    active: bool,
    style: Style,
    toggleable: bool,
}

impl<'a> LayerRow<'a> {
    pub fn new(layer: &'a mut Layer, active: bool) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            layer,
            active,
            style: Style::default(),
            toggleable: true,
        }
    }

    pub fn icon_font(mut self, font_id: text::font::Id) -> Self {
        self.style.icon_font = Some(Some(font_id));
        self
    }

    pub fn toggleable(mut self, toggleable: bool) -> Self {
        self.toggleable = toggleable;
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
        visibility_button,
        thumbnail,
        layer_type,
        title,
        title_edit,
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
        let mut event = None;

        if self.toggleable {
            for _press in util::icon_button(
                if self.layer.enabled {
                    util::IconName::EYE
                } else {
                    util::IconName::EYEOFF
                },
                self.style.icon_font.unwrap().unwrap(),
            )
            .color(color::DARK_CHARCOAL)
            .label_font_size(10)
            .label_color(color::WHITE)
            .border(0.0)
            .w_h(32.0, 32.0)
            .mid_left_with_margin(8.0)
            .parent(args.id)
            .set(args.state.ids.visibility_button, args.ui)
            {
                event = Some(Event::ToggleEnabled);
            }
        }

        if let Some(image_id) = self.layer.thumbnail {
            widget::Image::new(image_id)
                .w_h(32.0, 32.0)
                .top_left_with_margins(8.0, 48.0)
                .parent(args.id)
                .graphics_for(args.id)
                .set(args.state.ids.thumbnail, args.ui);
        }

        if args.state.editing_title {
            for ev in widget::TextBox::new(&self.layer.title)
                .font_size(10)
                .mid_left_with_margin(88.0)
                .parent(args.id)
                .h(16.0)
                .w(args.rect.w() - 128.0)
                .set(args.state.ids.title_edit, args.ui)
            {
                match ev {
                    widget::text_box::Event::Update(new) => {
                        event = Some(Event::Retitled(new.clone()));
                        self.layer.title = new
                    }
                    widget::text_box::Event::Enter => {
                        args.state.update(|state| state.editing_title = false)
                    }
                }
            }
        } else {
            widget::Text::new(&self.layer.title)
                .color(if self.active {
                    color::Color::Rgba(0.9, 0.4, 0.15, 1.0)
                } else {
                    color::WHITE
                })
                .font_size(12)
                .mid_left_with_margin(88.0)
                .parent(args.id)
                .set(args.state.ids.title, args.ui);
        }

        for _dblclick in args
            .ui
            .widget_input(args.state.ids.title)
            .events()
            .filter(|ev| match ev {
                event::Widget::DoubleClick(_) => true,
                _ => false,
            })
        {
            args.state.update(|state| state.editing_title = true)
        }

        widget::Text::new(self.layer.icon.0)
            .color(color::WHITE)
            .font_size(14)
            .font_id(self.style.icon_font.unwrap().unwrap())
            .mid_right_with_margin(8.0)
            .parent(args.id)
            .graphics_for(args.id)
            .set(args.state.ids.layer_type, args.ui);

        for _click in args.ui.widget_input(args.id).clicks() {
            event = Some(Event::ActiveElement);
        }

        event
    }
}
