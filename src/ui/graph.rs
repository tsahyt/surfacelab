use super::node;
use crate::lang::*;

use conrod_core::*;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet, VecDeque};
use std::iter::FromIterator;

const STANDARD_NODE_SIZE: f64 = 128.0;
const ZOOM_SENSITIVITY: f64 = 1.0 / 100.0;

#[derive(Clone, Debug, PartialEq)]
pub struct NodeData {
    pub resource: Resource<Node>,
    pub thumbnail: Option<image::Id>,
    pub position: Point,
    pub title: String,
    pub inputs: Vec<(String, OperatorType)>,
    pub outputs: Vec<(String, OperatorType)>,
    pub type_variables: HashMap<TypeVariable, ImageType>,
    pub param_box: ParamBoxDescription<MessageWriters>,
}

impl NodeData {
    pub fn new(
        resource: Resource<Node>,
        position: Option<Point>,
        operator: &Operator,
        param_box: ParamBoxDescription<Field>,
    ) -> Self {
        let mut inputs: Vec<_> = operator
            .inputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        inputs.sort();
        let mut outputs: Vec<_> = operator
            .outputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        outputs.sort();
        let title = operator.title().to_owned();
        let pbox = node_attributes(&resource, !operator.is_output())
            .map_transmitters(|t| t.clone().into())
            .merge(param_box.map_transmitters(|t| t.clone().into()));
        Self {
            resource,
            title,
            inputs,
            outputs,
            param_box: pbox,
            thumbnail: None,
            position: position.unwrap_or([0., 0.]),
            type_variables: HashMap::new(),
        }
    }

    pub fn update(&mut self, operator: Operator, param_box: ParamBoxDescription<Field>) {
        let mut inputs: Vec<_> = operator
            .inputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        inputs.sort();
        self.inputs = inputs;
        let mut outputs: Vec<_> = operator
            .outputs()
            .iter()
            .map(|(a, b)| (a.clone(), *b))
            .collect();
        outputs.sort();
        self.outputs = outputs;
        self.title = operator.title().to_owned();

        self.param_box = node_attributes(&self.resource, !operator.is_output())
            .map_transmitters(|t| t.clone().into())
            .merge(param_box.map_transmitters(|t| t.clone().into()));
    }
}

fn node_attributes(res: &Resource<Node>, scalable: bool) -> ParamBoxDescription<ResourceField> {
    let mut parameters = vec![Parameter {
        name: "Node Resource".to_string(),
        transmitter: ResourceField::Name,
        control: Control::Entry {
            value: res
                .path()
                .file_name()
                .and_then(|x| x.to_str())
                .map(|x| x.to_string())
                .unwrap(),
        },
        expose_status: None,
    }];
    if scalable {
        parameters.push(Parameter {
            name: "Size".to_string(),
            transmitter: ResourceField::Size,
            control: Control::DiscreteSlider {
                value: 0,
                min: -16,
                max: 16,
            },
            expose_status: None,
        });
        parameters.push(Parameter {
            name: "Absolute Size".to_string(),
            transmitter: ResourceField::AbsoluteSize,
            control: Control::Toggle { def: false },
            expose_status: None,
        });
    }
    ParamBoxDescription {
        box_title: "Node Attributes".to_string(),
        categories: vec![ParamCategory {
            name: "Node",
            parameters,
        }],
    }
}

impl NodeData {
    pub fn set_type_variable(&mut self, var: TypeVariable, ty: Option<ImageType>) {
        match ty {
            Some(ty) => self.type_variables.insert(var, ty),
            None => self.type_variables.remove(&var),
        };
    }
}

pub type NodeGraph = petgraph::Graph<NodeData, (String, String)>;

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
            (point[1] / self.zoom) - self.position[1]
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

#[derive(Clone)]
pub struct Selection {
    rect: Option<(Point, Point)>,
    set: HashSet<widget::Id>,
    active: Option<widget::Id>,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            rect: None,
            set: HashSet::new(),
            active: None,
        }
    }
}

impl Selection {
    pub fn set_geometry(&mut self, from: Point, to: Point) {
        self.rect = Some((from, to))
    }

    pub fn add(&mut self, widget_id: widget::Id) {
        self.set.insert(widget_id);
    }

    pub fn get_geometry(&mut self) -> Option<(Point, Point)> {
        self.rect
    }

    pub fn set_active(&mut self, widget_id: Option<widget::Id>) {
        if let Some(wid) = widget_id {
            if !self.is_selected(wid) {
                self.set.clear();
            }

            self.set.insert(wid);
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

    pub fn set_selection(&mut self, selection: HashSet<widget::Id>) {
        self.set = selection;
        self.set_active(None);
    }

    pub fn is_selected(&self, id: widget::Id) -> bool {
        self.set.contains(&id)
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
}

#[derive(Clone)]
pub struct ConnectionDraw {
    from: Point,
    to: Point,
}

#[derive(Clone, WidgetCommon)]
pub struct Graph<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    graph: &'a NodeGraph,
    style: Style,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

widget_ids! {
    #[derive(Clone)]
    pub struct Ids {
        selection_rect,
        floating_noodle
    }
}

#[derive(Clone)]
pub struct State {
    ids: Ids,
    node_ids: HashMap<petgraph::graph::NodeIndex, widget::Id>,
    edge_ids: HashMap<petgraph::graph::EdgeIndex, widget::Id>,
    camera: Camera,
    selection: Selection,
    connection_draw: Option<ConnectionDraw>,
}

#[derive(Clone, Debug)]
pub enum Event {
    NodeDrag(petgraph::graph::NodeIndex, Scalar, Scalar),
    ConnectionDrawn(
        petgraph::graph::NodeIndex,
        String,
        petgraph::graph::NodeIndex,
        String,
    ),
    SocketClear(petgraph::graph::NodeIndex, String),
    NodeDelete(petgraph::graph::NodeIndex),
    ActiveElement(petgraph::graph::NodeIndex),
    AddModal(Point),
}

impl<'a> Graph<'a> {
    pub fn new(graph: &'a NodeGraph) -> Self {
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

        for _release in ui
            .widget_input(id)
            .releases()
            .mouse()
            .button(input::MouseButton::Left)
        {
            state.update(|state| {
                let selected = HashSet::from_iter(self.graph.node_indices().filter_map(|idx| {
                    if state.selection.geometry_contains(
                        state
                            .camera
                            .transform(self.graph.node_weight(idx).unwrap().position),
                    ) {
                        Some(*state.node_ids.get(&idx).unwrap())
                    } else {
                        None
                    }
                }));
                state.selection.set_selection(selected);
                state.selection.finish();
            })
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
        self.graph
            .node_indices()
            .filter_map(|idx| {
                let w_id = state.node_ids.get(&idx)?;
                let socket = super::node::target_socket(ui, *w_id, drop_point)?;
                Some((idx, socket.to_string()))
            })
            .next()
    }
}

type Events = VecDeque<Event>;

impl<'a> Widget for Graph<'a> {
    type State = State;
    type Style = Style;
    type Event = Events;

    fn init_state(&self, mut id_gen: widget::id::Generator) -> Self::State {
        State {
            node_ids: HashMap::from_iter(self.graph.node_indices().map(|idx| (idx, id_gen.next()))),
            edge_ids: HashMap::from_iter(self.graph.edge_indices().map(|idx| (idx, id_gen.next()))),
            ids: Ids::new(id_gen),
            camera: Camera::default(),
            selection: Selection::default(),
            connection_draw: None,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { id, state, ui, .. } = args;
        let mut evs = VecDeque::new();

        // We collect the new nodes into a SmallVec that will spill after 4
        // elements. This should be plenty, since updates should arrive slowly
        // anyhow, unless we're rebuilding the whole graph on load, in which
        // case the allocation is acceptable.
        let new_nodes: SmallVec<[_; 4]> = self
            .graph
            .node_indices()
            .filter(|x| state.node_ids.get(&x).is_none())
            .collect();

        for idx in new_nodes {
            state.update(|state| {
                state.node_ids.insert(idx, ui.widget_id_generator().next());
            })
        }

        // Now repeat the same thing for edges. Note that we don't have to do
        // the deletion here, because all edges are the same in terms of
        // internal state, since they're just simple primitives.
        let new_edges: SmallVec<[_; 4]> = self
            .graph
            .edge_indices()
            .filter(|x| state.edge_ids.get(&x).is_none())
            .collect();

        for idx in new_edges {
            state.update(|state| {
                state.edge_ids.insert(idx, ui.widget_id_generator().next());
            })
        }

        // Update camera
        self.camera_handling(ui, state, id);

        // Update selection
        self.rect_selection_handling(ui, state, id);

        let mut node_drags: SmallVec<[_; 4]> = SmallVec::new();

        // Handle deletion events
        let delete_trigger = ui
            .widget_input(id)
            .presses()
            .key()
            .any(|x| x.key == input::Key::X);

        // Build a node for each known index
        for idx in self.graph.node_indices() {
            let w_id = *state.node_ids.get(&idx).unwrap();
            let node = self.graph.node_weight(idx).unwrap();

            for press in ui
                .widget_input(w_id)
                .presses()
                .mouse()
                .button(input::MouseButton::Left)
            {
                state.update(|state| {
                    if press.1 == input::ModifierKey::SHIFT {
                        state.selection.add(w_id);
                    }
                    state.selection.set_active(Some(w_id))
                });
                evs.push_back(Event::ActiveElement(idx));
            }

            let selection_state = if state.selection.is_active(w_id) {
                node::SelectionState::Active
            } else if state.selection.is_selected(w_id) {
                node::SelectionState::Selected
            } else {
                node::SelectionState::None
            };

            for ev in node::Node::new(
                idx,
                &node.type_variables,
                &node.inputs,
                &node.outputs,
                &node.title,
            )
            .selected(selection_state)
            .parent(id)
            .xy_relative_to(id, state.camera.transform(node.position))
            .thumbnail(node.thumbnail)
            .wh([
                STANDARD_NODE_SIZE * state.camera.zoom,
                STANDARD_NODE_SIZE * state.camera.zoom,
            ])
            .set(w_id, ui)
            {
                match ev {
                    node::Event::NodeDrag(delta) => {
                        node_drags.push(state.camera.inv_scale(delta));
                    }
                    node::Event::NodeDelete => {
                        evs.push_back(Event::NodeDelete(idx));
                    }
                    node::Event::SocketDrag(from, to) => {
                        state.update(|state| {
                            state.connection_draw = Some(ConnectionDraw { from, to })
                        });
                    }
                    node::Event::SocketRelease(nid) => {
                        if let Some(draw) = &state.connection_draw {
                            if let Some(target) = self.find_target_socket(ui, state, draw.to) {
                                // This is available above, but we recreate it
                                // because this allows sidestepping the borrow
                                // checker.
                                let w_id = state.node_ids.get(&nid).unwrap();
                                evs.push_back(Event::ConnectionDrawn(
                                    nid,
                                    node::target_socket(ui, *w_id, draw.from)
                                        .unwrap()
                                        .to_string(),
                                    target.0,
                                    target.1,
                                ))
                            }
                        }
                        state.update(|state| {
                            state.connection_draw = None;
                        });
                    }
                    node::Event::SocketClear(socket) => {
                        evs.push_back(Event::SocketClear(idx, socket))
                    }
                }
            }

            if delete_trigger && selection_state != node::SelectionState::None {
                evs.push_back(Event::NodeDelete(idx));
            }
        }

        // Dragging of nodes processed separately to apply operation to the
        // entire selection set
        for idx in self.graph.node_indices() {
            for [x, y] in node_drags.iter() {
                let w_id = state.node_ids.get(&idx).unwrap();
                if state.selection.is_selected(*w_id) {
                    evs.push_back(Event::NodeDrag(idx, *x, *y));
                }
            }
        }

        // Draw a line for each edge
        for idx in self.graph.edge_indices() {
            let w_id = state.edge_ids.get(&idx).unwrap();
            let (from_idx, to_idx) = self.graph.edge_endpoints(idx).unwrap();
            let edge = self.graph.edge_weight(idx).unwrap();

            let from_pos =
                super::node::socket_rect(ui, *state.node_ids.get(&from_idx).unwrap(), &edge.0)
                    .expect("Missing source socket for drawing")
                    .xy();
            let to_pos =
                super::node::socket_rect(ui, *state.node_ids.get(&to_idx).unwrap(), &edge.1)
                    .expect("Missing sink socket for drawing")
                    .xy();

            widget::Line::abs(from_pos, to_pos)
                .color(color::LIGHT_GREY)
                .thickness(3.0)
                .parent(id)
                .depth(1.0)
                .middle()
                .set(*w_id, ui);
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
                .color(color::Color::Rgba(0.9, 0.8, 0.15, 0.2))
                .border_color(color::Color::Rgba(0.9, 0.8, 0.15, 1.0))
                .set(state.ids.selection_rect, ui);
        }

        // Draw floating noodle if currently drawing a connection
        if let Some(ConnectionDraw { from, to, .. }) = &state.connection_draw {
            widget::Line::abs(*from, *to)
                .color(color::DARK_RED)
                .thickness(3.0)
                .parent(id)
                .depth(1.0)
                .middle()
                .set(state.ids.floating_noodle, ui);
        }

        // Trigger Add Modal
        evs.extend(
            ui.widget_input(id)
                .clicks()
                .button(input::MouseButton::Right)
                .map(|c| Event::AddModal(state.camera.inv_transform(c.xy))),
        );

        evs
    }
}
