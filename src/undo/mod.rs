use std::thread;

use crate::{broker, lang::*};

pub mod actions;

pub use actions::*;

pub fn start_undo_thread(broker: &mut broker::Broker<Lang>) -> thread::JoinHandle<()> {
    let (sender, receiver, disconnector) = broker.subscribe("undo");
    thread::Builder::new()
        .name("undo".to_string())
        .spawn(move || {
            log::info!("Starting Undo manager");

            let mut undo_stack = UndoStack::new();

            for event in receiver {
                match &*event {
                    Lang::UserIOEvent(UserIOEvent::OpenSurface(..))
                    | Lang::UserIOEvent(UserIOEvent::NewSurface) => {
                        undo_stack.clear();
                    }
                    Lang::UserIOEvent(UserIOEvent::Undo) => {
                        if let Some(mut evs) = undo_stack.pop() {
                            log::debug!("Performing undo");
                            for ev in evs.drain(0..) {
                                sender.send(ev).unwrap()
                            }
                        } else {
                            log::debug!("Undo stack empty");
                        }
                    }
                    event => undo_stack.notify_event(event),
                }
            }

            log::info!("Undo manager terminating");
            disconnector.disconnect();
        })
        .expect("Failed to start Undo manager thread!")
}

pub trait UndoBuilder {
    /// Complete the building process, yielding events for a complete undo action
    fn build(&self) -> Option<Vec<Lang>>;

    /// Perform the next step in the building process, supplying the next read
    /// event. Returns whether the event was among the expected set of events.
    fn next(&mut self, event: &Lang) -> bool;

    /// Returns whether the builder can accept more events
    fn more(&self) -> bool;
}

pub enum UndoAction {
    Complete(Vec<Lang>),
    Building(Box<dyn UndoBuilder + Send>),
}

impl UndoAction {
    pub fn from_event(event: &Lang) -> Option<Self> {
        match event {
            Lang::UserNodeEvent(UserNodeEvent::ParameterChange(res, from, to)) => {
                Some(Self::parameter_change_action(res, from, to))
            }
            Lang::UserNodeEvent(UserNodeEvent::NewNode(g, _, _, _, _)) => {
                Some(Self::new_node_action(g))
            }
            Lang::UserNodeEvent(UserNodeEvent::RemoveNode(node)) => {
                Some(Self::remove_node_action(node))
            }
            Lang::UserNodeEvent(UserNodeEvent::DissolveNode(node)) => {
                Some(Self::remove_node_action(node))
            }
            Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(source, sink)) => {
                Some(Self::connect_sockets_action(source, sink))
            }
            Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(sink)) => {
                Some(Self::disconnect_sink_action(sink))
            }
            Lang::UserNodeEvent(UserNodeEvent::ConnectBetweenSockets(node, source, sink)) => {
                Some(Self::connect_between_sockets_action(node, source, sink))
            }
            Lang::UserNodeEvent(UserNodeEvent::QuickCombine(op, _, _)) => {
                Some(Self::quick_combine_action(op))
            }
            Lang::UserNodeEvent(UserNodeEvent::RenameNode(from, to)) => {
                Some(Self::rename_node_action(from, to))
            }
            Lang::UserGraphEvent(UserGraphEvent::Extract(ns)) => Some(Self::extract_action(ns)),
            Lang::UserGraphEvent(UserGraphEvent::RenameGraph(from, to)) => {
                Some(Self::rename_graph_action(from, to))
            }
            Lang::UserGraphEvent(UserGraphEvent::AddGraph)
            | Lang::UserLayersEvent(UserLayersEvent::Convert(..)) => Some(Self::add_graph_action()),
            Lang::UserGraphEvent(UserGraphEvent::ExposeParameter(param, _, _, _)) => {
                Some(Self::expose_parameter_action(param))
            }
            Lang::UserGraphEvent(UserGraphEvent::ConcealParameter(graph, field)) => {
                Some(Self::conceal_parameter_action(graph, field))
            }
            Lang::UserGraphEvent(UserGraphEvent::RefieldParameter(graph, field, to)) => {
                Some(Self::refield_parameter_action(graph, field, to))
            }
            Lang::UserGraphEvent(UserGraphEvent::RetitleParameter(graph, field, from, to)) => {
                Some(Self::retitle_parameter_action(graph, field, from, to))
            }
            Lang::UserIOEvent(UserIOEvent::AddImageResource(path)) => {
                Some(Self::add_image_resource_action(path))
            }
            Lang::UserIOEvent(UserIOEvent::AddSvgResource(path)) => {
                Some(Self::add_svg_resource_action(path))
            }
            Lang::UserIOEvent(UserIOEvent::NewExportSpec(spec, _)) => {
                Some(Self::new_export_spec_action(spec))
            }
            Lang::UserIOEvent(UserIOEvent::RemoveExportSpec(spec_name)) => {
                Some(Self::remove_export_spec_action(spec_name))
            }
            Lang::UserIOEvent(UserIOEvent::UpdateExportSpec(name, spec)) => {
                Some(Self::update_export_spec_action(name, spec))
            }
            Lang::UserLayersEvent(UserLayersEvent::AddLayers) => Some(Self::add_layers_action()),
            Lang::UserLayersEvent(UserLayersEvent::PushLayer(g, _, _)) => {
                Some(Self::push_layer_action(g))
            }
            Lang::UserLayersEvent(UserLayersEvent::PushMask(parent, _)) => {
                Some(Self::push_mask_action(parent))
            }
            Lang::UserLayersEvent(UserLayersEvent::SetOpacity(layer, from, to)) => {
                Some(Self::set_layer_opacity_action(layer, *from, *to))
            }
            Lang::UserLayersEvent(UserLayersEvent::SetBlendMode(layer, from, to)) => {
                Some(Self::set_layer_blend_mode_action(layer, *from, *to))
            }
            Lang::UserLayersEvent(UserLayersEvent::SetTitle(layer, from, to)) => {
                Some(Self::set_layer_title_action(layer, from, to))
            }
            Lang::UserLayersEvent(UserLayersEvent::SetEnabled(layer, from, to)) => {
                Some(Self::set_layer_enabled_action(layer, *from, *to))
            }
            _ => None,
        }
    }

    /// Build this action if possible. If the action is already built, nothing
    /// is done. May fail if the action is not buildable.
    pub fn build(&mut self) -> Option<()> {
        match self {
            UndoAction::Complete(_) => {}
            UndoAction::Building(b) => {
                *self = b.build().map(|vs| Self::Complete(vs))?;
            }
        }

        Some(())
    }

    /// Returns whether the action can accept more events
    pub fn more(&self) -> bool {
        match self {
            UndoAction::Complete(_) => false,
            UndoAction::Building(b) => b.more(),
        }
    }
}

pub struct UndoStack {
    stack: Vec<UndoAction>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.stack.clear()
    }

    /// Notify undo stack of a new event from the bus.
    pub fn notify_event(&mut self, event: &Lang) {
        match self.stack.last_mut() {
            Some(UndoAction::Building(builder)) => {
                if !builder.next(event) {
                    if let Some(new) = UndoAction::from_event(event) {
                        self.stack.last_mut().unwrap().build();
                        self.stack.push(new);
                    }
                }
            }
            _ => {
                if let Some(new) = UndoAction::from_event(event) {
                    self.stack.push(new);
                }
            }
        }

        if self.stack.last().map(|x| !x.more()).unwrap_or(false) {
            self.stack.last_mut().unwrap().build();
        }
    }

    /// Pop an element off the undo stack.
    pub fn pop(&mut self) -> Option<Vec<Lang>> {
        // Attempt building the topmost action first
        self.stack.last_mut().and_then(|x| x.build());

        // Fetch action and return
        self.stack.pop().and_then(|x| match x {
            UndoAction::Complete(evs) => Some(evs),
            _ => None,
        })
    }
}
