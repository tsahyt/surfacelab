use crate::lang::resource as r;
use crate::lang::*;

use conrod_core::{image, Point};
use rstar::{PointDistance, RTree, RTreeObject, SelectionFunction};
use std::collections::HashMap;

use super::collection::Collection;

pub const STANDARD_NODE_SIZE: f64 = 128.;

#[derive(Debug, Clone)]
pub struct Graph {
    pub rtree: RTree<GraphObject>,
    pub nodes: HashMap<Resource<r::Node>, NodeData>,
    pub node_count: usize,
    pub connection_count: usize,
    pub exposed_parameters: Vec<(String, GraphParameter)>,
    pub param_box: ParamBoxDescription<GraphField>,
    pub active_element: Option<Resource<Node>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphObject {
    Node {
        position: Point,
        resource: Resource<Node>,
    },
    Connection {
        source: Resource<Socket>,
        sink: Resource<Socket>,
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
            GraphObject::Connection { from, to, .. } => rstar::AABB::from_corners(*from, *to),
        }
    }
}

impl PointDistance for GraphObject {
    fn distance_2(&self, point: &Point) -> f64 {
        match self {
            GraphObject::Node { position, .. } => {
                (point[0] - position[0]).powi(2) + (point[1] - position[1]).powi(2)
            }
            GraphObject::Connection { from, to, .. } => {
                let mid = [(from[0] + to[0]) / 2., (from[1] + to[1]) / 2.];
                (point[0] - mid[0]).powi(2) + (point[1] - mid[1]).powi(2)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeData {
    pub resource: Resource<Node>,
    pub position: Point,
    pub callee: Option<Resource<r::Graph>>,
    pub thumbnail: Option<image::Id>,
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
        operator: &Operator,
        position: Point,
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
            position,
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

    pub fn socket_position(&self, socket: &str) -> Option<Point> {
        let nheight = node_height(self.inputs.len().max(self.outputs.len()), 16., 8.);

        if let Some(pos) = self.inputs.iter().position(|(s, _)| s == socket) {
            let skip = socket_margin_skip(self.inputs.len(), 16., 16., nheight);
            let margin = 16. + (pos as f64 + 1.) * skip + (pos as f64) * (skip + 16.);

            let x = self.position[0] - (STANDARD_NODE_SIZE / 2.) + 8.;
            let y = self.position[1] + (STANDARD_NODE_SIZE / 2.) - margin - 8.;

            Some([x, y])
        } else if let Some(pos) = self.outputs.iter().position(|(s, _)| s == socket) {
            let skip = socket_margin_skip(self.outputs.len(), 16., 16., nheight);
            let margin = 16. + (pos as f64 + 1.) * skip + (pos as f64) * (skip + 16.);

            let x = self.position[0] + (STANDARD_NODE_SIZE / 2.) - 8.;
            let y = self.position[1] + (STANDARD_NODE_SIZE / 2.) - margin - 8.;

            Some([x, y])
        } else {
            None
        }
    }

    /// Find socket at the position given. May be an input or an output socket.
    /// `radius2` describes the *squared* radius within which a socket is
    /// detected.
    pub fn socket_at_position(&self, position: Point, radius2: f64) -> Option<&str> {
        self.inputs
            .iter()
            .chain(self.outputs.iter())
            .find(|(socket, _)| {
                if let Some(socket_pos) = self.socket_position(socket) {
                    ((position[0] - socket_pos[0]).powi(2) + (position[1] - socket_pos[1]).powi(2))
                        < radius2
                } else {
                    false
                }
            })
            .map(|x| x.0.as_ref())
    }
}

impl Graph {
    pub fn new(name: &str) -> Self {
        Self {
            rtree: RTree::new(),
            nodes: HashMap::new(),
            node_count: 0,
            connection_count: 0,
            exposed_parameters: Vec::new(),
            param_box: ParamBoxDescription::graph_parameters(name),
            active_element: None,
        }
    }

    /// Add a node into the graph
    pub fn add_node(&mut self, res: Resource<Node>, node: NodeData) {
        self.rtree.insert(GraphObject::Node {
            resource: res.clone(),
            position: node.position,
        });
        self.nodes.insert(res, node);
        self.node_count += 1;
    }

    /// Remove a node from the graph
    pub fn remove_node(&mut self, res: &Resource<Node>) {
        if let Some(node) = self.nodes.remove(res) {
            self.rtree
                .remove_with_selection_function(SelectNodeFunction::new(res, node.position))
                .expect("R-Tree inconsistency detected!");
        }
        self.node_count -= 1;
    }

    /// Move a node position, updating acceleration structures. Returns new
    /// position. Snapping can be enabled via the boolean parameter.
    ///
    /// Panics if the node resource is invalid!
    pub fn move_node(&mut self, res: &Resource<Node>, to: Point, snap: bool) -> Point {
        let node_data = self.nodes.get_mut(res).expect("Moving unknown node");
        let old_position = node_data.position;
        let new_position = if snap {
            [(to[0] / 32.).round() * 32., (to[1] / 32.).round() * 32.]
        } else {
            to
        };

        match self
            .rtree
            .remove_with_selection_function(SelectNodeFunction::new(res, old_position))
            .expect("R-Tree inconsistency detected")
        {
            GraphObject::Node { resource, .. } => {
                self.rtree.insert(GraphObject::Node {
                    position: new_position,
                    resource,
                });
                node_data.position = new_position;
            }
            _ => unreachable!(),
        }

        let mut conns = Vec::new();

        // Remove all affected connections and store them temporarily
        while let Some(GraphObject::Connection {
            source,
            sink,
            from,
            to,
        }) = self
            .rtree
            .remove_with_selection_function(SelectConnectionFunction::new(res, old_position))
        {
            if source.is_socket_of(res) {
                conns.push(GraphObject::Connection {
                    from: node_data
                        .socket_position(source.fragment().unwrap())
                        .expect("Unable to determine source socket position"),
                    source,
                    sink,
                    to,
                })
            } else if sink.is_socket_of(res) {
                conns.push(GraphObject::Connection {
                    to: node_data
                        .socket_position(sink.fragment().unwrap())
                        .expect("Unable to determine sink socket position"),
                    source,
                    sink,
                    from,
                })
            } else {
                panic!("R-Tree inconsistency detected")
            }
        }

        // Reinsert affected connections
        for conn in conns.drain(0..) {
            self.rtree.insert(conn);
        }

        new_position
    }

    /// Find the node closest to the given position. Note that this node does
    /// not necessarily contain the position!
    pub fn nearest_node_at(&self, position: Point) -> Option<&Resource<Node>> {
        self.rtree
            .nearest_neighbor(&position)
            .and_then(|gobj| match gobj {
                GraphObject::Node { resource, .. } => Some(resource),
                _ => None,
            })
    }

    /// Find all nodes contained in the enveloped defined by the two corner
    /// points.
    pub fn nodes_in_envelope(
        &self,
        corner1: Point,
        corner2: Point,
    ) -> impl Iterator<Item = &Resource<Node>> {
        self.rtree
            .locate_in_envelope(&rstar::AABB::from_corners(corner1, corner2))
            .filter_map(|gobj| match gobj {
                GraphObject::Node { resource, .. } => Some(resource),
                _ => None,
            })
    }

    /// Connect two sockets this graph.
    ///
    /// This will not alter any node data, but merely insert a drawable graph object.
    pub fn connect_sockets(&mut self, from: &Resource<Socket>, to: &Resource<Socket>) {
        let from_pos = self
            .nodes
            .get(&from.socket_node())
            .and_then(|node| node.socket_position(from.fragment().unwrap()))
            .expect("Missing source node or socket for connection");
        let to_pos = self
            .nodes
            .get(&to.socket_node())
            .and_then(|node| node.socket_position(to.fragment().unwrap()))
            .expect("Missing source node or socket for connection");

        self.rtree.insert(GraphObject::Connection {
            source: from.clone(),
            sink: to.clone(),
            from: from_pos,
            to: to_pos,
        });
        self.connection_count += 1;
    }

    /// Disconnect two sockets in this graph.
    ///
    /// This will not alter any node data, but merely remove a drawable graph object.
    pub fn disconnect_sockets(&mut self, from: &Resource<Socket>, to: &Resource<Socket>) {
        let from_pos = self
            .nodes
            .get(&from.socket_node())
            .and_then(|node| node.socket_position(from.fragment().unwrap()))
            .expect("Missing source node or socket for connection");
        let to_pos = self
            .nodes
            .get(&to.socket_node())
            .and_then(|node| node.socket_position(to.fragment().unwrap()))
            .expect("Missing source node or socket for connection");

        self.rtree
            .remove(&GraphObject::Connection {
                source: from.clone(),
                sink: to.clone(),
                from: from_pos,
                to: to_pos,
            })
            .expect("R-Tree inconsistency detected during connection removal");

        self.connection_count -= 1;
    }

    /// Set type variable present at given socket.
    pub fn set_type_variable(&mut self, socket: &Resource<Socket>, ty: Option<ImageType>) {
        if let Some(node) = self.nodes.get_mut(&socket.socket_node()) {
            if let Some(var) = type_variable_from_socket_iter(
                node.inputs.iter().chain(node.outputs.iter()),
                socket.fragment().unwrap(),
            ) {
                node.set_type_variable(var, ty)
            }
        }
    }

    /// Align given nodes in the graph on a best guess basis, returning
    /// resources and new positions
    ///
    /// It does so by calculating the variance in X and Y directions separately,
    /// and aligning in whichever axis the variance is currently minimal.
    pub fn align_nodes(&mut self, nodes: &[Resource<Node>]) -> Vec<(Resource<Node>, (f64, f64))> {
        use statrs::statistics::Statistics;

        let poss = nodes
            .iter()
            .filter_map(|res| self.nodes.get(res))
            .map(|n| n.position);
        let var_x = poss.clone().map(|x| x[0]).variance();
        let var_y = poss.clone().map(|x| x[1]).variance();

        if var_y > var_x {
            let mean_x = poss.clone().map(|x| x[0]).mean();
            for (res, pos) in nodes
                .iter()
                .filter_map(|res| self.nodes.get(res).map(|n| (res, n.position)))
                .collect::<Vec<_>>()
            {
                let new_pos = [mean_x, pos[1]];
                self.move_node(res, new_pos, false);
            }
        } else {
            let mean_y = poss.clone().map(|x| x[1]).mean();
            for (res, pos) in nodes
                .iter()
                .filter_map(|res| self.nodes.get(res).map(|n| (res, n.position)))
                .collect::<Vec<_>>()
            {
                let new_pos = [pos[0], mean_y];
                self.move_node(res, new_pos, false);
            }
        }

        nodes
            .iter()
            .filter_map(|res| {
                let n = self.nodes.get(res)?;
                Some((n.resource.clone(), (n.position[0], n.position[1])))
            })
            .collect()
    }
}

impl Collection for Graph {
    fn rename_collection(&mut self, to: &Resource<r::Graph>) {
        // self.param_box.categories[0].parameters[0].control = Control::Entry {
        //     value: to.file().unwrap().to_string(),
        // };
        // for gp in self.exposed_parameters.iter_mut().map(|x| &mut x.1) {
        //     gp.parameter.set_graph(to.path());
        // }
        // for (mut res, idx) in self.resources.drain().collect::<Vec<_>>() {
        //     res.set_graph(to.path());
        //     self.resources.insert(res.clone(), idx);
        //     self.graph.node_weight_mut(idx).unwrap().resource = res;
        // }
    }

    fn exposed_parameters(&mut self) -> &mut Vec<(String, GraphParameter)> {
        &mut self.exposed_parameters
    }

    fn collection_parameters(&mut self) -> &mut ParamBoxDescription<GraphField> {
        &mut self.param_box
    }

    fn expose_parameter(&mut self, param: GraphParameter) {
        let node = param.parameter.parameter_node();
        if let Some(node_data) = self.nodes.get_mut(&node) {
            node_data.param_box.set_expose_status(
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
            if let Some(node_data) = self.nodes.get_mut(&node) {
                node_data.param_box.set_expose_status(
                    param.parameter.fragment().unwrap(),
                    Some(ExposeStatus::Unexposed),
                );
            }
        }
    }

    fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id) {
        if let Some(node) = self.nodes.get_mut(node) {
            node.thumbnail = Some(thumbnail);
        }
    }

    fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        let mut old_id = None;

        if let Some(node) = self.nodes.get_mut(node) {
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
        if let Some(node_data) = self.nodes.get_mut(node) {
            node_data.update(Operator::ComplexOperator(op.clone()), pbox.clone());
        }
    }

    fn active_element(
        &mut self,
    ) -> Option<(&Resource<r::Node>, &mut ParamBoxDescription<MessageWriters>)> {
        let node = self.nodes.get_mut(self.active_element.as_ref()?)?;
        Some((&node.resource, &mut node.param_box))
    }

    fn active_resource(&self) -> Option<&Resource<r::Node>> {
        self.active_element.as_ref()
    }

    fn set_active(&mut self, element: &Resource<r::Node>) {
        self.active_element = Some(element.clone())
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new("base")
    }
}

/// Selection function to select a specific node in the R-Tree given its
/// position. Differs from standard selection functions in that it *only*
/// compares based on the resource (other than spatial queries)
pub struct SelectNodeFunction<'a> {
    resource: &'a Resource<Node>,
    position: Point,
}

impl<'a> SelectNodeFunction<'a> {
    pub fn new(resource: &'a Resource<Node>, position: Point) -> Self {
        Self { resource, position }
    }
}

impl<'a> SelectionFunction<GraphObject> for SelectNodeFunction<'a> {
    fn should_unpack_parent(&self, parent_envelope: &rstar::AABB<Point>) -> bool {
        use rstar::Envelope;

        let position = self.position;
        let envelope = rstar::AABB::from_corners(
            [
                position[0] - STANDARD_NODE_SIZE / 2.,
                position[1] - STANDARD_NODE_SIZE / 2.,
            ],
            [
                position[0] + STANDARD_NODE_SIZE / 2.,
                position[1] + STANDARD_NODE_SIZE / 2.,
            ],
        );
        parent_envelope.contains_envelope(&envelope)
    }

    fn should_unpack_leaf(&self, leaf: &GraphObject) -> bool {
        match leaf {
            GraphObject::Node { resource, .. } => resource == self.resource,
            _ => false,
        }
    }
}

/// Selection function to select connections adjacent to a node, connecting
/// sockets of that node.
pub struct SelectConnectionFunction<'a> {
    resource: &'a Resource<Node>,
    position: Point,
}

impl<'a> SelectConnectionFunction<'a> {
    pub fn new(resource: &'a Resource<Node>, position: Point) -> Self {
        Self { resource, position }
    }
}

impl<'a> SelectionFunction<GraphObject> for SelectConnectionFunction<'a> {
    fn should_unpack_parent(&self, parent_envelope: &rstar::AABB<Point>) -> bool {
        use rstar::Envelope;

        let position = self.position;
        let envelope = rstar::AABB::from_corners(
            [
                position[0] - STANDARD_NODE_SIZE,
                position[1] - STANDARD_NODE_SIZE,
            ],
            [
                position[0] + STANDARD_NODE_SIZE,
                position[1] + STANDARD_NODE_SIZE,
            ],
        );
        parent_envelope.intersects(&envelope)
    }

    fn should_unpack_leaf(&self, leaf: &GraphObject) -> bool {
        match leaf {
            GraphObject::Connection { source, sink, .. } => {
                source.is_socket_of(self.resource) || sink.is_socket_of(self.resource)
            }
            _ => false,
        }
    }
}

/// Calculate the margin skip for drawing sockets given the specified parameters.
///
/// Required both for actual drawing of sockets as well as for laying out connections.
pub fn socket_margin_skip(
    sockets: usize,
    margin: f64,
    socket_height: f64,
    node_height: f64,
) -> f64 {
    let a = node_height - 2. * margin;
    let n = sockets as f64;

    (a - n * socket_height) / (2. * n)
}

/// Calculate the height of a node given the parameters
///
/// Required both for actual drawing of nodes as well as for determining bounding boxes.
pub fn node_height(socket_count: usize, socket_size: f64, min_skip: f64) -> f64 {
    let n = socket_count as f64;
    (2. * n * min_skip + socket_size * n).max(STANDARD_NODE_SIZE)
}
