use conrod_core::*;

#[derive(Debug, WidgetCommon)]
pub struct Modal<W> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    widget: W,
}

impl Modal<widget::Canvas<'_>> {
    pub fn canvas() -> Self {
        Self::new(widget::Canvas::new().color(color::DARK_CHARCOAL))
    }
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
pub struct Style {
    #[conrod(default = "256.0")]
    padding: Option<Scalar>,
}

widget_ids! {
    pub struct Ids {
        widget,
        canvas,
    }
}

pub enum Event<W>
where
    W: Widget,
{
    ChildEvent((W::Event, widget::Id)),
    Hide,
}

impl<W> Widget for Modal<W>
where
    W: Widget,
{
    type State = Ids;
    type Style = Style;
    type Event = Event<W>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        widget::Canvas::new()
            .border(0.0)
            .wh_of(args.id)
            .middle_of(args.id)
            .color(color::Color::Rgba(0., 0., 0., 0.9))
            .set(args.state.canvas, args.ui);

        if args.ui.widget_input(args.state.canvas).clicks().next().is_some() {
            return Event::Hide;
        }

        let ev = self
            .widget
            .middle_of(args.state.canvas)
            .padded_wh_of(args.state.canvas, self.style.padding.unwrap_or(256.0))
            .set(args.state.widget, args.ui);

        Event::ChildEvent((ev, args.state.widget))
    }
}
