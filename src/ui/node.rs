use conrod_core::*;

#[derive(Clone, WidgetCommon)]
pub struct Node {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    selected: bool,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

#[derive(Copy, Clone, Debug)]
pub enum Event {}

impl Node {
    pub fn new() -> Self {
        Node {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            selected: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

widget_ids! {
    pub struct Ids {
        rectangle
    }
}

impl Widget for Node {
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
        widget::BorderedRectangle::new(args.rect.dim())
            .parent(args.id)
            .border(if self.selected { 3.0 } else { 0.0 })
            .border_color(color::Color::Rgba(0.9, 0.8, 0.15, 1.0))
            .middle()
            .graphics_for(args.id)
            .set(args.state.rectangle, args.ui)
    }
}
