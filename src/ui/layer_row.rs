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
}

impl<'a> LayerRow<'a> {
    pub fn new(layer: &'a mut Layer, active: bool) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            layer,
            active,
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
        visibility_button,
        thumbnail,
        layer_type,
        title,
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event {
    ActiveElement,
}

impl<'a> Widget for LayerRow<'a> {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut event = None;

        util::icon_button(
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
        .set(args.state.ids.visibility_button, args.ui);

        if let Some(image_id) = self.layer.thumbnail {
            widget::Image::new(image_id)
                .w_h(32.0, 32.0)
                .top_left_with_margins(8.0, 40.0)
                .parent(args.id)
                .graphics_for(args.id)
                .set(args.state.ids.thumbnail, args.ui);
        }

        widget::Text::new(&self.layer.title)
            .color(if self.active {
                color::Color::Rgba(0.9, 0.4, 0.15, 1.0)
            } else {
                color::WHITE
            })
            .font_size(12)
            .mid_left_with_margin(80.0)
            .parent(args.id)
            .graphics_for(args.id)
            .set(args.state.ids.title, args.ui);

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
