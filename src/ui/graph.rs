use super::node;
use crate::lang::Resource;

use conrod_core::*;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct NodeData {
    pub thumbnail: Option<image::Id>,
    pub position: Point,
}

pub type NodeGraph = petgraph::Graph<NodeData, (String, String)>;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera {
    position: Point,
    zoom: Scalar,
}

impl Camera {
    pub fn transform_point(&self, point: Point) -> Point {
        [
            self.zoom * (point[0] + self.position[0]),
            self.zoom * (point[1] + self.position[1]),
        ]
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
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            rect: None,
            set: HashSet::new(),
        }
    }
}

impl Selection {
    pub fn set_geometry(&mut self, from: Point, to: Point) {
        self.rect = Some((from, to))
    }

    pub fn get_geometry(&mut self) -> Option<(Point, Point)> {
        self.rect
    }

    pub fn finish(&mut self) {
        self.rect = None
    }

    pub fn set_selection(&mut self, selection: HashSet<widget::Id>) {
        self.set = selection;
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
        selection_rect
    }
}

#[derive(Clone)]
pub struct State {
    ids: Ids,
    node_ids: HashMap<petgraph::graph::NodeIndex, widget::Id>,
    edge_ids: HashMap<petgraph::graph::EdgeIndex, widget::Id>,
    camera: Camera,
    selection: Selection,
}

#[derive(Copy, Clone, Debug)]
pub enum Event {
    PanCamera(Scalar, Scalar),
}

impl<'a> Graph<'a> {
    pub fn new(graph: &'a NodeGraph) -> Self {
        Graph {
            common: widget::CommonBuilder::default(),
            graph,
            style: Style::default(),
        }
    }
}

impl<'a> Widget for Graph<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, mut id_gen: widget::id::Generator) -> Self::State {
        State {
            node_ids: HashMap::from_iter(self.graph.node_indices().map(|idx| (idx, id_gen.next()))),
            edge_ids: HashMap::from_iter(self.graph.edge_indices().map(|idx| (idx, id_gen.next()))),
            ids: Ids::new(id_gen),
            camera: Camera::default(),
            selection: Selection::default(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { id, state, ui, .. } = args;

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

        // Now repeat the same thing for edges
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
        for [dx, dy] in ui.widget_input(id).drags().filter_map(|drag| match drag {
            event::Drag {
                button: input::MouseButton::Middle,
                delta_xy,
                ..
            } => Some(delta_xy),
            _ => None,
        }) {
            state.update(|state| {
                state.camera.position[0] += dx;
                state.camera.position[1] += dy;
            });
        }

        for dz in ui.widget_input(id).scrolls().map(|scroll| scroll.y) {
            state.update(|state| {
                state.camera.zoom = (state.camera.zoom - dz * 0.01).max(0.0);
            });
        }

        // Update selection
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

        for _release in ui.widget_input(id).releases().mouse() {
            state.update(|state| {
                let selected = HashSet::from_iter(self.graph.node_indices().filter_map(|idx| {
                    if state.selection.geometry_contains(
                        state
                            .camera
                            .transform_point(self.graph.node_weight(idx).unwrap().position),
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

        // Build a node for each known index
        for idx in self.graph.node_indices() {
            let w_id = state.node_ids.get(&idx).unwrap();
            let node = self.graph.node_weight(idx).unwrap();

            node::Node::new()
                .selected(state.selection.is_selected(*w_id))
                .parent(id)
                .xy_relative_to(id, state.camera.transform_point(node.position))
                .wh([128.0 * state.camera.zoom, 128.0 * state.camera.zoom])
                .set(*w_id, ui);
        }

        // Draw a line for each edge
        for idx in self.graph.edge_indices() {
            let w_id = state.edge_ids.get(&idx).unwrap();
            let (from_idx, to_idx) = self.graph.edge_endpoints(idx).unwrap();

            let from_pos = ui
                .rect_of(*state.node_ids.get(&from_idx).unwrap())
                .unwrap()
                .xy();
            let to_pos = ui
                .rect_of(*state.node_ids.get(&to_idx).unwrap())
                .unwrap()
                .xy();

            widget::Line::centred(from_pos, to_pos)
                .thickness(3.0)
                .parent(id)
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
    }
}