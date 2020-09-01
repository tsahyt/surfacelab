use enum_dispatch::*;
use std::collections::HashMap;

pub use super::{ImageType, OperatorType, TypeVariable};

#[enum_dispatch]
pub trait Socketed {
    fn inputs(&self) -> HashMap<String, OperatorType>;

    fn outputs(&self) -> HashMap<String, OperatorType>;

    fn sockets_by_type_variable(&self, var: TypeVariable) -> Vec<String> {
        self.inputs()
            .iter()
            .chain(self.outputs().iter())
            .filter(|(_, t)| **t == OperatorType::Polymorphic(var))
            .map(|x| x.0.to_owned())
            .collect()
    }

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

    fn default_name<'a>(&'a self) -> &'a str;

    fn title<'a>(&'a self) -> &'a str;

    fn external_data(&self) -> bool {
        false
    }

    fn is_output(&self) -> bool {
        false
    }
}
