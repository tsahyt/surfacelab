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
                Some(Self::Building(Box::new(ParameterChangeBuilder::new(
                    res.clone(),
                    from.clone(),
                    to.clone(),
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
        Self { stack: Vec::new(), ignore: HashSet::new() }
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
            },
            _ => None,
        })
    }
}

#[derive(Debug)]
struct ParameterChangeBuilder {
    res: Resource<Param>,
    first: Vec<u8>,
    last: Vec<u8>,
}

impl ParameterChangeBuilder {
    fn new(res: Resource<Param>, first: Vec<u8>, last: Vec<u8>) -> Self {
        Self { res, first, last }
    }
}

impl UndoBuilder for ParameterChangeBuilder {
    fn build(&self) -> Option<Vec<Lang>> {
        Some(vec![Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
            self.res.clone(),
            self.last.clone(),
            self.first.clone(),
        ))])
    }

    fn next(&mut self, event: &Lang) -> bool {
        match event {
            Lang::UserNodeEvent(UserNodeEvent::ParameterChange(res, _, new))
                if res == &self.res =>
            {
                self.last = new.clone();
                true
            }
            _ => false,
        }
    }

    fn more(&self) -> bool {
        true
    }
}
