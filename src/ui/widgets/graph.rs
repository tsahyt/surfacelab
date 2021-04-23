use conrod_core::*;
use rstar::RTree;
use std::collections::{HashMap, VecDeque};

use crate::ui::app_state::GraphObject;

const STANDARD_NODE_SIZE: f64 = 128.0;
const ZOOM_SENSITIVITY: f64 = 1.0 / 100.0;

#[derive(Clone, WidgetCommon)]
pub struct Graph<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    rtree: &'a RTree<GraphObject>,
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
        floating_noodle
    }
}

pub struct State {
    ids: Ids,
    camera: Camera,
    selection: Selection,
    connection_draw: Option<ConnectionDraw>,
    socket_view: Option<(petgraph::graph::NodeIndex, String)>,
}

#[derive(Clone, Debug)]
pub enum Event {
    NodeDrag(petgraph::graph::NodeIndex, Scalar, Scalar, bool),
    ConnectionDrawn(
        petgraph::graph::NodeIndex,
        String,
        petgraph::graph::NodeIndex,
        String,
    ),
    SocketClear(petgraph::graph::NodeIndex, String),
    NodeDelete(petgraph::graph::NodeIndex),
    NodeEnter(petgraph::graph::NodeIndex),
    ActiveElement(petgraph::graph::NodeIndex),
    AddModal(Point),
    Extract(Vec<petgraph::graph::NodeIndex>),
    AlignNodes(Vec<petgraph::graph::NodeIndex>),
    ExportSetup(Vec<petgraph::graph::NodeIndex>),
    SocketView(petgraph::graph::NodeIndex, String),
    SocketViewClear,
}

impl<'a> Graph<'a> {
    pub fn new(rtree: &'a RTree<GraphObject>) -> Self {
        Graph {
            common: widget::CommonBuilder::default(),
            rtree,
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
            // state.update(|state| {
            //     let mut selected: Vec<_> = self
            //         .graph
            //         .node_indices()
            //         .filter_map(|idx| {
            //             if state.selection.geometry_contains(
            //                 state
            //                     .camera
            //                     .transform(self.graph.node_weight(idx).unwrap().position),
            //             ) {
            //                 Some(*state.node_ids.get(&idx).unwrap())
            //             } else {
            //                 None
            //             }
            //         })
            //         .collect();
            //     state.selection.set_selection(
            //         selected.drain(0..),
            //         release.modifiers == input::ModifierKey::SHIFT,
            //     );
            //     state.selection.finish();
            // })
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

    fn find_target_socket(
        &self,
        ui: &Ui,
        state: &State,
        drop_point: Point,
    ) -> Option<(petgraph::graph::NodeIndex, String)> {
        // self.graph.node_indices().find_map(|idx| {
        //     let w_id = state.node_ids.get(&idx)?;
        //     let socket = super::node::target_socket(ui, *w_id, drop_point)?;
        //     Some((idx, socket.to_string()))
        // })
        None
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

enum SelectionOperation {
    Delete,
    Extract,
    Align,
    ExportSetup,
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
    type Event = VecDeque<Event>;

    fn init_state(&self, mut id_gen: widget::id::Generator) -> Self::State {
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
        let mut evs = VecDeque::new();

        // // We collect the new nodes into a SmallVec that will spill after 4
        // // elements. This should be plenty, since updates should arrive slowly
        // // anyhow, unless we're rebuilding the whole graph on load, in which
        // // case the allocation is acceptable.
        // let new_nodes: SmallVec<[_; 4]> = self
        //     .graph
        //     .node_indices()
        //     .filter(|x| state.node_ids.get(&x).is_none())
        //     .collect();

        // for idx in new_nodes {
        //     state.update(|state| {
        //         state.node_ids.insert(idx, ui.widget_id_generator().next());
        //     })
        // }

        // // Now repeat the same thing for edges. Note that we don't have to do
        // // the deletion here, because all edges are the same in terms of
        // // internal state, since they're just simple primitives.
        // let new_edges: SmallVec<[_; 4]> = self
        //     .graph
        //     .edge_indices()
        //     .filter(|x| state.edge_ids.get(&x).is_none())
        //     .collect();

        // for idx in new_edges {
        //     state.update(|state| {
        //         state.edge_ids.insert(idx, ui.widget_id_generator().next());
        //     })
        // }

        // Update camera
        self.camera_handling(ui, state, id);

        // // Update selection
        // self.rect_selection_handling(ui, state, id);

        // Create Grid
        super::grid::Grid::new()
            .zoom(state.camera.zoom)
            .pan(state.camera.position)
            .wh_of(id)
            .middle()
            .parent(id)
            .graphics_for(id)
            .set(state.ids.grid, ui);

        // let mut drag_operation = None;

        // // Handle selection operation events
        // let selection_op =
        //     ui.widget_input(id)
        //         .presses()
        //         .key()
        //         .find_map(|x| match (x.key, x.modifiers) {
        //             (input::Key::X, input::ModifierKey::NO_MODIFIER) => {
        //                 Some(SelectionOperation::Delete)
        //             }
        //             (input::Key::G, input::ModifierKey::CTRL) => Some(SelectionOperation::Extract),
        //             (input::Key::E, input::ModifierKey::NO_MODIFIER) => {
        //                 Some(SelectionOperation::ExportSetup)
        //             }
        //             (input::Key::A, input::ModifierKey::NO_MODIFIER) => {
        //                 Some(SelectionOperation::Align)
        //             }
        //             _ => None,
        //         });

        // let mut extract_ids = vec![];
        // let mut align_ids = vec![];
        // let mut export_ids = vec![];

        // // Build a node for each known index
        // for idx in self.graph.node_indices() {
        //     let w_id = *state.node_ids.get(&idx).unwrap();
        //     let node = self.graph.node_weight(idx).unwrap();
        //     let view_socket =
        //         state
        //             .socket_view
        //             .as_ref()
        //             .and_then(|(n, s)| if *n == idx { Some(s.clone()) } else { None });

        //     for press in ui
        //         .widget_input(w_id)
        //         .presses()
        //         .mouse()
        //         .button(input::MouseButton::Left)
        //     {
        //         state.update(|state| {
        //             if press.1 == input::ModifierKey::SHIFT {
        //                 state.selection.add(w_id);
        //             }
        //             state.selection.set_active(Some(w_id))
        //         });
        //         evs.push_back(Event::ActiveElement(idx));
        //     }

        //     let selection_state = if state.selection.is_active(w_id) {
        //         node::SelectionState::Active
        //     } else if state.selection.is_selected(w_id) {
        //         node::SelectionState::Selected
        //     } else {
        //         node::SelectionState::None
        //     };

        //     let socket_count = node.inputs.len().max(node.outputs.len());

        //     for ev in node::Node::new(
        //         idx,
        //         &node.type_variables,
        //         &node.inputs,
        //         &node.outputs,
        //         &node.title,
        //     )
        //     .title_color(style.node_title_color(&ui.theme))
        //     .title_size(style.node_title_size(&ui.theme))
        //     .selected(selection_state)
        //     .view_socket(view_socket)
        //     .active_color(style.node_active_color(&ui.theme))
        //     .selection_color(style.node_selection_color(&ui.theme))
        //     .parent(id)
        //     .xy_relative_to(id, state.camera.transform(node.position))
        //     .thumbnail(node.thumbnail)
        //     .wh([
        //         STANDARD_NODE_SIZE * state.camera.zoom,
        //         node_height(socket_count, 16., 8.) * state.camera.zoom,
        //     ])
        //     .zoom(state.camera.zoom)
        //     .set(w_id, ui)
        //     {
        //         match ev {
        //             node::Event::NodeDragStart => {
        //                 drag_operation = Some(DragOperation::Starting);
        //             }
        //             node::Event::NodeDragMotion(delta, tmp_snap) => {
        //                 drag_operation = Some(DragOperation::Moving(delta, tmp_snap));
        //             }
        //             node::Event::NodeDragStop => {
        //                 drag_operation = Some(DragOperation::Drop);
        //             }
        //             node::Event::NodeDelete => {
        //                 evs.push_back(Event::NodeDelete(idx));
        //             }
        //             node::Event::NodeEnter => {
        //                 evs.push_back(Event::NodeEnter(idx));
        //             }
        //             node::Event::SocketDrag(from, to) => {
        //                 state.update(|state| {
        //                     state.connection_draw = Some(ConnectionDraw { from, to })
        //                 });
        //             }
        //             node::Event::SocketRelease(nid, node::SocketType::Source) => {
        //                 if let Some(draw) = &state.connection_draw {
        //                     if let Some(target) = self.find_target_socket(ui, state, draw.to) {
        //                         let w_id = state.node_ids.get(&nid).unwrap();
        //                         evs.push_back(Event::ConnectionDrawn(
        //                             nid,
        //                             node::target_socket(ui, *w_id, draw.from)
        //                                 .unwrap()
        //                                 .to_string(),
        //                             target.0,
        //                             target.1,
        //                         ))
        //                     }
        //                 }
        //                 state.update(|state| {
        //                     state.connection_draw = None;
        //                 });
        //             }
        //             node::Event::SocketRelease(nid, node::SocketType::Sink) => {
        //                 if let Some(draw) = &state.connection_draw {
        //                     if let Some(target) = self.find_target_socket(ui, state, draw.from) {
        //                         let w_id = state.node_ids.get(&nid).unwrap();
        //                         evs.push_back(Event::ConnectionDrawn(
        //                             target.0,
        //                             target.1,
        //                             nid,
        //                             node::target_socket(ui, *w_id, draw.to).unwrap().to_string(),
        //                         ))
        //                     }
        //                 }
        //                 state.update(|state| {
        //                     state.connection_draw = None;
        //                 });
        //             }
        //             node::Event::SocketClear(socket) => {
        //                 evs.push_back(Event::SocketClear(idx, socket))
        //             }
        //             node::Event::SocketView(socket) => {
        //                 if state
        //                     .socket_view
        //                     .as_ref()
        //                     .map(|s| s == &(idx, socket.clone()))
        //                     .unwrap_or(false)
        //                 {
        //                     state.update(|state| state.socket_view = None);
        //                     evs.push_back(Event::SocketViewClear)
        //                 } else {
        //                     state.update(|state| state.socket_view = Some((idx, socket.clone())));
        //                     evs.push_back(Event::SocketView(idx, socket))
        //                 }
        //             }
        //         }
        //     }

        //     if selection_state != node::SelectionState::None {
        //         match selection_op {
        //             Some(SelectionOperation::Delete) => {
        //                 evs.push_back(Event::NodeDelete(idx));
        //             }
        //             Some(SelectionOperation::Extract) => {
        //                 extract_ids.push(idx);
        //             }
        //             Some(SelectionOperation::Align) => {
        //                 align_ids.push(idx);
        //             }
        //             Some(SelectionOperation::ExportSetup) => {
        //                 export_ids.push(idx);
        //             }
        //             _ => {}
        //         }
        //     }
        // }

        // // Dragging of nodes processed separately to apply operation to the
        // // entire selection set
        // for idx in self.graph.node_indices() {
        //     match drag_operation {
        //         Some(DragOperation::Starting) => {
        //             let w_id = state.node_ids.get(&idx).unwrap().clone();
        //             let pos = self.graph.node_weight(idx).unwrap().position;
        //             state.update(|state| state.selection.start_drag(&w_id, pos));
        //         }
        //         Some(DragOperation::Moving(delta, tmp_snap)) => {
        //             let w_id = state.node_ids.get(&idx).unwrap().clone();
        //             state
        //                 .update(|state| state.selection.drag(&w_id, state.camera.inv_scale(delta)));
        //             if let Some(pos) = state.selection.drag_pos(&w_id) {
        //                 evs.push_back(Event::NodeDrag(idx, pos[0], pos[1], tmp_snap))
        //             }
        //         }
        //         Some(DragOperation::Drop) => {
        //             let w_id = state.node_ids.get(&idx).unwrap().clone();
        //             state.update(|state| state.selection.stop_drag(&w_id));
        //         }
        //         None => {}
        //     }
        // }

        // // Draw a line for each edge
        // for idx in self.graph.edge_indices() {
        //     let w_id = state.edge_ids.get(&idx).unwrap();
        //     let (from_idx, to_idx) = self.graph.edge_endpoints(idx).unwrap();
        //     let edge = self.graph.edge_weight(idx).unwrap();

        //     let from_pos =
        //         super::node::socket_rect(ui, *state.node_ids.get(&from_idx).unwrap(), &edge.0)
        //             .expect("Missing source socket for drawing")
        //             .xy();
        //     let to_pos =
        //         super::node::socket_rect(ui, *state.node_ids.get(&to_idx).unwrap(), &edge.1)
        //             .expect("Missing sink socket for drawing")
        //             .xy();

        //     let dist = (from_pos[0] - to_pos[0]).abs();
        //     super::bezier::Bezier::new(
        //         from_pos,
        //         [from_pos[0] + dist / 2., from_pos[1]],
        //         to_pos,
        //         [to_pos[0] - dist / 2., to_pos[1]],
        //     )
        //     .thickness((style.edge_thickness(&ui.theme) * state.camera.zoom).clamp(1.5, 8.))
        //     .color(style.edge_color(&ui.theme))
        //     .parent(id)
        //     .middle()
        //     .graphics_for(id)
        //     .depth(1.0)
        //     .set(*w_id, ui);
        // }

        // // Draw selection rectangle if actively selecting
        // if let Selection {
        //     rect: Some((from, to)),
        //     ..
        // } = &state.selection
        // {
        //     widget::BorderedRectangle::new(Rect::from_corners(*from, *to).dim())
        //         .xy_relative_to(
        //             id,
        //             [
        //                 from[0] + (to[0] - from[0]) / 2.0,
        //                 from[1] + (to[1] - from[1]) / 2.0,
        //             ],
        //         )
        //         .parent(id)
        //         .color(style.select_rect_color(&ui.theme).alpha(0.2))
        //         .border_color(style.select_rect_color(&ui.theme))
        //         .set(state.ids.selection_rect, ui);
        // }

        // // Draw floating noodle if currently drawing a connection
        // if let Some(ConnectionDraw { from, to, .. }) = &state.connection_draw {
        //     let dist = (from[0] - to[0]).abs();
        //     super::bezier::Bezier::new(
        //         *from,
        //         [from[0] + dist / 2., from[1]],
        //         *to,
        //         [to[0] - dist / 2., to[1]],
        //     )
        //     .thickness((style.edge_thickness(&ui.theme) * state.camera.zoom).clamp(1.5, 6.))
        //     .color(style.edge_drag_color(&ui.theme))
        //     .graphics_for(id)
        //     .pattern(widget::point_path::Pattern::Dotted)
        //     .middle()
        //     .parent(id)
        //     .depth(1.0)
        //     .set(state.ids.floating_noodle, ui);
        // }

        // // Trigger Add Modal
        // evs.extend(
        //     ui.widget_input(id)
        //         .clicks()
        //         .button(input::MouseButton::Right)
        //         .map(|c| Event::AddModal(state.camera.inv_transform(c.xy))),
        // );
        // if rect.is_over(ui.global_input().current.mouse.xy) {
        //     evs.extend(ui.global_input().events().ui().find_map(|x| match x {
        //         event::Ui::Press(
        //             _,
        //             event::Press {
        //                 button: event::Button::Keyboard(input::Key::A),
        //                 modifiers: input::ModifierKey::CTRL,
        //             },
        //         ) => Some(Event::AddModal(state.camera.inv_transform([0., 0.]))),
        //         _ => None,
        //     }));
        // }

        // // Handle extraction events
        // if !extract_ids.is_empty() {
        //     evs.push_back(Event::Extract(extract_ids));
        // }

        // // Handle align operation
        // if !align_ids.is_empty() {
        //     evs.push_back(Event::AlignNodes(align_ids));
        // }

        // // Handle export requests
        // if !export_ids.is_empty() {
        //     evs.push_back(Event::ExportSetup(export_ids));
        // }

        evs
    }
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
    set: HashMap<widget::Id, Selected>,
    active: Option<widget::Id>,
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

    pub fn add(&mut self, widget_id: widget::Id) {
        self.set.insert(widget_id, Selected::default());
    }

    pub fn get_geometry(&mut self) -> Option<(Point, Point)> {
        self.rect
    }

    pub fn set_active(&mut self, widget_id: Option<widget::Id>) {
        if let Some(wid) = widget_id {
            if !self.is_selected(wid) {
                self.set.clear();
            }

            self.set.insert(wid, Selected::default());
        }
        self.active = widget_id;
    }

    pub fn get_active(&self) -> Option<widget::Id> {
        self.active
    }

    pub fn is_active(&self, widget_id: widget::Id) -> bool {
        self.active == Some(widget_id)
    }

    pub fn finish(&mut self) {
        self.rect = None
    }

    pub fn set_selection<I>(&mut self, selection: I, adding: bool)
    where
        I: Iterator<Item = widget::Id>,
    {
        if adding {
            self.set.extend(selection.map(|x| (x, Selected::default())));
        } else {
            self.set = selection.map(|x| (x, Selected::default())).collect();
        }
        self.set_active(None);
    }

    pub fn is_selected(&self, id: widget::Id) -> bool {
        self.set.get(&id).is_some()
    }

    pub fn geometry_contains(&self, point: Point) -> bool {
        if let Some((from, to)) = self.rect {
            (from[0].min(to[0])..to[0].max(from[0])).contains(&point[0])
                && (from[1].min(to[1])..to[1].max(from[1])).contains(&point[1])
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn start_drag(&mut self, widget_id: &widget::Id, pos: Point) {
        if let Some(selected) = self.set.get_mut(widget_id) {
            selected.drag_start = Some(pos);
            selected.drag_delta = Some([0., 0.]);
        }
    }

    pub fn drag(&mut self, widget_id: &widget::Id, delta: Point) {
        if let Some(selected) = self.set.get_mut(widget_id) {
            selected.drag_delta = selected
                .drag_delta
                .map(|[x, y]| [x + delta[0], y + delta[1]]);
        }
    }

    pub fn drag_pos(&self, widget_id: &widget::Id) -> Option<Point> {
        if let Some(selected) = self.set.get(widget_id) {
            selected
                .drag_start
                .and_then(|[px, py]| selected.drag_delta.map(|[dx, dy]| [px + dx, py + dy]))
        } else {
            None
        }
    }

    pub fn stop_drag(&mut self, widget_id: &widget::Id) {
        if let Some(selected) = self.set.get_mut(widget_id) {
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

fn node_height(socket_count: usize, socket_size: f64, min_skip: f64) -> f64 {
    let n = socket_count as f64;
    (2. * n * min_skip + socket_size * n).max(STANDARD_NODE_SIZE)
}
