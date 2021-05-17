use crate::lang::*;

use super::{UndoAction, UndoBuilder};

/// An incremental change action is a buildable undo action that is constructed
/// from successive small changes, e.g. changing a parameter or moving the
/// camera. It can always take more events. This mimics a fold across time,
/// potentially updating an accumulator with each new event.
pub struct IncrementalChangeAction<R, T> {
    update: Box<dyn Fn(&R, &T, &Lang) -> Option<T> + Send>,
    finalize: Box<dyn Fn(&R, &T) -> Option<Vec<Lang>> + Send>,
    reference: R,
    acc: T,
}

impl<R, T> IncrementalChangeAction<R, T> {
    pub fn new<F, G>(reference: R, initial: T, update: F, finalize: G) -> Self
    where
        F: Fn(&R, &T, &Lang) -> Option<T> + 'static + Send,
        G: Fn(&R, &T) -> Option<Vec<Lang>> + 'static + Send,
    {
        Self {
            update: Box::new(update),
            finalize: Box::new(finalize),
            reference,
            acc: initial,
        }
    }
}

impl<R, T> UndoBuilder for IncrementalChangeAction<R, T> {
    fn build(&self) -> Option<Vec<Lang>> {
        (self.finalize)(&self.reference, &self.acc)
    }

    fn next(&mut self, event: &Lang) -> bool {
        if let Some(new) = (self.update)(&self.reference, &self.acc, event) {
            self.acc = new;
            true
        } else {
            false
        }
    }

    fn more(&self) -> bool {
        true
    }
}

/// An action that can be built from a single response to a call. It can take
/// events until the value has been filled.
pub struct CallResponseAction<R, T> {
    fill: Box<dyn Fn(&R, &Lang) -> Option<T> + Send>,
    finalize: Box<dyn Fn(&R, &T) -> Vec<Lang> + Send>,
    reference: R,
    value: Option<T>,
}

impl<R, T> CallResponseAction<R, T> {
    pub fn new<F, G>(reference: R, fill: F, finalize: G) -> Self
    where
        F: Fn(&R, &Lang) -> Option<T> + 'static + Send,
        G: Fn(&R, &T) -> Vec<Lang> + 'static + Send,
    {
        Self {
            fill: Box::new(fill),
            finalize: Box::new(finalize),
            reference,
            value: None,
        }
    }
}

impl<R, T> UndoBuilder for CallResponseAction<R, T> {
    fn build(&self) -> Option<Vec<Lang>> {
        self.value
            .as_ref()
            .map(|v| (self.finalize)(&self.reference, v))
    }

    fn next(&mut self, event: &Lang) -> bool {
        if let Some(v) = (self.fill)(&self.reference, event) {
            self.value = Some(v);
            true
        } else {
            false
        }
    }

    fn more(&self) -> bool {
        self.value.is_none()
    }
}

/// An action that can be built from a multiple responses to a call.
pub struct CallMultiResponseAction<R, T> {
    next: Box<dyn Fn(&R, &Lang) -> Option<T> + Send>,
    finalize: Box<dyn Fn(&R, &[T]) -> Option<Vec<Lang>> + Send>,
    reference: R,
    values: Vec<T>,
}

impl<R, T> CallMultiResponseAction<R, T> {
    pub fn new<F, G>(reference: R, next: F, finalize: G) -> Self
    where
        F: Fn(&R, &Lang) -> Option<T> + 'static + Send,
        G: Fn(&R, &[T]) -> Option<Vec<Lang>> + 'static + Send,
    {
        Self {
            next: Box::new(next),
            finalize: Box::new(finalize),
            reference,
            values: Vec::new(),
        }
    }
}

impl<R, T> UndoBuilder for CallMultiResponseAction<R, T> {
    fn build(&self) -> Option<Vec<Lang>> {
        (self.finalize)(&self.reference, &self.values)
    }

    fn next(&mut self, event: &Lang) -> bool {
        if let Some(v) = (self.next)(&self.reference, event) {
            self.values.push(v);
            true
        } else {
            false
        }
    }

    fn more(&self) -> bool {
        true
    }
}

impl UndoAction {
    pub fn parameter_change_action(res: &Resource<Param>, from: &[u8], to: &[u8]) -> Self {
        Self::Building(Box::new(IncrementalChangeAction::new(
            res.clone(),
            (from.to_vec(), to.to_vec()),
            |r, (from, _), ev| match ev {
                Lang::UserNodeEvent(UserNodeEvent::ParameterChange(new_res, _, new))
                    if new_res == r =>
                {
                    Some((from.clone(), new.clone()))
                }
                _ => None,
            },
            |r, (from, to)| {
                Some(vec![Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                    r.clone(),
                    to.clone(),
                    from.clone(),
                ))])
            },
        )))
    }

    pub fn new_node_action(graph: &Resource<Graph>) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            graph.clone(),
            |graph, event| match event {
                Lang::GraphEvent(GraphEvent::NodeAdded(res, _, _, _, _))
                    if res.is_node_of(&graph) =>
                {
                    Some(res.clone())
                }
                _ => None,
            },
            |_, node| vec![Lang::UserNodeEvent(UserNodeEvent::RemoveNode(node.clone()))],
        )))
    }

    pub fn disconnect_sink_action(sink: &Resource<Socket>) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            sink.clone(),
            |sink, event| match event {
                Lang::GraphEvent(GraphEvent::DisconnectedSockets(source, other_sink))
                    if sink == other_sink =>
                {
                    Some(source.clone())
                }
                _ => None,
            },
            |sink, source| {
                vec![Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(
                    source.clone(),
                    sink.clone(),
                ))]
            },
        )))
    }

    pub fn connect_sockets_action(source: &Resource<Socket>, sink: &Resource<Socket>) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            (source.clone(), sink.clone()),
            |(u_source, u_sink), event| match event {
                Lang::GraphEvent(GraphEvent::ConnectedSockets(source, sink))
                    if u_source == source && u_sink == sink =>
                {
                    Some(())
                }
                _ => None,
            },
            |(_, sink), _| {
                vec![Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                    sink.clone(),
                ))]
            },
        )))
    }

    pub fn remove_node_action(node: &Resource<Node>) -> Self {
        use itertools::Itertools;

        enum RemovalData {
            NodeData(Operator, (f64, f64)),
            Connection(Resource<Socket>, Resource<Socket>),
        }

        fn cmp_removal_data(a: &RemovalData, b: &RemovalData) -> std::cmp::Ordering {
            match (a, b) {
                (RemovalData::NodeData(..), _) => std::cmp::Ordering::Less,
                (_, RemovalData::NodeData(..)) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            }
        }

        Self::Building(Box::new(CallMultiResponseAction::new(
            node.clone(),
            |node, event| match event {
                Lang::GraphEvent(GraphEvent::NodeRemoved(res, op, pos)) if node == res => {
                    Some(RemovalData::NodeData(op.clone(), pos.clone()))
                }
                Lang::GraphEvent(GraphEvent::DisconnectedSockets(source, sink))
                    if source.is_socket_of(node) || sink.is_socket_of(node) =>
                {
                    Some(RemovalData::Connection(source.clone(), sink.clone()))
                }
                _ => None,
            },
            |node, data| {
                if data
                    .iter()
                    .find(|x| matches!(x, RemovalData::NodeData(..)))
                    .is_some()
                {
                    Some(
                        data.iter()
                            .sorted_by(|a, b| cmp_removal_data(a, b))
                            .map(|x| match x {
                                RemovalData::NodeData(op, pos) => {
                                    Lang::UserNodeEvent(UserNodeEvent::NewNode(
                                        node.node_graph(),
                                        op.clone(),
                                        pos.clone(),
                                        None,
                                        node.file().map(|x| x.to_string()),
                                    ))
                                }
                                RemovalData::Connection(source, sink) => Lang::UserNodeEvent(
                                    UserNodeEvent::ConnectSockets(source.clone(), sink.clone()),
                                ),
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            },
        )))
    }

    pub fn connect_between_sockets_action(
        node: &Resource<Node>,
        source: &Resource<Socket>,
        sink: &Resource<Socket>,
    ) -> Self {
        use itertools::Itertools;

        enum BetweenSocketsData {
            Connection(Resource<Socket>),
            Disconnection(Resource<Socket>, Resource<Socket>),
        }

        fn cmp_between_sockets_data(
            a: &BetweenSocketsData,
            b: &BetweenSocketsData,
        ) -> std::cmp::Ordering {
            match (a, b) {
                (BetweenSocketsData::Connection(..), BetweenSocketsData::Disconnection(..)) => {
                    std::cmp::Ordering::Less
                }
                (BetweenSocketsData::Disconnection(..), BetweenSocketsData::Connection(..)) => {
                    std::cmp::Ordering::Greater
                }
                _ => std::cmp::Ordering::Equal,
            }
        }
        Self::Building(Box::new(CallMultiResponseAction::new(
            (node.clone(), source.clone(), sink.clone()),
            |(node, original_source, original_sink), event| match event {
                Lang::GraphEvent(GraphEvent::ConnectedSockets(source, sink))
                    if source.is_socket_of(node) || sink.is_socket_of(node) =>
                {
                    Some(BetweenSocketsData::Connection(sink.clone()))
                }
                Lang::GraphEvent(GraphEvent::DisconnectedSockets(source, sink))
                    if source == original_source && sink == original_sink =>
                {
                    Some(BetweenSocketsData::Disconnection(
                        source.clone(),
                        sink.clone(),
                    ))
                }
                _ => None,
            },
            |(_, _, _), data| {
                Some(
                    data.iter()
                        .sorted_by(|a, b| cmp_between_sockets_data(a, b))
                        .map(|x| match x {
                            BetweenSocketsData::Connection(sink) => Lang::UserNodeEvent(
                                UserNodeEvent::DisconnectSinkSocket(sink.clone()),
                            ),
                            BetweenSocketsData::Disconnection(source, sink) => Lang::UserNodeEvent(
                                UserNodeEvent::ConnectSockets(source.clone(), sink.clone()),
                            ),
                        })
                        .collect(),
                )
            },
        )))
    }

    pub fn quick_combine_action(operator: &Operator) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            operator.clone(),
            |op, event| match event {
                Lang::GraphEvent(GraphEvent::NodeAdded(node, added_op, _, _, _))
                    if op == added_op =>
                {
                    Some(node.clone())
                }
                _ => None,
            },
            |_, node| vec![Lang::UserNodeEvent(UserNodeEvent::RemoveNode(node.clone()))],
        )))
    }

    pub fn rename_node_action(from: &Resource<Node>, to: &Resource<Node>) -> Self {
        Self::Building(Box::new(IncrementalChangeAction::new(
            (),
            (from.clone(), to.clone()),
            |_, (z_from, z_to), event| match event {
                Lang::GraphEvent(GraphEvent::NodeRenamed(from, to))
                    if from == z_from && to == z_to =>
                {
                    Some((from.clone(), to.clone()))
                }
                Lang::GraphEvent(GraphEvent::NodeRenamed(from, to)) if from == z_to => {
                    Some((z_from.clone(), to.clone()))
                }
                Lang::UserNodeEvent(UserNodeEvent::RenameNode(from, _)) if from == z_to => {
                    Some((z_from.clone(), z_to.clone()))
                }
                _ => None,
            },
            |_, (from, to)| {
                Some(vec![Lang::UserNodeEvent(UserNodeEvent::RenameNode(
                    to.clone(),
                    from.clone(),
                ))])
            },
        )))
    }

    pub fn extract_action(nodes: &[Resource<Node>]) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            nodes[0].node_graph(),
            |graph, event| match event {
                Lang::GraphEvent(GraphEvent::NodeAdded(
                    res,
                    Operator::ComplexOperator(op),
                    _,
                    _,
                    _,
                )) if res.is_node_of(graph) => Some((res.clone(), op.clone())),
                _ => None,
            },
            |_, (node, op)| {
                vec![Lang::UserGraphEvent(UserGraphEvent::Inject(
                    node.clone(),
                    op.clone(),
                ))]
            },
        )))
    }

    pub fn add_graph_action() -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            (),
            |_, event| match event {
                Lang::GraphEvent(GraphEvent::GraphAdded(g)) => Some(g.clone()),
                _ => None,
            },
            |_, g| vec![Lang::UserGraphEvent(UserGraphEvent::DeleteGraph(g.clone()))],
        )))
    }

    pub fn rename_graph_action(from: &Resource<Graph>, to: &Resource<Graph>) -> Self {
        Self::Building(Box::new(IncrementalChangeAction::new(
            (),
            (from.clone(), to.clone()),
            |_, (z_from, z_to), event| match event {
                Lang::GraphEvent(GraphEvent::GraphRenamed(from, to))
                    if from == z_from && to == z_to =>
                {
                    Some((from.clone(), to.clone()))
                }
                Lang::GraphEvent(GraphEvent::GraphRenamed(from, to)) if from == z_to => {
                    Some((z_from.clone(), to.clone()))
                }
                Lang::UserGraphEvent(UserGraphEvent::RenameGraph(from, _)) if from == z_to => {
                    Some((z_from.clone(), z_to.clone()))
                }
                _ => None,
            },
            |_, (from, to)| {
                Some(vec![Lang::UserGraphEvent(UserGraphEvent::RenameGraph(
                    to.clone(),
                    from.clone(),
                ))])
            },
        )))
    }

    pub fn add_image_resource_action(path: &std::path::PathBuf) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            path.clone(),
            |path, event| match event {
                Lang::ComputeEvent(ComputeEvent::ImageResourceAdded(res, _, false))
                    if res.file() == path.file_name().and_then(|x| x.to_str()) =>
                {
                    Some(res.clone())
                }
                _ => None,
            },
            |_, res| {
                vec![Lang::UserIOEvent(UserIOEvent::RemoveImageResource(
                    res.clone(),
                ))]
            },
        )))
    }

    pub fn add_svg_resource_action(path: &std::path::PathBuf) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            path.clone(),
            |path, event| match event {
                Lang::ComputeEvent(ComputeEvent::SvgResourceAdded(res, false))
                    if res.file() == path.file_name().and_then(|x| x.to_str()) =>
                {
                    Some(res.clone())
                }
                _ => None,
            },
            |_, res| {
                vec![Lang::UserIOEvent(UserIOEvent::RemoveSvgResource(
                    res.clone(),
                ))]
            },
        )))
    }

    pub fn new_export_spec_action(spec: &ExportSpec) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            spec.clone(),
            |u_spec, event| match event {
                Lang::SurfaceEvent(SurfaceEvent::ExportSpecDeclared(spec))
                    if u_spec.node == spec.node =>
                {
                    Some(spec.name.clone())
                }
                _ => None,
            },
            |_, name| {
                vec![Lang::UserIOEvent(UserIOEvent::RemoveExportSpec(
                    name.clone(),
                ))]
            },
        )))
    }

    pub fn remove_export_spec_action(name: &str) -> Self {
        Self::Building(Box::new(CallResponseAction::new(
            name.to_string(),
            |name, event| match event {
                Lang::SurfaceEvent(SurfaceEvent::ExportSpecRemoved(spec)) if &spec.name == name => {
                    Some(spec.clone())
                }
                _ => None,
            },
            |_, spec| {
                vec![Lang::UserIOEvent(UserIOEvent::NewExportSpec(
                    spec.clone(),
                    true,
                ))]
            },
        )))
    }

    pub fn update_export_spec_action(name: &str, spec: &ExportSpec) -> Self {
        Self::Building(Box::new(IncrementalChangeAction::new(
            name.to_string(),
            (spec.clone(), spec.clone()),
            |initial_name, (initial, current), event| match event {
                Lang::SurfaceEvent(SurfaceEvent::ExportSpecUpdated(from, to))
                    if &from.name == initial_name && to == current =>
                {
                    Some((from.clone(), to.clone()))
                }
                Lang::SurfaceEvent(SurfaceEvent::ExportSpecUpdated(from, to))
                    if from == current =>
                {
                    Some((initial.clone(), to.clone()))
                }
                Lang::UserIOEvent(UserIOEvent::UpdateExportSpec(n, _)) if n == &current.name => {
                    Some((initial.clone(), current.clone()))
                }
                _ => None,
            },
            |_, (initial, last)| {
                Some(vec![Lang::UserIOEvent(UserIOEvent::UpdateExportSpec(
                    last.name.clone(),
                    initial.clone(),
                ))])
            },
        )))
    }

    pub fn add_layers_action() -> UndoAction {
        Self::Building(Box::new(CallResponseAction::new(
            (),
            |_, event| match event {
                Lang::LayersEvent(LayersEvent::LayersAdded(res, _, _)) => Some(res.clone()),
                _ => None,
            },
            |_, res| {
                vec![Lang::UserLayersEvent(UserLayersEvent::DeleteLayers(
                    res.clone(),
                ))]
            },
        )))
    }

    pub fn push_layer_action(graph: &Resource<Graph>) -> UndoAction {
        Self::Building(Box::new(CallResponseAction::new(
            graph.clone(),
            |g, event| match event {
                Lang::LayersEvent(LayersEvent::LayerPushed(res, _, _, _, _, _, _, _))
                    if res.is_node_of(g) =>
                {
                    Some(res.clone())
                }
                _ => None,
            },
            |_, res| {
                vec![Lang::UserLayersEvent(UserLayersEvent::RemoveLayer(
                    res.clone(),
                ))]
            },
        )))
    }

    pub fn push_mask_action(parent: &Resource<Node>) -> UndoAction {
        Self::Building(Box::new(CallResponseAction::new(
            parent.clone(),
            |p, event| match event {
                Lang::LayersEvent(LayersEvent::MaskPushed(parent_node, res, _, _, _, _, _, _))
                    if parent_node == p =>
                {
                    Some(res.clone())
                }
                _ => None,
            },
            |_, res| {
                vec![Lang::UserLayersEvent(UserLayersEvent::RemoveMask(
                    res.clone(),
                ))]
            },
        )))
    }

    pub fn set_layer_opacity_action(layer: &Resource<Node>, from: f32, to: f32) -> UndoAction {
        Self::Building(Box::new(IncrementalChangeAction::new(
            layer.clone(),
            (from, to),
            |layer, (initial, _), event| match event {
                Lang::UserLayersEvent(UserLayersEvent::SetOpacity(l, _, to)) if l == layer => {
                    Some((*initial, *to))
                }
                _ => None,
            },
            |layer, (from, to)| {
                Some(vec![Lang::UserLayersEvent(UserLayersEvent::SetOpacity(
                    layer.clone(),
                    *to,
                    *from,
                ))])
            },
        )))
    }

    pub fn set_layer_blend_mode_action(
        layer: &Resource<Node>,
        from: BlendMode,
        to: BlendMode,
    ) -> UndoAction {
        Self::Complete(vec![Lang::UserLayersEvent(UserLayersEvent::SetBlendMode(
            layer.clone(),
            to,
            from,
        ))])
    }
}
