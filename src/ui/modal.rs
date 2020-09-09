use conrod_core::*;

#[derive(Debug, WidgetCommon)]
pub struct Modal<W> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    widget: W,
}

impl<W> Modal<W> {
    pub fn new(widget: W) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            widget,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        widget,
        canvas,
    }
}

impl<W> Widget for Modal<W>
where
    W: Widget,
{
    type State = Ids;
    type Style = Style;
    type Event = W::Event;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        widget::Canvas::new()
            .wh_of(args.id)
            .middle_of(args.id)
            .color(color::Color::Rgba(0., 0., 0., 0.9))
            .set(args.state.canvas, args.ui);

        self.widget
            .middle_of(args.state.canvas)
            .padded_wh_of(args.state.canvas, 256.0)
            .set(args.state.widget, args.ui)
    }
}
