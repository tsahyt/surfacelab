use super::node;
use conrod_core::*;
use std::collections::HashMap;

use crate::{
    lang::{Node, Resource, Socket},
    ui::app_state::{
        graph::{node_height, STANDARD_NODE_SIZE},
        GraphObject,
    },
};

const ZOOM_SENSITIVITY: f64 = 1.0 / 100.0;

#[derive(Clone, WidgetCommon)]
pub struct Graph<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    graph: &'a crate::ui::app_state::graph::Graph,
    style: Style,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {
    #[conrod(default = "color::LIGHT_GRAY")]
    edge_color: Option<Color>,
    #[conrod(default = "3.0")]
    edge_thickness: Option<Scalar>,
    #[conrod(default = "color::DARK_RED")]
    edge_drag_color: Option<Color>,
    #[conrod(default = "color::LIGHT_BLUE")]
    select_rect_color: Option<Color>,
    #[conrod(default = "theme.label_color")]
    node_title_color: Option<Color>,
    #[conrod(default = "theme.font_size_medium")]
    node_title_size: Option<FontSize>,
    #[conrod(default = "theme.border_color")]
    node_border_color: Option<Color>,
    #[conrod(default = "color::ORANGE")]
    node_active_color: Option<Color>,
    #[conrod(default = "color::YELLOW")]
    node_selection_color: Option<Color>,
}

widget_ids! {
    #[derive(Clone)]
    pub struct Ids {
        nodes[],
        connections[],
        grid,
        selection_rect,
        floating_noodle,
        highlight_noodle,
    }
}

pub struct State {
    ids: Ids,
    camera: Camera,
    selection: Selection,
    connection_draw: Option<ConnectionDraw>,
    socket_view: Option<Resource<Socket>>,
}

#[derive(Clone, Debug)]
pub enum Event {
    NodeDrag(Resource<Node>, Point, bool),
    ConnectionDrawn(Resource<Socket>, Resource<Socket>),
    SocketClear(Resource<Socket>),
    NodeDelete(Resource<Node>),
    NodeEnter(Resource<Node>),
    ActiveElement(Resource<Node>),
    AddModal(Point),
    Extract(Vec<Resource<Node>>),
    AlignNodes(Vec<Resource<Node>>),
    ExportSetup(Vec<Resource<Node>>),
    SocketView(Resource<Socket>),
    SocketViewClear,
}

impl<'a> Graph<'a> {
    pub fn new(graph: &'a crate::ui::app_state::graph::Graph) -> Self {
        Graph {
            common: widget::CommonBuilder::default(),
            graph,
            style: Style::default(),
        }
    }

    /// Handle the creation of selection via dragging a rectangle across nodes.
    /// Single click selection is handled during node creation where the widget
    /// ID is readily available.
    fn rect_selection_handling(
        &self,
        ui: &Ui,
        state: &'a mut widget::State<'_, State>,
        id: widget::Id,
    ) {
        for (to, origin) in ui.widget_input(id).drags().filter_map(|drag| match drag {
            event::Drag {
                button: input::MouseButton::Left,
                to,
                origin,
                ..
            } => Some((to, origin)),
            _ => None,
        }) {
            state.update(|state| {
                state.selection.set_geometry(origin, to);
            })
        }

        for release in ui
            .widget_input(id)
            .releases()
            .mouse()
            .filter(|mr| mr.button == input::MouseButton::Left)
        {
            if let Some(rect) = state.selection.rect {
                state.update(|state| {
                    let nodes = self.graph.nodes_in_envelope(
                        state.camera.inv_transform(rect.0),
                        state.camera.inv_transform(rect.1),
                    );
                    state
                        .selection
                        .set_selection(nodes, release.modifiers == input::ModifierKey::SHIFT);
                    state.selection.finish();
                })
            }
        }
    }

    /// Handle camera controls
    fn camera_handling(&self, ui: &Ui, state: &'a mut widget::State<'_, State>, id: widget::Id) {
        for delta_xy in ui.widget_input(id).drags().filter_map(|drag| match drag {
            event::Drag {
                button: input::MouseButton::Middle,
                delta_xy,
                ..
            } => Some(delta_xy),
            _ => None,
        }) {
            let [dx, dy] = state.camera.inv_scale(delta_xy);
            state.update(|state| {
                state.camera.pan(dx, dy);
            });
        }

        for dz in ui.widget_input(id).scrolls().map(|scroll| scroll.y) {
            state.update(|state| state.camera.zoom(dz));
        }
    }

    fn find_target_socket(&self, pos: Point) -> Option<Resource<Socket>> {
        let node = self.graph.nodes.get(self.graph.nearest_node_at(pos)?)?;
        let socket = node.socket_at_position(pos, 64.)?;
        Some(node.resource.node_socket(socket))
    }

    builder_methods! {
        pub edge_color { style.edge_color = Some(Color) }
        pub edge_thickness { style.edge_thickness = Some(Scalar) }
        pub edge_drag_color { style.edge_drag_color = Some(Color) }
        pub select_rect_color { style.select_rect_color = Some(Color) }
        pub node_title_color { style.node_title_color = Some(Color) }
        pub node_title_size { style.node_title_size = Some(FontSize) }
        pub node_border_color { style.node_border_color = Some(Color) }
        pub node_active_color { style.node_active_color = Some(Color) }
        pub node_selection_color { style.node_selection_color = Some(Color) }
    }
}

#[derive(Debug)]
enum DragOperation {
    Starting,
    Moving(Point, bool),
    Drop,
}

impl<'a> Widget for Graph<'a> {
    type State = State;
    type Style = Style;
    type Event = Vec<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            camera: Camera::default(),
            selection: Selection::default(),
            connection_draw: None,
            socket_view: None,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            id,
            state,
            ui,
            style,
            rect,
            ..
        } = args;
        let mut evs = Vec::new();

        // Update list sizes if required
        if state.ids.nodes.len() < self.graph.node_count {
            let mut id_gen = ui.widget_id_generator();
            state.update(|state| state.ids.nodes.resize(self.graph.node_count, &mut id_gen));
        }

        if state.ids.connections.len() < self.graph.connection_count {
            let mut id_gen = ui.widget_id_generator();
            state.update(|state| {
                state
                    .ids
                    .connections
                    .resize(self.graph.connection_count, &mut id_gen)
            });
        }

        // Update camera
        self.camera_handling(ui, state, id);

        // Update selection
        self.rect_selection_handling(ui, state, id);

        // Create Grid
        super::grid::Grid::new()
            .zoom(state.camera.zoom)
            .pan(state.camera.position)
            .wh_of(id)
            .middle()
            .parent(id)
            .graphics_for(id)
            .set(state.ids.grid, ui);

        let mut drag_operation = None;

        // Create widgets for all graph objects
        let mut node_i = 0;
        let mut connection_i = 0;

        let (corner_1, corner_2) = state.camera.viewport(rect.w(), rect.h());

        for gobj in self.graph.drawables_in_envelope(corner_1, corner_2) {
            match gobj {
                GraphObject::Node { resource, .. } => {
                    let w_id = state.ids.nodes[node_i];
                    node_i += 1;

                    let node = &self.graph.nodes[resource];

                    for press in ui
                        .widget_input(w_id)
                        .presses()
                        .mouse()
                        .button(input::MouseButton::Left)
                    {
                        state.update(|state| {
                            if press.1 == input::ModifierKey::SHIFT {
                                state.selection.add(node.resource.clone());
                            }
                            state.selection.set_active(Some(node.resource.clone()))
                        });
                        evs.push(Event::ActiveElement(node.resource.clone()));
                    }

                    let selection_state = if state.selection.is_active(&node.resource) {
                        node::SelectionState::Active
                    } else if state.selection.is_selected(&node.resource) {
                        node::SelectionState::Selected
                    } else {
                        node::SelectionState::None
                    };
                    let view_socket = state.socket_view.as_ref().and_then(|socket| {
                        if socket.is_socket_of(&node.resource) {
                            socket.fragment().map(|x| x.to_string())
                        } else {
                            None
                        }
                    });
                    let socket_count = node.inputs.len().max(node.outputs.len());

                    for ev in node::Node::new(
                        &node.type_variables,
                        &node.inputs,
                        &node.outputs,
                        &node.title,
                    )
                    .title_color(style.node_title_color(&ui.theme))
                    .title_size(style.node_title_size(&ui.theme))
                    .selected(selection_state)
                    .view_socket(view_socket)
                    .active_color(style.node_active_color(&ui.theme))
                    .selection_color(style.node_selection_color(&ui.theme))
                    .parent(id)
                    .xy_relative_to(id, state.camera.transform(node.position))
                    .thumbnail(node.thumbnail)
                    .wh([
                        STANDARD_NODE_SIZE * state.camera.zoom,
                        node_height(socket_count, 16., 8.) * state.camera.zoom,
                    ])
                    .zoom(state.camera.zoom)
                    .set(w_id, ui)
                    {
                        match ev {
                            node::Event::NodeDragStart => {
                                drag_operation = Some(DragOperation::Starting);
                            }
                            node::Event::NodeDragMotion(delta, tmp_snap) => {
                                drag_operation = Some(DragOperation::Moving(delta, tmp_snap));
                            }
                            node::Event::NodeDragStop => {
                                drag_operation = Some(DragOperation::Drop);
                            }
                            node::Event::NodeDelete => {
                                evs.push(Event::NodeDelete(node.resource.clone()));
                            }
                            node::Event::NodeEnter => {
                                evs.push(Event::NodeEnter(node.resource.clone()));
                            }
                            node::Event::SocketDrag(from, to) => {
                                state.update(|state| {
                                    state.connection_draw = Some(ConnectionDraw { from, to })
                                });
                            }
                            node::Event::SocketRelease(source, node::SocketType::Source) => {
                                if let Some(draw) = &state.connection_draw {
                                    let pos = state.camera.inv_transform([
                                        draw.to[0] - rect.xy()[0],
                                        draw.to[1] - rect.xy()[1],
                                    ]);
                                    if let Some(sink) = self.find_target_socket(pos) {
                                        evs.push(Event::ConnectionDrawn(
                                            node.resource.node_socket(&source),
                                            sink,
                                        ))
                                    }
                                }
                                state.update(|state| {
                                    state.connection_draw = None;
                                });
                            }
                            node::Event::SocketRelease(sink, node::SocketType::Sink) => {
                                if let Some(draw) = &state.connection_draw {
                                    let pos = state.camera.inv_transform([
                                        draw.from[0] - rect.xy()[0],
                                        draw.from[1] - rect.xy()[1],
                                    ]);
                                    if let Some(source) = self.find_target_socket(pos) {
                                        evs.push(Event::ConnectionDrawn(
                                            source,
                                            node.resource.node_socket(&sink),
                                        ))
                                    }
                                }
                                state.update(|state| {
                                    state.connection_draw = None;
                                });
                            }
                            node::Event::SocketClear(socket) => {
                                evs.push(Event::SocketClear(node.resource.node_socket(&socket)))
                            }
                            node::Event::SocketView(socket) => {
                                let res = node.resource.node_socket(&socket);
                                if state
                                    .socket_view
                                    .as_ref()
                                    .map(|s| s == &res)
                                    .unwrap_or(false)
                                {
                                    state.update(|state| state.socket_view = None);
                                    evs.push(Event::SocketViewClear)
                                } else {
                                    state.update(|state| state.socket_view = Some(res.clone()));
                                    evs.push(Event::SocketView(res))
                                }
                            }
                        }
                    }
                }
                GraphObject::Connection { from, to, .. } => {
                    let w_id = state.ids.connections[connection_i];
                    connection_i += 1;

                    draw_noodle(
                        rect,
                        id,
                        ui,
                        &state.camera,
                        w_id,
                        *from,
                        *to,
                        style.edge_color(&ui.theme),
                        (style.edge_thickness(&ui.theme) * state.camera.zoom).clamp(1.5, 8.),
                        true,
                    );
                }
            }
        }

        // Handle operations on the selection
        for press in ui.widget_input(id).presses().key() {
            match (press.key, press.modifiers) {
                (input::Key::X, input::ModifierKey::NO_MODIFIER) => {
                    evs.extend(
                        state
                            .selection
                            .get_selection()
                            .cloned()
                            .map(|res| Event::NodeDelete(res)),
                    );
                }
                (input::Key::G, input::ModifierKey::CTRL) => {
                    evs.push(Event::Extract(
                        state.selection.get_selection().cloned().collect(),
                    ));
                }
                (input::Key::E, input::ModifierKey::NO_MODIFIER) => {
                    evs.push(Event::ExportSetup(
                        state.selection.get_selection().cloned().collect(),
                    ));
                }
                (input::Key::A, input::ModifierKey::NO_MODIFIER) => {
                    evs.push(Event::AlignNodes(
                        state.selection.get_selection().cloned().collect(),
                    ));
                }
                _ => {}
            }
        }

        // Dragging of nodes processed separately to apply operation to the
        // entire selection set
        match drag_operation {
            Some(DragOperation::Starting) => state.update(|state| {
                state.selection.start_drag(|res| {
                    self.graph
                        .nodes
                        .get(res)
                        .map(|node_data| node_data.position)
                        .unwrap_or([0., 0.])
                })
            }),

            Some(DragOperation::Moving(delta, tmp_snap)) => {
                state.update(|state| state.selection.drag(state.camera.inv_scale(delta)));
                evs.extend(
                    state
                        .selection
                        .current_drag_positions()
                        .filter_map(|(res, pos)| {
                            pos.map(|p| Event::NodeDrag(res.clone(), p, tmp_snap))
                        }),
                );

                // Draw highlight noodle on nearest connection if appropriate
                if let Some(pos) = state.selection.get_active_current_drag_pos() {
                    if let Some(GraphObject::Connection {
                        from,
                        to,
                        source,
                        sink,
                        ..
                    }) = self.graph.nearest_connection_at(pos)
                    {
                        if !(source.is_socket_of(state.selection.get_active().unwrap())
                            || sink.is_socket_of(state.selection.get_active().unwrap()))
                            && noodle_distance(pos, *from, *to) < 64.
                        {
                            draw_noodle(
                                rect,
                                id,
                                ui,
                                &state.camera,
                                state.ids.highlight_noodle,
                                *from,
                                *to,
                                color::RED.alpha(0.5),
                                (style.edge_thickness(&ui.theme) * state.camera.zoom)
                                    .clamp(1.5, 8.)
                                    * 3.,
                                true,
                            );
                        }
                    }
                }
            }

            Some(DragOperation::Drop) => {
                state.update(|state| state.selection.stop_drag());
            }

            None => {}
        }

        // Draw selection rectangle if actively selecting
        if let Selection {
            rect: Some((from, to)),
            ..
        } = &state.selection
        {
            widget::BorderedRectangle::new(Rect::from_corners(*from, *to).dim())
                .xy_relative_to(
                    id,
                    [
                        from[0] + (to[0] - from[0]) / 2.0,
                        from[1] + (to[1] - from[1]) / 2.0,
                    ],
                )
                .parent(id)
                .color(style.select_rect_color(&ui.theme).alpha(0.2))
                .border_color(style.select_rect_color(&ui.theme))
                .set(state.ids.selection_rect, ui);
        }

        // Draw floating noodle if currently drawing a connection
        if let Some(ConnectionDraw { from, to, .. }) = &state.connection_draw {
            draw_noodle(
                rect,
                id,
                ui,
                &state.camera,
                state.ids.floating_noodle,
                *from,
                *to,
                style.edge_drag_color(&ui.theme),
                (style.edge_thickness(&ui.theme) * state.camera.zoom).clamp(1.5, 6.),
                false,
            );
        }

        // Trigger Add Modal
        evs.extend(
            ui.widget_input(id)
                .clicks()
                .button(input::MouseButton::Right)
                .map(|c| Event::AddModal(state.camera.inv_transform(c.xy))),
        );
        if rect.is_over(ui.global_input().current.mouse.xy) {
            evs.extend(ui.global_input().events().ui().find_map(|x| match x {
                event::Ui::Press(
                    _,
                    event::Press {
                        button: event::Button::Keyboard(input::Key::A),
                        modifiers: input::ModifierKey::CTRL,
                    },
                ) => Some(Event::AddModal(state.camera.inv_transform([0., 0.]))),
                _ => None,
            }));
        }

        evs
    }
}

/// Helper function for drawing noodles
fn draw_noodle(
    rect: Rect,
    id: widget::Id,
    ui: &mut UiCell,
    camera: &Camera,
    w_id: widget::Id,
    from: Point,
    to: Point,
    color: color::Color,
    thickness: f64,
    transform: bool,
) {
    let rect_xy = rect.xy();
    let from_view = if transform {
        let transformed = camera.transform(from);
        [transformed[0] + rect_xy[0], transformed[1] + rect_xy[1]]
    } else {
        from
    };
    let to_view = if transform {
        let transformed = camera.transform(to);
        [transformed[0] + rect_xy[0], transformed[1] + rect_xy[1]]
    } else {
        to
    };

    let dist = (from_view[0] - to_view[0]).abs();
    super::bezier::Bezier::abs(
        from_view,
        [from_view[0] + dist / 2., from_view[1]],
        to_view,
        [to_view[0] - dist / 2., to_view[1]],
    )
    .thickness(thickness)
    .color(color)
    .graphics_for(id)
    .middle()
    .parent(id)
    .depth(1.0)
    .set(w_id, ui);
}

/// Helper function to determine distance to bezier. This function assumes
/// canvas coordinates!
fn noodle_distance(p: Point, from: Point, to: Point) -> f64 {
    let dist = (from[0] - to[0]).abs();
    super::bezier::approx_bezier_distance(
        p,
        from,
        [from[0] + dist / 2., from[1]],
        [to[0] - dist / 2., to[1]],
        to,
    )
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera {
    position: Point,
    zoom: Scalar,
}

impl Camera {
    pub fn transform(&self, point: Point) -> Point {
        [
            self.zoom * (point[0] + self.position[0]),
            self.zoom * (point[1] + self.position[1]),
        ]
    }

    pub fn inv_scale(&self, point: Point) -> Point {
        [point[0] / self.zoom, point[1] / self.zoom]
    }

    pub fn inv_transform(&self, point: Point) -> Point {
        [
            (point[0] / self.zoom) - self.position[0],
            (point[1] / self.zoom) - self.position[1],
        ]
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.position[0] += dx;
        self.position[1] += dy;
    }

    pub fn zoom(&mut self, dz: f64) {
        self.zoom = (self.zoom * (1.0 - (dz * ZOOM_SENSITIVITY))).clamp(0.2, 4.0);
    }

    /// Return corners of the currently visible viewport
    pub fn viewport(&self, width: Scalar, height: Scalar) -> (Point, Point) {
        let width = width / self.zoom;
        let height = height / self.zoom;

        let x_min = -self.position[0] - width / 2.;
        let x_max = -self.position[0] + width / 2.;
        let y_min = -self.position[1] - height / 2.;
        let y_max = -self.position[1] + height / 2.;

        ([x_min, y_min], [x_max, y_max])
    }
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            position: [0.0, 0.0],
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Selected {
    drag_start: Option<Point>,
    drag_delta: Option<Point>,
}

impl Default for Selected {
    fn default() -> Self {
        Self {
            drag_start: None,
            drag_delta: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Selection {
    rect: Option<(Point, Point)>,
    set: HashMap<Resource<Node>, Selected>,
    active: Option<Resource<Node>>,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            rect: None,
            set: HashMap::new(),
            active: None,
        }
    }
}

impl Selection {
    pub fn set_geometry(&mut self, from: Point, to: Point) {
        self.rect = Some((from, to))
    }

    pub fn add(&mut self, node: Resource<Node>) {
        self.set.insert(node, Selected::default());
    }

    pub fn get_geometry(&mut self) -> Option<(Point, Point)> {
        self.rect
    }

    pub fn set_active(&mut self, active: Option<Resource<Node>>) {
        if let Some(active) = active.as_ref() {
            if !self.is_selected(active) {
                self.set.clear();
            }

            self.set.insert(active.clone(), Selected::default());
        }

        self.active = active;
    }

    pub fn get_active(&self) -> Option<&Resource<Node>> {
        self.active.as_ref()
    }

    pub fn get_active_current_drag_pos(&self) -> Option<Point> {
        let selected = self.set.get(self.active.as_ref()?)?;
        selected
            .drag_start
            .and_then(|[px, py]| selected.drag_delta.map(|[dx, dy]| [px + dx, py + dy]))
    }

    pub fn is_active(&self, node: &Resource<Node>) -> bool {
        self.active.as_ref() == Some(node)
    }

    pub fn finish(&mut self) {
        self.rect = None
    }

    pub fn set_selection<'a, I>(&mut self, selection: I, adding: bool)
    where
        I: Iterator<Item = &'a Resource<Node>>,
    {
        if adding {
            self.set
                .extend(selection.map(|x| (x.clone(), Selected::default())));
        } else {
            self.set = selection
                .map(|x| (x.clone(), Selected::default()))
                .collect();
        }
        self.set_active(None);
    }

    pub fn get_selection(&self) -> impl Iterator<Item = &Resource<Node>> {
        self.set.keys()
    }

    pub fn is_selected(&self, node: &Resource<Node>) -> bool {
        self.set.get(node).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Start the drag operation, given a closure to determine the original
    /// position of each node in the selection set.
    pub fn start_drag<F: Fn(&Resource<Node>) -> Point>(&mut self, original_position: F) {
        for (res, selected) in self.set.iter_mut() {
            let pos = original_position(res);
            selected.drag_start = Some(pos);
            selected.drag_delta = Some([0., 0.]);
        }
    }

    /// Drag the entire selection set by a delta
    pub fn drag(&mut self, delta: Point) {
        for selected in self.set.values_mut() {
            selected.drag_delta = selected
                .drag_delta
                .map(|[x, y]| [x + delta[0], y + delta[1]]);
        }
    }

    /// Get the current dragging positions of the selection set, i.e. the
    /// original positions plus the accumulated deltas.
    pub fn current_drag_positions(&self) -> impl Iterator<Item = (&Resource<Node>, Option<Point>)> {
        self.set.iter().map(|(res, selected)| {
            (
                res,
                selected
                    .drag_start
                    .and_then(|[px, py]| selected.drag_delta.map(|[dx, dy]| [px + dx, py + dy])),
            )
        })
    }

    /// Stop a drag operation, resetting all stored positions for the whole selection set.
    pub fn stop_drag(&mut self) {
        for selected in self.set.values_mut() {
            selected.drag_start = None;
            selected.drag_delta = None;
        }
    }
}

#[derive(Clone)]
pub struct ConnectionDraw {
    from: Point,
    to: Point,
}
