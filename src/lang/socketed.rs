use enum_dispatch::*;
use std::collections::HashMap;

pub use super::{ImageType, OperatorType, TypeVariable};

#[enum_dispatch]
/// The Socketed trait defines functions related to things with sockets,
/// primarily nodes. This is not strictly limited to atomic operators and such.
///
/// It exists primarily to abstract the common functionality between atomic
/// operators and complex operators.
pub trait Socketed {
    /// Get the input sockets of this Socketed type, in the form of a HashMap
    /// from Strings (socket names) to their types.
    fn inputs(&self) -> HashMap<String, OperatorType>;

    /// Get the output sockets of this Socketed type, in the form of a HashMap
    /// from Strings (socket names) to their types.
    fn outputs(&self) -> HashMap<String, OperatorType>;

    /// Get all socket (names) that use a certain type variable
    fn sockets_by_type_variable(&self, var: TypeVariable) -> Vec<String> {
        self.inputs()
            .iter()
            .chain(self.outputs().iter())
            .filter(|(_, t)| **t == OperatorType::Polymorphic(var))
            .map(|x| x.0.to_owned())
            .collect()
    }

    /// Get the type variable of a socket, if any. Will return None for
    /// monomorphic types.
    fn type_variable_from_socket(&self, socket: &str) -> Option<TypeVariable> {
        self.inputs()
            .iter()
            .chain(self.outputs().iter())
            .filter(|(s, _)| s.as_str() == socket)
            .map(|x| x.1)
            .next()
            .and_then(|opty| match opty {
                OperatorType::Monomorphic(_) => None,
                OperatorType::Polymorphic(v) => Some(*v),
            })
    }

    /// The default name of a Socketed type is the name used for instantiating
    /// it in a graph.
    fn default_name(&self) -> &str;

    /// The title of a Socketed type is a human readable name
    fn title(&self) -> &str;

    /// Determine whether this Socketed value has external data, e.g. images,
    /// associated with it.
    fn external_data(&self) -> bool {
        false
    }

    /// Determine whether this Socketed value is an Output
    fn is_output(&self) -> bool {
        false
    }

    /// Determine whether this Socketed value is an Input
    fn is_input(&self) -> bool {
        false
    }
}

pub fn type_variable_from_socket_iter<'a, I: IntoIterator<Item = &'a (String, OperatorType)>>(
    iter: I,
    socket: &str,
) -> Option<TypeVariable> {
    iter.into_iter()
        .filter(|(s, _)| s.as_str() == socket)
        .map(|x| x.1)
        .next()
        .and_then(|opty| match opty {
            OperatorType::Monomorphic(_) => None,
            OperatorType::Polymorphic(v) => Some(v),
        })
}
