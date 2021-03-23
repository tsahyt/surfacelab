use crate::lang::*;
use conrod_core::*;
use smallvec::SmallVec;
use std::collections::HashMap;

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
pub struct Style {
    #[conrod(default = "theme.label_color")]
    title_color: Option<Color>,
    #[conrod(default = "theme.font_size_medium")]
    title_size: Option<FontSize>,
    #[conrod(default = "theme.border_color")]
    border_color: Option<Color>,
    #[conrod(default = "color::ORANGE")]
    active_color: Option<Color>,
    #[conrod(default = "color::YELLOW")]
    selection_color: Option<Color>,
}

#[derive(Clone, Debug)]
pub enum Event {
    NodeDrag([f64; 2]),
    NodeDelete,
    NodeEnter,
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

    builder_methods! {
        pub selected { selected = SelectionState }
        pub title_color { style.title_color = Some(Color) }
        pub title_size { style.title_size = Some(FontSize) }
        pub border_color { style.border_color = Some(Color) }
        pub active_color { style.active_color = Some(Color) }
        pub selection_color { style.selection_color = Some(Color) }
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
    sockets_hash: u64,
}

impl State {
    pub fn renew_sockets(
        &mut self,
        id_gen: &mut widget::id::Generator,
        inputs: &[(String, OperatorType)],
        outputs: &[(String, OperatorType)],
    ) {
        let input_ids = self
            .input_sockets
            .values()
            .copied()
            .chain(std::iter::repeat_with(|| id_gen.next()));
        self.input_sockets = inputs
            .iter()
            .zip(input_ids)
            .map(|(s, i)| (s.0.clone(), i))
            .collect();

        let output_ids = self
            .output_sockets
            .values()
            .copied()
            .chain(std::iter::repeat_with(|| id_gen.next()));
        self.output_sockets = outputs
            .iter()
            .zip(output_ids)
            .map(|(s, i)| (s.0.clone(), i))
            .collect();

        self.sockets_hash = hash_sockets(inputs, outputs);
    }
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

fn hash_sockets(inputs: &[(String, OperatorType)], outputs: &[(String, OperatorType)]) -> u64 {
    use std::hash::*;

    let mut s = std::collections::hash_map::DefaultHasher::new();
    inputs.hash(&mut s);
    outputs.hash(&mut s);
    s.finish()
}

impl<'a> Widget for Node<'a> {
    type State = State;
    type Style = Style;
    type Event = SmallVec<[Event; 1]>;

    fn init_state(&self, mut id_gen: widget::id::Generator) -> Self::State {
        State {
            input_sockets: self
                .inputs
                .iter()
                .map(|(k, _)| (k.clone(), id_gen.next()))
                .collect(),
            output_sockets: self
                .outputs
                .iter()
                .map(|(k, _)| (k.clone(), id_gen.next()))
                .collect(),
            sockets_hash: hash_sockets(self.inputs, self.outputs),
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            rect,
            ..
        } = args;

        if hash_sockets(self.inputs, self.outputs) != state.sockets_hash {
            let mut id_gen = ui.widget_id_generator();
            state.update(|state| {
                state.renew_sockets(&mut id_gen, self.inputs, self.outputs);
            })
        }
        let mut evs = SmallVec::new();

        widget::BorderedRectangle::new(rect.dim())
            .parent(id)
            .border(3.0)
            .border_color(match self.selected {
                SelectionState::Active => style.active_color(&ui.theme),
                SelectionState::Selected => style.selection_color(&ui.theme),
                _ => style.border_color(&ui.theme),
            })
            .color(color::CHARCOAL)
            .middle()
            .graphics_for(id)
            .set(state.ids.rectangle, ui);

        widget::Text::new(self.title)
            .parent(id)
            .color(style.title_color(&ui.theme))
            .graphics_for(id)
            .font_size(style.title_size(&ui.theme))
            .mid_top()
            .up(2.0)
            .set(state.ids.title, ui);

        if let Some(thumbnail) = self.thumbnail {
            widget::Image::new(thumbnail)
                .parent(state.ids.rectangle)
                .middle()
                .padded_wh_of(state.ids.rectangle, 8.0)
                .graphics_for(id)
                .set(state.ids.thumbnail, ui);
        }

        let mut margin = 16.0;

        for (input, ty) in self.inputs.iter() {
            let w_id = state.input_sockets.get(input).copied().unwrap();
            widget::BorderedRectangle::new([16.0, 16.0])
                .border(3.0)
                .color(operator_type_color(ty, self.type_variables))
                .parent(state.ids.rectangle)
                .top_left_with_margins(margin, 0.0)
                .set(w_id, ui);

            let middle = ui.xy_of(w_id).unwrap();

            evs.extend(
                ui.widget_input(w_id)
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
                ui.widget_input(w_id)
                    .releases()
                    .map(|_| Event::SocketRelease(self.node_id)),
            );

            evs.extend(
                ui.widget_input(w_id)
                    .presses()
                    .mouse()
                    .button(input::MouseButton::Right)
                    .map(|_| Event::SocketClear(input.clone())),
            );

            margin += 32.0;
        }

        margin = 16.0;

        for (output, ty) in self.outputs.iter() {
            let w_id = state.output_sockets.get(output).copied().unwrap();
            widget::BorderedRectangle::new([16.0, 16.0])
                .border(3.0)
                .color(operator_type_color(ty, self.type_variables))
                .parent(state.ids.rectangle)
                .top_right_with_margins(margin, 0.0)
                .set(w_id, ui);

            let middle = ui.xy_of(w_id).unwrap();

            evs.extend(
                ui.widget_input(w_id)
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
                ui.widget_input(w_id)
                    .releases()
                    .map(|_| Event::SocketRelease(self.node_id)),
            );

            margin += 32.0;
        }

        // Node Dragging
        evs.extend(
            ui.widget_input(id)
                .drags()
                .button(input::MouseButton::Left)
                .map(|x| Event::NodeDrag(x.delta_xy)),
        );

        // Key events
        evs.extend(
            ui.widget_input(id)
                .presses()
                .key()
                .filter_map(|press| match press.key {
                    input::Key::X => Some(Event::NodeDelete),
                    input::Key::Tab => Some(Event::NodeEnter),
                    _ => None,
                }),
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
