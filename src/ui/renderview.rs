use conrod_core::*;
use conrod_derive::*;

#[derive(Copy, Clone, WidgetCommon)]
pub struct RenderView {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    image_id: image::Id,
    style: Style,
}

#[derive(Copy, Clone)]
pub struct State {}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        image
    }
}

impl RenderView {
    /// Begin building a `RenderView`.
    pub fn new(image_id: image::Id) -> Self {
        RenderView {
            common: widget::CommonBuilder::default(),
            image_id,
            style: Style::default(),
        }
    }
}

impl Widget for RenderView {
    type State = Ids;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            state,
            rect,
            id,
            ui,
            ..
        } = args;
        let image_id = self.image_id;

        let (x, y, w, h) = rect.x_y_w_h();
        let image = widget::Image::new(image_id)
            .x_y(x, y)
            .w_h(w, h)
            .parent(id)
            .graphics_for(id)
            .set(state.image, ui);
    }
}
