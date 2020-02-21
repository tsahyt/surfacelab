use maplit::hashmap;
use std::collections::HashMap;
use std::path::*;

#[repr(C)]
#[derive(Clone, Debug)]
pub struct BlendParameters {
    mix: f32,
}

impl Default for BlendParameters {
    fn default() -> Self {
        BlendParameters { mix: 0.5 }
    }
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct PerlinNoiseParameters {
    scale: f32,
    octaves: f32,
}

#[derive(Clone, Debug)]
pub enum Operator {
    Blend(BlendParameters),
    PerlinNoise(PerlinNoiseParameters),
    Image { path: std::path::PathBuf },
    Output { output_type: OutputType },
}

impl Operator {
    pub fn inputs(&self) -> HashMap<String, ImageType> {
        match self {
            Operator::Blend(..) => hashmap! {
                "color1".to_string() => ImageType::RgbaImage,
                "color2".to_string() => ImageType::RgbaImage
            },
            Operator::PerlinNoise(..) => HashMap::new(),
            Operator::Image { .. } => HashMap::new(),
            Operator::Output { output_type } => match output_type {
                OutputType::Albedo => hashmap! { "albedo".to_string() => ImageType::RgbImage },
                OutputType::Roughness => hashmap! { "roughness".to_string() => ImageType::Value },
                OutputType::Normal => hashmap! { "normal".to_string() => ImageType::RgbImage },
                OutputType::Displacement => {
                    hashmap! { "displacement".to_string() => ImageType::Value }
                }
                OutputType::Value => hashmap! { "value".to_string() => ImageType::Value },
            },
        }
    }

    pub fn outputs(&self) -> HashMap<String, ImageType> {
        match self {
            Operator::Blend(..) => hashmap! {
                "color".to_string() => ImageType::RgbaImage
            },
            Operator::PerlinNoise(..) => hashmap! { "noise".to_string() => ImageType::Value
            },
            Operator::Image { .. } => hashmap! { "image".to_string() => ImageType::RgbaImage },
            Operator::Output { .. } => HashMap::new(),
        }
    }

    pub fn default_name(&self) -> &str {
        match self {
            Operator::Blend(..) => "blend",
            Operator::PerlinNoise(..) => "perlin_noise",
            Operator::Image { .. } => "image",
            Operator::Output { .. } => "output",
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Operator::Blend(..) => "Blend",
            Operator::PerlinNoise(..) => "Perlin Noise",
            Operator::Image { .. } => "Image",
            Operator::Output { .. } => "Output",
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum ImageType {
    RgbImage,
    RgbaImage,
    Value,
}

impl Default for ImageType {
    fn default() -> Self {
        ImageType::Value
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum OutputType {
    Albedo,
    Roughness,
    Normal,
    Displacement,
    Value,
}

impl Default for OutputType {
    fn default() -> Self {
        OutputType::Value
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Resource {
    scheme: String,
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

impl std::convert::TryFrom<&str> for Resource {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let pieces: Vec<&str> = value.split(':').collect();

        let scheme = pieces
            .get(0)
            .ok_or("Missing schema in resource identifier")?
            .to_string();
        let resource_path =
            PathBuf::from(pieces.get(1).ok_or("Missing path in resource identifier")?);
        let fragment = pieces.get(2).map(|x| x.to_string());

        Ok(Resource {
            scheme,
            resource_path,
            fragment,
        })
    }
}

impl Resource {
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

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn path(&self) -> &Path {
        &self.resource_path
    }

    pub fn unregistered_node() -> Resource {
        Resource {
            scheme: "node".to_string(),
            resource_path: PathBuf::from("__unregistered__"),
            fragment: None,
        }
    }
}

/// Events concerning node operation triggered by the user
#[derive(Clone, Debug)]
pub enum UserNodeEvent {
    NewNode(Operator),
    RemoveNode(Resource),
    ConnectSockets(Resource, Resource),
    DisconnectSockets(Resource, Resource),
}

#[derive(Clone, Debug)]
pub enum GraphEvent {
    NodeAdded(Resource, Operator),
    ConnectedSockets(Resource, Resource),
}

#[derive(Clone, Debug)]
pub enum Lang {
    UserNodeEvent(UserNodeEvent),
    GraphEvent(GraphEvent),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn test_resource_parse_node() {
        let x = Resource::try_from("node:/foo/bar-01").unwrap();
        assert_eq!(x.fragment, None);
        assert_eq!(x.scheme, "node");
        assert_eq!(x.resource_path, PathBuf::from("/foo/bar-01"));
    }

    #[test]
    fn test_resource_parse_node_socket() {
        // simple
        let x = Resource::try_from("node:/foo:socket_in").unwrap();
        assert_eq!(x.fragment, Some("socket_in".to_string()));
        assert_eq!(x.scheme, "node");
        assert_eq!(x.resource_path, PathBuf::from("/foo"));

        // in nested node
        let x = Resource::try_from("node:/foo/bar-01:socket").unwrap();
        assert_eq!(x.fragment, Some("socket".to_string()));
        assert_eq!(x.scheme, "node");
        assert_eq!(x.resource_path, PathBuf::from("/foo/bar-01"));
    }

    #[test]
    fn test_resource_display() {
        let r = Resource {
            scheme: "node".to_string(),
            resource_path: PathBuf::from("/foo/bar"),
            fragment: Some("socket".to_string()),
        };

        assert_eq!(format!("{}", r), "node:/foo/bar:socket");
    }
}
