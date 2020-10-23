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
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        thumbnail,
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
        if let Some(image_id) = self.layer.thumbnail {
            widget::Image::new(image_id)
                .w_h(24.0, 24.0)
                .top_left_with_margin(8.0)
                .parent(args.id)
                .set(args.state.ids.thumbnail, args.ui);
        }

        widget::Text::new(&self.layer.title)
            .font_size(12)
            .top_left_with_margins(8.0, 40.0)
            .parent(args.id)
            .set(args.state.ids.title, args.ui);
    }
}
