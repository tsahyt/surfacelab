use crate::lang::*;
use conrod_core::*;
use std::collections::HashMap;
use std::iter::FromIterator;

#[derive(Clone, WidgetCommon)]
pub struct Node<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    selected: bool,
    thumbnail: Option<image::Id>,
    operator: &'a Operator,
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
            thumbnail: None,
            operator,
        }
    }

    pub fn thumbnail(mut self, thumbnail: Option<image::Id>) -> Self {
        self.thumbnail = thumbnail;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

widget_ids! {
    pub struct Ids {
        rectangle,
        thumbnail,
        title,
    }
}

pub struct State {
    ids: Ids,
    input_sockets: HashMap<String, widget::Id>,
    output_sockets: HashMap<String, widget::Id>,
}

pub fn socket_rect(ui: &Ui, node_id: widget::Id, socket: &str) -> Option<Rect> {
    let unique = ui
        .widget_graph()
        .widget(node_id)?
        .state_and_style::<State, Style>()?;
    let mut sockets = unique
        .state
        .input_sockets
        .iter()
        .chain(unique.state.output_sockets.iter());
    let result = sockets
        .find(|(name, _)| name.as_str() == socket)
        .map(|x| x.1)?;
    ui.rect_of(*result)
}

impl<'a> Widget for Node<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, mut id_gen: widget::id::Generator) -> Self::State {
        State {
            input_sockets: HashMap::from_iter(
                self.operator
                    .inputs()
                    .iter()
                    .map(|(k, _)| (k.clone(), id_gen.next())),
            ),
            output_sockets: HashMap::from_iter(
                self.operator
                    .outputs()
                    .iter()
                    .map(|(k, _)| (k.clone(), id_gen.next())),
            ),
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let state = args.state;

        widget::BorderedRectangle::new(args.rect.dim())
            .parent(args.id)
            .border(3.0)
            .border_color(if self.selected {
                color::Color::Rgba(0.9, 0.8, 0.15, 1.0)
            } else {
                color::BLACK
            })
            .color(color::CHARCOAL)
            .middle()
            .graphics_for(args.id)
            .set(state.ids.rectangle, args.ui);

        widget::Text::new(self.operator.title())
            .parent(args.id)
            .color(color::LIGHT_CHARCOAL)
            .graphics_for(args.id)
            .font_size(14)
            .mid_top()
            .up(2.0)
            .set(state.ids.title, args.ui);

        if let Some(thumbnail) = self.thumbnail {
            widget::Image::new(thumbnail)
                .parent(state.ids.rectangle)
                .middle()
                .padded_wh_of(state.ids.rectangle, 8.0)
                .set(state.ids.thumbnail, args.ui);
        }

        let mut margin = 16.0;

        for (input, ty) in self.operator.inputs().iter() {
            widget::BorderedRectangle::new([16.0, 16.0])
                .border(3.0)
                .color(operator_type_color(ty))
                .parent(state.ids.rectangle)
                .top_left_with_margins(margin, 0.0)
                .set(*state.input_sockets.get(input).unwrap(), args.ui);

            margin += 32.0;
        }

        margin = 16.0;

        for (output, ty) in self.operator.outputs().iter() {
            widget::BorderedRectangle::new([16.0, 16.0])
                .border(3.0)
                .color(operator_type_color(ty))
                .parent(state.ids.rectangle)
                .top_right_with_margins(margin, 0.0)
                .set(*state.output_sockets.get(output).unwrap(), args.ui);

            margin += 32.0;
        }
    }
}

fn operator_type_color(optype: &OperatorType) -> color::Color {
    match optype {
        OperatorType::Monomorphic(ImageType::Grayscale) => color::LIGHT_GREEN,
        OperatorType::Monomorphic(ImageType::Rgb) => color::LIGHT_ORANGE,
        OperatorType::Polymorphic(_) => color::DARK_RED,
    }
}
