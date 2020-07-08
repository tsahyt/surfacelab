use serde_derive::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::path::{Path, PathBuf};

pub type ResourcePart = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scheme {
    Node,
    Graph,
}

impl std::fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node => write!(f, "node"),
            Self::Graph => write!(f, "graph"),
        }
    }
}

impl TryFrom<&str> for Scheme {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "node" => Ok(Self::Node),
            "graph" => Ok(Self::Graph),
            _ => Err("Unknown Scheme"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resource {
    scheme: Scheme,
    resource_path: PathBuf,
    fragment: Option<String>,
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(frag) = &self.fragment {
            write!(
                f,
                "{}:{}:{}",
                self.scheme,
                self.resource_path.to_str().unwrap(),
                frag
            )
        } else {
            write!(
                f,
                "{}:{}",
                self.scheme,
                self.resource_path.to_str().unwrap()
            )
        }
    }
}

impl TryFrom<&str> for Resource {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let pieces: Vec<&str> = value.split(':').collect();

        let scheme = Scheme::try_from(
            *pieces
                .get(0)
                .ok_or("Missing schema in resource identifier")?,
        )?;
        let resource_path =
            PathBuf::from(pieces.get(1).ok_or("Missing path in resource identifier")?);
        let fragment = pieces.get(2).map(|x| (*x).to_string());

        Ok(Resource {
            scheme,
            resource_path,
            fragment,
        })
    }
}

impl Resource {
    pub fn new<P: AsRef<Path>>(scheme: Scheme, path: P, fragment: Option<String>) -> Self {
        Self {
            scheme,
            resource_path: path.as_ref().to_path_buf(),
            fragment,
        }
    }

    pub fn node<P: AsRef<Path>>(path: P, fragment: Option<String>) -> Self {
        Self::new(Scheme::Node, path, fragment)
    }

    pub fn graph<P: AsRef<Path>>(path: P, fragment: Option<String>) -> Self {
        Self::new(Scheme::Graph, path, fragment)
    }

    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_ref().map(|x| x.as_ref())
    }

    pub fn extend_fragment(&self, fragment: &str) -> Self {
        let mut new = self.clone();
        new.fragment = Some(fragment.to_string());
        new
    }

    pub fn drop_fragment(&self) -> Self {
        let mut new = self.clone();
        new.fragment = None;
        new
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    pub fn path(&self) -> &Path {
        &self.resource_path
    }

    pub fn path_str(&self) -> Option<&str> {
        self.path().to_str()
    }

    pub fn unregistered_node() -> Resource {
        Resource {
            scheme: Scheme::Node,
            resource_path: PathBuf::from("__unregistered__"),
            fragment: None,
        }
    }

    pub fn is_fragment_of(&self, other: &Resource) -> bool {
        other.scheme == self.scheme && other.resource_path == self.resource_path
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn test_resource_parse_node() {
        let x = Resource::try_from("node:/foo/bar-01").unwrap();
        assert_eq!(x.fragment, None);
        assert_eq!(x.scheme, Scheme::Node);
        assert_eq!(x.resource_path, PathBuf::from("/foo/bar-01"));
    }

    #[test]
    fn test_resource_parse_node_socket() {
        // simple
        let x = Resource::try_from("node:/foo:socket_in").unwrap();
        assert_eq!(x.fragment, Some("socket_in".to_string()));
        assert_eq!(x.scheme, Scheme::Node);
        assert_eq!(x.resource_path, PathBuf::from("/foo"));

        // in nested node
        let x = Resource::try_from("node:/foo/bar-01:socket").unwrap();
        assert_eq!(x.fragment, Some("socket".to_string()));
        assert_eq!(x.scheme, Scheme::Node);
        assert_eq!(x.resource_path, PathBuf::from("/foo/bar-01"));
    }

    #[test]
    fn test_resource_display() {
        let r = Resource {
            scheme: Scheme::Node,
            resource_path: PathBuf::from("/foo/bar"),
            fragment: Some("socket".to_string()),
        };

        assert_eq!(format!("{}", r), "node:/foo/bar:socket");
    }
}
