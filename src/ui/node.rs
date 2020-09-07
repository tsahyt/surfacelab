use crate::lang::*;
use conrod_core::*;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::iter::FromIterator;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SelectionState {
    Selected,
    Active,
    None,
}

#[derive(Clone, Debug, WidgetCommon)]
pub struct Node<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    node_id: petgraph::graph::NodeIndex,
    style: Style,
    selected: SelectionState,
    thumbnail: Option<image::Id>,
    inputs: &'a [(String, OperatorType)],
    outputs: &'a [(String, OperatorType)],
    title: &'a str,
    type_variables: &'a HashMap<TypeVariable, ImageType>,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

#[derive(Clone, Debug)]
pub enum Event {
    NodeDrag([f64; 2]),
    SocketDrag(Point, Point),
    SocketClear(String),
    SocketRelease(petgraph::graph::NodeIndex),
}

impl<'a> Node<'a> {
    pub fn new(
        node_id: petgraph::graph::NodeIndex,
        type_variables: &'a HashMap<TypeVariable, ImageType>,
        inputs: &'a [(String, OperatorType)],
        outputs: &'a [(String, OperatorType)],
        title: &'a str,
    ) -> Self {
        Node {
            common: widget::CommonBuilder::default(),
            node_id,
            style: Style::default(),
            selected: SelectionState::None,
            thumbnail: None,
            inputs,
            outputs,
            title,
            type_variables,
        }
    }

    pub fn thumbnail(mut self, thumbnail: Option<image::Id>) -> Self {
        self.thumbnail = thumbnail;
        self
    }

    pub fn selected(mut self, selected: SelectionState) -> Self {
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

pub fn target_socket(ui: &Ui, node_id: widget::Id, point: Point) -> Option<&str> {
    let unique = ui
        .widget_graph()
        .widget(node_id)?
        .state_and_style::<State, Style>()?;
    for socket in unique
        .state
        .input_sockets
        .iter()
        .chain(unique.state.output_sockets.iter())
    {
        let rect = ui.rect_of(*socket.1).unwrap();

        if rect.x.is_over(point[0]) && rect.y.is_over(point[1]) {
            return Some(socket.0);
        }
    }

    None
}

impl<'a> Widget for Node<'a> {
    type State = State;
    type Style = Style;
    type Event = SmallVec<[Event; 1]>;

    fn init_state(&self, mut id_gen: widget::id::Generator) -> Self::State {
        State {
            input_sockets: HashMap::from_iter(
                self.inputs.iter().map(|(k, _)| (k.clone(), id_gen.next())),
            ),
            output_sockets: HashMap::from_iter(
                self.outputs.iter().map(|(k, _)| (k.clone(), id_gen.next())),
            ),
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let state = args.state;
        let mut evs = SmallVec::new();

        widget::BorderedRectangle::new(args.rect.dim())
            .parent(args.id)
            .border(3.0)
            .border_color(match self.selected {
                SelectionState::Active => color::Color::Rgba(0.9, 0.4, 0.15, 1.0),
                SelectionState::Selected => color::Color::Rgba(0.9, 0.8, 0.15, 1.0),
                _ => color::BLACK,
            })
            .color(color::CHARCOAL)
            .middle()
            .graphics_for(args.id)
            .set(state.ids.rectangle, args.ui);

        widget::Text::new(self.title)
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
                .graphics_for(args.id)
                .set(state.ids.thumbnail, args.ui);
        }

        let mut margin = 16.0;

        for (input, ty) in self.inputs.iter() {
            let w_id = state.input_sockets.get(input).unwrap();
            widget::BorderedRectangle::new([16.0, 16.0])
                .border(3.0)
                .color(operator_type_color(ty, self.type_variables))
                .parent(state.ids.rectangle)
                .top_left_with_margins(margin, 0.0)
                .set(*w_id, args.ui);

            let middle = args.ui.xy_of(*w_id).unwrap();

            evs.extend(
                args.ui
                    .widget_input(*w_id)
                    .drags()
                    .button(input::MouseButton::Left)
                    .map(|x| {
                        Event::SocketDrag(
                            middle,
                            [
                                middle[0] + x.total_delta_xy[0],
                                middle[1] + x.total_delta_xy[1],
                            ],
                        )
                    }),
            );

            evs.extend(
                args.ui
                    .widget_input(*w_id)
                    .releases()
                    .map(|_| Event::SocketRelease(self.node_id)),
            );

            evs.extend(
                args.ui
                    .widget_input(*w_id)
                    .presses()
                    .mouse()
                    .button(input::MouseButton::Right)
                    .map(|_| Event::SocketClear(input.clone())),
            );

            margin += 32.0;
        }

        margin = 16.0;

        for (output, ty) in self.outputs.iter() {
            let w_id = state.output_sockets.get(output).unwrap();
            widget::BorderedRectangle::new([16.0, 16.0])
                .border(3.0)
                .color(operator_type_color(ty, self.type_variables))
                .parent(state.ids.rectangle)
                .top_right_with_margins(margin, 0.0)
                .set(*w_id, args.ui);

            let middle = args.ui.xy_of(*w_id).unwrap();

            evs.extend(
                args.ui
                    .widget_input(*w_id)
                    .drags()
                    .button(input::MouseButton::Left)
                    .map(|x| {
                        Event::SocketDrag(
                            middle,
                            [
                                middle[0] + x.total_delta_xy[0],
                                middle[1] + x.total_delta_xy[1],
                            ],
                        )
                    }),
            );

            evs.extend(
                args.ui
                    .widget_input(*w_id)
                    .releases()
                    .map(|_| Event::SocketRelease(self.node_id)),
            );

            margin += 32.0;
        }

        // Node Dragging
        evs.extend(
            args.ui
                .widget_input(args.id)
                .drags()
                .button(input::MouseButton::Left)
                .map(|x| Event::NodeDrag(x.delta_xy)),
        );

        evs
    }
}

fn operator_type_color(
    optype: &OperatorType,
    variables: &HashMap<TypeVariable, ImageType>,
) -> color::Color {
    match optype {
        OperatorType::Monomorphic(ImageType::Grayscale) => color::LIGHT_GREEN,
        OperatorType::Monomorphic(ImageType::Rgb) => color::LIGHT_ORANGE,
        OperatorType::Polymorphic(v) => match variables.get(v) {
            Some(ImageType::Grayscale) => color::LIGHT_GREEN,
            Some(ImageType::Rgb) => color::LIGHT_ORANGE,
            None => match v {
                0 => color::DARK_RED,
                1 => color::DARK_ORANGE,
                2 => color::DARK_PURPLE,
                _ => color::DARK_BLUE,
            },
        },
    }
}
