use crate::lang::*;

pub struct UndoAction(Vec<Lang>);

pub struct UndoStack {
    stack: Vec<UndoAction>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Notify undo stack of a new event from the bus.
    pub fn notify_event(&mut self, event: &Lang) {
        match event {
            Lang::UserNodeEvent(UserNodeEvent::ParameterChange(param, from, to)) => {}
            _ => {}
        }
    }

    /// Pop an element off the undo stack.
    pub fn pop(&mut self) -> Option<Vec<Lang>> {
        self.stack.pop().map(|x| x.0)
    }
}
