use crate::lang::*;
use conrod_core::*;

#[derive(Clone, WidgetCommon)]
pub struct Node<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    selected: bool,
    operator: &'a Operator
}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

#[derive(Copy, Clone, Debug)]
pub enum Event {}

impl<'a> Node<'a> {
    pub fn new(operator: &'a Operator) -> Self {
        Node {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            selected: false,
            operator
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

widget_ids! {
    pub struct Ids {
        rectangle,
        title
    }
}

impl<'a> Widget for Node<'a> {
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
            .set(args.state.rectangle, args.ui);

        widget::Text::new(self.operator.title())
            .parent(args.id)
            .graphics_for(args.id)
            .mid_top()
            .set(args.state.title, args.ui);
    }
}
