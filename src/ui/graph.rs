use super::node;
use crate::lang::Resource;

use conrod_core::*;
use smallvec::SmallVec;
use std::collections::HashMap;
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
    from: Point,
    to: Point,
    creating: bool,
}

impl Selection {
    pub fn new(from: Point, to: Point) -> Self {
        Self {
            from,
            to,
            creating: true,
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
    selection: Option<Selection>,
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
            selection: None,
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
                state.selection = Some(Selection::new(origin, to));
            })
        }

        for release in ui.widget_input(id).releases().mouse() {
            state.update(|state| {
                if let Some(sel) = &mut state.selection {
                    sel.creating = false;
                }
            })
        }

        // Build a node for each known index
        for idx in self.graph.node_indices() {
            let w_id = state.node_ids.get(&idx).unwrap();
            let node = self.graph.node_weight(idx).unwrap();

            node::Node::new()
                .parent(id)
                .xy_relative_to(
                    id,
                    [
                        state.camera.zoom * (node.position[0] + state.camera.position[0]),
                        state.camera.zoom * (node.position[1] + state.camera.position[1]),
                    ],
                )
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
        if let Some(Selection {
            from,
            to,
            creating: true,
            ..
        }) = &state.selection
        {
            widget::BorderedRectangle::new(Rect::from_corners(*from, *to).dim())
                .xy_relative_to(id, [
                    from[0] + (to[0] - from[0]) / 2.0,
                    from[1] + (to[1] - from[1]) / 2.0
                ])
                .parent(id)
                .color(color::Color::Rgba(0.9, 0.8, 0.15, 0.2))
                .border_color(color::Color::Rgba(0.9, 0.8, 0.15, 1.0))
                .set(state.ids.selection_rect, ui);
        }
    }
}
