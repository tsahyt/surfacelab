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

trait UndoBuilder {
    /// Complete the building process, yielding events for a complete undo action
    fn build(&self) -> Option<Vec<Lang>>;

    /// Perform the next step in the building process, supplying the next read
    /// event. Returns whether the event was among the expected set of events.
    fn next(&mut self, event: &Lang) -> bool;

    /// Returns whether the builder can accept more events
    fn more(&self) -> bool;
}

enum UndoAction {
    Complete(Vec<Lang>),
    Building(Box<dyn UndoBuilder + Send>),
}

impl UndoAction {
    pub fn from_event(event: &Lang) -> Option<Self> {
        match event {
            Lang::UserNodeEvent(UserNodeEvent::ParameterChange(res, from, to)) => {
                Some(Self::Building(Box::new(IncrementalChangeAction::new(
                    res.clone(),
                    (from.clone(), to.clone()),
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
                ))))
            }
            Lang::UserRenderEvent(UserRenderEvent::Rotate(renderer, theta, phi)) => {
                Some(Self::Building(Box::new(IncrementalChangeAction::new(
                    *renderer,
                    (*theta, *phi),
                    |r, (theta, phi), ev| match ev {
                        Lang::UserRenderEvent(UserRenderEvent::Rotate(new_r, t, p))
                            if r == new_r =>
                        {
                            Some((theta + t, phi + p))
                        }
                        _ => None,
                    },
                    |r, (theta, phi)| {
                        Some(vec![Lang::UserRenderEvent(UserRenderEvent::Rotate(
                            *r, -theta, -phi,
                        ))])
                    },
                ))))
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
