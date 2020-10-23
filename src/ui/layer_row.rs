use super::app_state::Layer;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct LayerRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    layer: &'a mut Layer,
    style: Style,
}

impl<'a> LayerRow<'a> {
    pub fn new(layer: &'a mut Layer) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            layer,
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
        rectangle,
        thumbnail,
        layer_type,
        title,
    }
}

pub struct State {
    ids: Ids,
}

impl<'a> Widget for LayerRow<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        widget::BorderedRectangle::new([args.rect.w(), args.rect.h()])
            .border(1.0)
            .color(color::DARK_CHARCOAL)
            .parent(args.id)
            .middle_of(args.id)
            .set(args.state.ids.rectangle, args.ui);

        if let Some(image_id) = self.layer.thumbnail {
            widget::Image::new(image_id)
                .w_h(24.0, 24.0)
                .top_left_with_margin(8.0)
                .parent(args.id)
                .set(args.state.ids.thumbnail, args.ui);
        }

        widget::Text::new(self.layer.icon.0)
            .color(color::WHITE)
            .font_size(12)
            .font_id(self.style.icon_font.unwrap().unwrap())
            .mid_left_with_margin(40.0)
            .parent(args.id)
            .set(args.state.ids.layer_type, args.ui);

        widget::Text::new(&self.layer.title)
            .color(color::WHITE)
            .font_size(12)
            .mid_left_with_margin(60.0)
            .parent(args.id)
            .set(args.state.ids.title, args.ui);
    }
}
