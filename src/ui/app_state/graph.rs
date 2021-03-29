use crate::lang::resource as r;
use crate::lang::*;

use conrod_core::{image, Point};
use std::collections::HashMap;

use super::collection::Collection;

#[derive(Debug, Clone)]
pub struct Graph {
    pub graph: NodeGraph,
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

pub type NodeGraph = petgraph::Graph<NodeData, (String, String)>;

impl Graph {
    pub fn new(name: &str) -> Self {
        Self {
            graph: petgraph::Graph::new(),
            resources: HashMap::new(),
            exposed_parameters: Vec::new(),
            param_box: ParamBoxDescription::graph_parameters(name),
            active_element: None,
        }
    }

    pub fn insert_index(&mut self, resource: Resource<r::Node>, index: petgraph::graph::NodeIndex) {
        self.resources.insert(resource, index);
    }

    pub fn remove_index(&mut self, resource: &Resource<r::Node>) {
        self.resources.remove(resource);
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
            for idx in nodes.iter() {
                if let Some(n) = self.graph.node_weight_mut(*idx) {
                    n.position[0] = mean_x;
                }
            }
        } else {
            let mean_y = poss.clone().map(|x| x[1]).mean();
            for idx in nodes.iter() {
                if let Some(n) = self.graph.node_weight_mut(*idx) {
                    n.position[1] = mean_y;
                }
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
