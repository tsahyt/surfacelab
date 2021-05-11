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

pub fn parameter_change_action(res: &Resource<Param>, from: &[u8], to: &[u8]) -> UndoAction {
    UndoAction::Building(Box::new(IncrementalChangeAction::new(
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

pub fn camera_rotate_action(renderer: RendererID, theta: f32, phi: f32) -> UndoAction {
    UndoAction::Building(Box::new(IncrementalChangeAction::new(
        renderer,
        (theta, phi),
        |r, (theta, phi), ev| match ev {
            Lang::UserRenderEvent(UserRenderEvent::Rotate(new_r, t, p)) if r == new_r => {
                Some((theta + *t, phi + *p))
            }
            _ => None,
        },
        |r, (theta, phi)| {
            Some(vec![Lang::UserRenderEvent(UserRenderEvent::Rotate(
                *r, -theta, -phi,
            ))])
        },
    )))
}

pub fn new_node_action(graph: &Resource<Graph>) -> UndoAction {
    UndoAction::Building(Box::new(CallResponseAction::new(
        graph.clone(),
        |graph, event| match event {
            Lang::GraphEvent(GraphEvent::NodeAdded(res, _, _, _, _)) if res.is_node_of(&graph) => {
                Some(res.clone())
            }
            _ => None,
        },
        |_, node| vec![Lang::UserNodeEvent(UserNodeEvent::RemoveNode(node.clone()))],
    )))
}

pub fn disconnect_sink_action(sink: &Resource<Socket>) -> UndoAction {
    UndoAction::Building(Box::new(CallResponseAction::new(
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

pub fn connect_sockets_action(source: &Resource<Socket>, sink: &Resource<Socket>) -> UndoAction {
    UndoAction::Building(Box::new(CallResponseAction::new(
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

pub fn remove_node_action(node: &Resource<Node>) -> UndoAction {
    use itertools::Itertools;

    enum RemovalData {
        NodeData(Operator, (f64, f64)),
        Connection(Resource<Socket>, Resource<Socket>),
    }

    fn cmp_removal_data(a: &RemovalData, b: &RemovalData) -> std::cmp::Ordering {
        match (a, b) {
            (RemovalData::NodeData(..), _) => std::cmp::Ordering::Greater,
            (_, RemovalData::NodeData(..)) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Equal,
        }
    }

    UndoAction::Building(Box::new(CallMultiResponseAction::new(
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
                            RemovalData::NodeData(op, pos) => Lang::UserNodeEvent(
                                UserNodeEvent::NewNodeNamed(node.clone(), op.clone(), pos.clone()),
                            ),
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
) -> UndoAction {
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
    UndoAction::Building(Box::new(CallMultiResponseAction::new(
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
                        BetweenSocketsData::Connection(sink) => {
                            Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(sink.clone()))
                        }
                        BetweenSocketsData::Disconnection(source, sink) => Lang::UserNodeEvent(
                            UserNodeEvent::ConnectSockets(source.clone(), sink.clone()),
                        ),
                    })
                    .collect(),
            )
        },
    )))
}
