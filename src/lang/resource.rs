use serde_derive::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub type ResourcePart = String;

pub trait Scheme {
    fn scheme_name() -> &'static str;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Node;

impl Scheme for Node {
    fn scheme_name() -> &'static str {
        "node"
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Graph;

impl Scheme for Graph {
    fn scheme_name() -> &'static str {
        "graph"
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Param;

impl Scheme for Param {
    fn scheme_name() -> &'static str {
        "param"
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Socket;

impl Scheme for Socket {
    fn scheme_name() -> &'static str {
        "socket"
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resource<S> {
    resource_path: PathBuf,
    fragment: Option<String>,
    phantom_data: std::marker::PhantomData<S>,
}

impl<S> Clone for Resource<S> {
    fn clone(&self) -> Self {
        Self {
            resource_path: self.resource_path.clone(),
            fragment: self.fragment.clone(),
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl<S> std::fmt::Display for Resource<S>
where
    S: Scheme,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(frag) = &self.fragment {
            write!(
                f,
                "{}:{}:{}",
                S::scheme_name(),
                self.resource_path.to_str().unwrap(),
                frag
            )
        } else {
            write!(
                f,
                "{}:{}",
                S::scheme_name(),
                self.resource_path.to_str().unwrap()
            )
        }
    }
}

impl Resource<Node> {
    pub fn node<P: AsRef<Path>>(path: P, fragment: Option<String>) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment,
            phantom_data: std::marker::PhantomData,
        }
    }

    pub fn node_socket(&self, socket: &str) -> Resource<Socket> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: Some(socket.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }

    pub fn node_graph(&self) -> Resource<Graph> {
        let mut path = self.resource_path.clone();
        path.pop();
        Resource {
            resource_path: path,
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }

    pub fn node_parameter(&self, parameter: &str) -> Resource<Param> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: Some(parameter.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl Resource<Graph> {
    pub fn graph<P: AsRef<Path>>(path: P, fragment: Option<String>) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment,
            phantom_data: std::marker::PhantomData,
        }
    }

    pub fn graph_node(&self, node: &str) -> Resource<Node> {
        let mut path = self.resource_path.clone();
        path.push(node);

        Resource {
            resource_path: path,
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl Resource<Param> {
    pub fn parameter<P: AsRef<Path>>(path: P, fragment: &str) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: Some(fragment.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }

    pub fn parameter_node(&self) -> Resource<Node> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl Resource<Socket> {
    pub fn socket<P: AsRef<Path>>(path: P, fragment: &str) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: Some(fragment.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }

    pub fn socket_node(&self) -> Resource<Node> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl<S> Resource<S> {
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_ref().map(|x| x.as_ref())
    }

    pub fn path(&self) -> &Path {
        &self.resource_path
    }

    pub fn path_mut(&mut self) -> &mut PathBuf {
        &mut self.resource_path
    }

    pub fn path_str(&self) -> Option<&str> {
        self.path().to_str()
    }

    pub fn unregistered_node() -> Resource<S> {
        Resource {
            resource_path: PathBuf::from("__unregistered__"),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }

    pub fn modify_path<F: FnOnce(&mut PathBuf) -> ()>(&mut self, func: F) {
        func(&mut self.resource_path);
    }

    pub fn file(&self) -> Option<&str> {
        self.path().file_name().and_then(|x| x.to_str())
    }

    pub fn directory(&self) -> Option<&str> {
        self.path().parent().and_then(|x| x.to_str())
    }

    pub fn set_graph<P: AsRef<Path>>(&mut self, graph: P) {
        let file = self.resource_path.file_name().unwrap();
        let mut path = graph.as_ref().to_path_buf();
        path.push(file);
        self.resource_path = path;
    }
}
