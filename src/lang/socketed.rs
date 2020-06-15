use std::collections::HashMap;

use super::{OperatorType, TypeVariable};

trait Socketed {
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

    fn default_name<'a>(&'a self) -> &'static str;

    fn title(&self) -> &'static str;
}
