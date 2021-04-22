use crate::lang::resource as r;
use crate::lang::*;

use conrod_core::{image, Point};
use rstar::{PointDistance, RTree, RTreeObject};
use std::collections::HashMap;

use super::collection::Collection;

#[derive(Debug, Clone)]
pub struct Graph {
    pub graph: NodeGraph,
    pub rtree: RTree<GraphObject>,
    pub resources: HashMap<Resource<r::Node>, petgraph::graph::NodeIndex>,
    pub exposed_parameters: Vec<(String, GraphParameter)>,
    pub param_box: ParamBoxDescription<GraphField>,
    pub active_element: Option<petgraph::graph::NodeIndex>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeData {
    pub resource: Resource<Node>,
    pub callee: Option<Resource<r::Graph>>,
    pub thumbnail: Option<image::Id>,
    pub position: Point,
    pub title: String,
    pub inputs: Vec<(String, OperatorType)>,
    pub outputs: Vec<(String, OperatorType)>,
    pub exportable: bool,
    pub type_variables: HashMap<TypeVariable, ImageType>,
    pub param_box: ParamBoxDescription<MessageWriters>,
}

impl NodeData {
    pub fn new(
        resource: Resource<Node>,
        position: Option<Point>,
        operator: &Operator,
        param_box: ParamBoxDescription<MessageWriters>,
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
        Self {
            resource,
            callee: operator.graph().cloned(),
            title,
            inputs,
            outputs,
            exportable: matches!(
                operator,
                Operator::AtomicOperator(AtomicOperator::Output(..))
            ),
            param_box,
            thumbnail: None,
            position: position.unwrap_or([0., 0.]),
            type_variables: HashMap::new(),
        }
    }

    pub fn update(&mut self, operator: Operator, param_box: ParamBoxDescription<MessageWriters>) {
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
        self.callee = operator.graph().cloned();

        self.param_box = param_box;
    }

    pub fn set_type_variable(&mut self, var: TypeVariable, ty: Option<ImageType>) {
        match ty {
            Some(ty) => self.type_variables.insert(var, ty),
            None => self.type_variables.remove(&var),
        };
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphObject {
    Node {
        index: petgraph::graph::NodeIndex,
        position: Point,
    },
    Noodle {
        index_from: petgraph::graph::NodeIndex,
        index_to: petgraph::graph::NodeIndex,
        from: Point,
        to: Point,
    },
}

impl RTreeObject for GraphObject {
    type Envelope = rstar::AABB<Point>;

    fn envelope(&self) -> Self::Envelope {
        match self {
            GraphObject::Node { position, .. } => rstar::AABB::from_corners(
                [position[0] - 64., position[1] - 64.],
                [position[0] + 64., position[1] + 64.],
            ),
            GraphObject::Noodle { from, to, .. } => rstar::AABB::from_corners(*from, *to),
        }
    }
}

impl PointDistance for GraphObject {
    fn distance_2(&self, point: &Point) -> f64 {
        match self {
            GraphObject::Node { position, .. } => {
                (point[0] - position[0]).powi(2) + (point[1] - position[1]).powi(2)
            }
            GraphObject::Noodle { from, to, .. } => {
                let mid = [(from[0] + to[0]) / 2., (from[1] + to[1]) / 2.];
                (point[0] - mid[0]).powi(2) + (point[1] - mid[1]).powi(2)
            }
        }
    }
}

pub type NodeGraph = petgraph::Graph<NodeData, (String, String)>;

impl Graph {
    pub fn new(name: &str) -> Self {
        Self {
            graph: petgraph::Graph::new(),
            rtree: RTree::new(),
            resources: HashMap::new(),
            exposed_parameters: Vec::new(),
            param_box: ParamBoxDescription::graph_parameters(name),
            active_element: None,
        }
    }

    /// Add a node into the graph, creating all necessary acceleration structures.
    pub fn add_node(&mut self, res: Resource<Node>, node: NodeData) {
        let pos = node.position;
        let idx = self.graph.add_node(node);
        self.rtree.insert(GraphObject::Node {
            index: idx,
            position: pos,
        });
        self.resources.insert(res, idx);
        dbg!(&self.rtree);
    }

    /// Remove a node from the graph, respecting all acceleration structures.
    pub fn remove_node(&mut self, res: &Resource<Node>) {
        if let Some(idx) = self.resources.remove(res) {
            // Obtain last node before removal for reindexing
            let last = {
                let last_idx = self.graph.node_indices().next_back().unwrap();
                let last = self.graph.node_weight(last_idx).unwrap();

                if last_idx != idx {
                    Some((last.resource.clone(), last.position, last_idx))
                } else {
                    None
                }
            };

            // Remove node
            let node = self
                .graph
                .remove_node(idx)
                .expect("Graph inconsistency detected during removal");
            self.rtree
                .remove(dbg!(&GraphObject::Node {
                    index: idx,
                    position: node.position,
                }))
                .expect("R-Tree inconsistency detected during removal phase 1");

            // Update index of last
            if let Some((last_res, last_pos, last_idx)) = last {
                self.resources.insert(last_res, idx);
                let gobj = self
                    .rtree
                    .locate_all_at_point_mut(&last_pos)
                    .find(|gobj| {
                        if let GraphObject::Node { index, .. } = gobj {
                            index == &last_idx
                        } else {
                            false
                        }
                    })
                    .unwrap();

                match gobj {
                    GraphObject::Node { index, .. } => *index = idx,
                    _ => unreachable!(),
                }
            }
        }
    }

    /// Move a node position, updating acceleration structures. Returns new
    /// position. Snapping can be enabled via the boolean parameter.
    ///
    /// Panics if the node index is invalid!
    pub fn move_node(&mut self, idx: petgraph::graph::NodeIndex, to: Point, snap: bool) {
        let mut node = self.graph.node_weight_mut(idx).unwrap();
        let old_position = node.position;

        node.position[0] = to[0];
        node.position[1] = to[1];

        if snap {
            node.position[0] = (node.position[0] / 32.).round() * 32.;
            node.position[1] = (node.position[1] / 32.).round() * 32.;
        }

        // Move node in R-Tree
        self.rtree
            .remove(&GraphObject::Node {
                position: old_position,
                index: idx,
            })
            .expect("R-Tree inconsistency during node moving");
        self.rtree.insert(GraphObject::Node {
            position: node.position,
            index: idx,
        });
    }

    pub fn resources(&self) -> &HashMap<Resource<r::Node>, petgraph::graph::NodeIndex> {
        &self.resources
    }

    pub fn resources_mut(&mut self) -> &mut HashMap<Resource<r::Node>, petgraph::graph::NodeIndex> {
        &mut self.resources
    }

    /// Align given nodes in the graph on a best guess basis, returning
    /// resources and new positions
    ///
    /// It does so by calculating the variance in X and Y directions separately,
    /// and aligning in whichever axis the variance is currently minimal.
    pub fn align_nodes(
        &mut self,
        nodes: &[petgraph::graph::NodeIndex],
    ) -> Vec<(Resource<Node>, (f64, f64))> {
        use statrs::statistics::Statistics;

        let poss = nodes
            .iter()
            .filter_map(|idx| self.graph.node_weight(*idx))
            .map(|n| n.position);
        let var_x = poss.clone().map(|x| x[0]).variance();
        let var_y = poss.clone().map(|x| x[1]).variance();

        if var_y > var_x {
            let mean_x = poss.clone().map(|x| x[0]).mean();
            for (idx, pos) in nodes
                .iter()
                .filter_map(|idx| self.graph.node_weight(*idx).map(|n| (idx, n.position)))
                .collect::<Vec<_>>()
            {
                let new_pos = [mean_x, pos[1]];
                self.move_node(*idx, new_pos, false);
            }
        } else {
            let mean_y = poss.clone().map(|x| x[1]).mean();
            for (idx, pos) in nodes
                .iter()
                .filter_map(|idx| self.graph.node_weight(*idx).map(|n| (idx, n.position)))
                .collect::<Vec<_>>()
            {
                let new_pos = [pos[0], mean_y];
                self.move_node(*idx, new_pos, false);
            }
        }

        nodes
            .iter()
            .filter_map(|idx| {
                let n = self.graph.node_weight(*idx)?;
                Some((n.resource.clone(), (n.position[0], n.position[1])))
            })
            .collect()
    }
}

impl Collection for Graph {
    fn rename_collection(&mut self, to: &Resource<r::Graph>) {
        self.param_box.categories[0].parameters[0].control = Control::Entry {
            value: to.file().unwrap().to_string(),
        };
        for gp in self.exposed_parameters.iter_mut().map(|x| &mut x.1) {
            gp.parameter.set_graph(to.path());
        }
        for (mut res, idx) in self.resources.drain().collect::<Vec<_>>() {
            res.set_graph(to.path());
            self.resources.insert(res.clone(), idx);
            self.graph.node_weight_mut(idx).unwrap().resource = res;
        }
    }

    fn exposed_parameters(&mut self) -> &mut Vec<(String, GraphParameter)> {
        &mut self.exposed_parameters
    }

    fn collection_parameters(&mut self) -> &mut ParamBoxDescription<GraphField> {
        &mut self.param_box
    }

    fn expose_parameter(&mut self, param: GraphParameter) {
        let node = param.parameter.parameter_node();
        if let Some(idx) = self.resources.get(&node) {
            let pbox = &mut self
                .graph
                .node_weight_mut(*idx)
                .expect("Malformed graph in UI")
                .param_box;
            pbox.set_expose_status(
                param.parameter.fragment().unwrap(),
                Some(ExposeStatus::Exposed),
            );
            self.exposed_parameters
                .push((param.graph_field.clone(), param));
        }
    }

    fn conceal_parameter(&mut self, field: &str) {
        if let Some(idx) = self.exposed_parameters.iter().position(|x| x.0 == field) {
            let (_, param) = self.exposed_parameters.remove(idx);
            let node = param.parameter.parameter_node();
            if let Some(gidx) = self.resources.get(&node) {
                let pbox = &mut self
                    .graph
                    .node_weight_mut(*gidx)
                    .expect("Malformed graph in UI")
                    .param_box;
                pbox.set_expose_status(
                    param.parameter.fragment().unwrap(),
                    Some(ExposeStatus::Unexposed),
                );
            }
        }
    }

    fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id) {
        if let Some(node) = self
            .resources
            .get(node)
            .copied()
            .and_then(|idx| self.graph.node_weight_mut(idx))
        {
            node.thumbnail = Some(thumbnail);
        }
    }

    fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        let mut old_id = None;

        if let Some(node) = self
            .resources
            .get(node)
            .copied()
            .and_then(|idx| self.graph.node_weight_mut(idx))
        {
            old_id = node.thumbnail;
            node.thumbnail = None;
        }

        old_id
    }

    fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        op: &ComplexOperator,
        pbox: &ParamBoxDescription<MessageWriters>,
    ) {
        if let Some(idx) = self.resources.get(node) {
            let node_weight = self.graph.node_weight_mut(*idx).unwrap();
            node_weight.update(Operator::ComplexOperator(op.clone()), pbox.clone());
        }
    }

    fn active_element(
        &mut self,
    ) -> Option<(&Resource<r::Node>, &mut ParamBoxDescription<MessageWriters>)> {
        let idx = self.active_element.as_ref()?;
        let node = self.graph.node_weight_mut(*idx)?;
        Some((&node.resource, &mut node.param_box))
    }

    fn active_resource(&self) -> Option<&Resource<r::Node>> {
        let idx = self.active_element.as_ref()?;
        let node = self.graph.node_weight(*idx)?;
        Some(&node.resource)
    }

    fn set_active(&mut self, element: &Resource<r::Node>) {
        self.active_element = self.resources.get(element).cloned();
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new("base")
    }
}
