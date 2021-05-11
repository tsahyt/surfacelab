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
