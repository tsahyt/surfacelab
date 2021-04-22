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
        Self::new(
            widget::Canvas::new()
                .color(color::DARK_CHARCOAL)
                .scroll_kids_vertically(),
        )
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

    builder_methods! {
        pub padding { style.padding = Some(Scalar) }
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
        let widget::UpdateArgs {
            id,
            ui,
            state,
            style,
            rect,
            ..
        } = args;

        widget::Canvas::new()
            .border(0.0)
            .wh_of(id)
            .middle_of(id)
            .color(color::Color::Rgba(0., 0., 0., 0.9))
            .set(state.canvas, ui);

        if rect.is_over(ui.global_input().current.mouse.xy) {
            for ev in ui.global_input().events().ui() {
                match ev {
                    event::Ui::Press(
                        _,
                        event::Press {
                            button: event::Button::Keyboard(input::Key::Escape),
                            ..
                        },
                    ) => {
                        return Event::Hide;
                    }
                    _ => {}
                }
            }
        }

        if ui.widget_input(state.canvas).clicks().next().is_some() {
            return Event::Hide;
        }

        let ev = self
            .widget
            .middle_of(state.canvas)
            .padded_wh_of(state.canvas, style.padding(&ui.theme))
            .set(state.widget, ui);

        Event::ChildEvent((ev, state.widget))
    }
}
