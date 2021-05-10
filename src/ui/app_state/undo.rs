use std::collections::HashSet;

use crate::lang::*;

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
    ignore: HashSet<Lang>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            ignore: HashSet::new(),
        }
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
            UndoAction::Complete(evs) => {
                // self.ignore.union(evs.iter().cloned().collect());
                Some(evs)
            }
            _ => None,
        })
    }
}

/// An incremental change action is a buildable undo action that is constructed
/// from successive small changes, e.g. changing a parameter or moving the
/// camera. It can always take more events. This mimics a fold across time,
/// potentially updating an accumulator with each new event.
struct IncrementalChangeAction<R, T> {
    update: Box<dyn Fn(&R, &T, &Lang) -> Option<T> + Send>,
    finalize: Box<dyn Fn(&R, &T) -> Option<Vec<Lang>> + Send>,
    reference: R,
    acc: T,
}

impl<R, T> IncrementalChangeAction<R, T> {
    fn new<F, G>(reference: R, initial: T, update: F, finalize: G) -> Self
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
