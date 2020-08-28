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

#[derive(Clone, WidgetCommon)]
pub struct Graph<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    graph: &'a NodeGraph,
    style: Style,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

#[derive(Clone)]
pub struct State {
    node_ids: HashMap<petgraph::graph::NodeIndex, widget::Id>,
}

#[derive(Copy, Clone, Debug)]
pub enum Event {}

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
        }
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { id, state, ui, .. } = args;
        let mut id_gen = ui.widget_id_generator();

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
                state.node_ids.insert(idx, id_gen.next());
            })
        }

        // Build a node for each known index
        for idx in self.graph.node_indices() {
            let w_id = state.node_ids.get(&idx).unwrap();
            let node = self.graph.node_weight(idx).unwrap();

            node::Node::new()
                .parent(id)
                .xy_relative_to(id, node.position)
                .set(*w_id, ui);
        }
    }
}
