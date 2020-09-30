use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ExportRow {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
}

impl ExportRow {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {

}

impl Widget for ExportRow {
    type State = ();
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        ()
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        ()
    }
}
