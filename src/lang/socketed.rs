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

    fn default_name<'a>(&'a self) -> &'a str;

    fn title<'a>(&'a self) -> &'a str;
}
