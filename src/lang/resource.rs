use serde_derive::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub type ResourcePart = String;

/// The UTF-8 representation of a Scheme. Used for resource rendering
pub trait Scheme: PartialEq {
    fn scheme_name() -> &'static str;
}

/// Resource types that are contained in a graph
pub trait InGraph {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Node;

impl Scheme for Node {
    fn scheme_name() -> &'static str {
        "node"
    }
}

impl InGraph for Node {}

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

impl InGraph for Param {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Socket;

impl Scheme for Socket {
    fn scheme_name() -> &'static str {
        "socket"
    }
}

impl InGraph for Socket {}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Img;

impl Scheme for Img {
    fn scheme_name() -> &'static str {
        "img"
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Svg;

impl Scheme for Svg {
    fn scheme_name() -> &'static str {
        "svg"
    }
}

/// A Resource describes some thing in the system. This could refer to a node,
/// or to a graph/layer stack, or to a socket of a node, or to a parameter, etc.
///
/// The type parameter narrows down the type of resource that can be described.
///
/// A resource has a schema, depending on the type of resource.
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
    /// Constructor for a node resource
    pub fn node<P: AsRef<Path>>(path: P) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Obtain a socket resource from a node resource
    pub fn node_socket(&self, socket: &str) -> Resource<Socket> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: Some(socket.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Retrieve the parent graph resource from a node resource
    pub fn node_graph(&self) -> Resource<Graph> {
        let mut path = self.resource_path.clone();
        path.pop();
        Resource {
            resource_path: path,
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Obtain a parameter resource from a node resource
    pub fn node_parameter(&self, parameter: &str) -> Resource<Param> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: Some(parameter.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Determine whether this socket is a socket of the given node
    pub fn is_node_of(&self, graph: &Resource<Graph>) -> bool {
        self.resource_path.starts_with(&graph.resource_path)
    }
}

impl Resource<Graph> {
    /// Constructor for a graph resource
    pub fn graph<P: AsRef<Path>>(path: P) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Obtain a node resource from a graph resource
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
    /// Constructor for a parameter resource
    pub fn parameter<P: AsRef<Path>>(path: P, fragment: &str) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: Some(fragment.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Obtain the parent node resource from a parameter
    pub fn parameter_node(&self) -> Resource<Node> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl Resource<Socket> {
    /// Constructor for a socket resource
    pub fn socket<P: AsRef<Path>>(path: P, fragment: &str) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: Some(fragment.to_string()),
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Obtain the parent node resource from a socket resource
    pub fn socket_node(&self) -> Resource<Node> {
        Resource {
            resource_path: self.resource_path.clone(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }

    /// Determine whether this socket is a socket of the given node
    pub fn is_socket_of(&self, node: &Resource<Node>) -> bool {
        self.resource_path.starts_with(&node.resource_path)
    }
}

impl Resource<Img> {
    /// Constructor for an image resource
    pub fn image<P: AsRef<Path>>(path: P) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl Resource<Svg> {
    /// Constructor for an SVG resource
    pub fn svg<P: AsRef<Path>>(path: P) -> Self {
        Self {
            resource_path: path.as_ref().to_path_buf(),
            fragment: None,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl<S> Resource<S> {
    /// Get the fragment part of a resource if it exists
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_ref().map(|x| x.as_ref())
    }

    /// Obtain a view of the path of a resource
    pub fn path(&self) -> &Path {
        &self.resource_path
    }

    /// Rename the file component of the resource
    pub fn rename_file(&mut self, new: &str) {
        self.resource_path.set_file_name(new);
    }

    /// Obtain a mutable view of the path of a resource
    pub fn path_mut(&mut self) -> &mut PathBuf {
        &mut self.resource_path
    }

    /// Render the resource path to a string
    pub fn path_str(&self) -> Option<&str> {
        self.path().to_str()
    }

    /// Obtain the file part of the path of a resource, if it exists
    pub fn file(&self) -> Option<&str> {
        self.path().file_name().and_then(|x| x.to_str())
    }

    /// Obtain the directory part of the path of a resource, if it exists
    pub fn directory(&self) -> Option<&str> {
        self.path().parent().and_then(|x| x.to_str())
    }

    /// Cast between resource types. Note that this does not perform *any*
    /// checks. Usually it is wiser to use one of the specialized casting
    /// functions. Handle with care!
    pub fn cast_unchecked<T>(self) -> Resource<T> {
        Resource {
            resource_path: self.resource_path,
            fragment: self.fragment,
            phantom_data: std::marker::PhantomData,
        }
    }
}

impl<S: InGraph> Resource<S> {
    /// Set the parent graph of this resource.
    pub fn set_graph<P: AsRef<Path>>(&mut self, graph: P) {
        let file = self.resource_path.file_name().unwrap();
        let mut path = graph.as_ref().to_path_buf();
        path.push(file);
        self.resource_path = path;
    }
}
