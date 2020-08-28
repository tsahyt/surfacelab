use conrod_core::*;

#[derive(Clone, WidgetCommon)]
pub struct Node {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
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
        }
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
        widget::Rectangle::fill(args.rect.dim())
            .parent(args.id)
            .middle()
            .graphics_for(args.id)
            .set(args.state.rectangle, args.ui)
    }
}
