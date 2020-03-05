use maplit::hashmap;
use std::collections::HashMap;
use std::path::*;
use zerocopy::AsBytes;

pub trait Parameters {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]);
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug)]
pub struct BlendParameters {
    mix: f32,
}

impl Default for BlendParameters {
    fn default() -> Self {
        BlendParameters { mix: 0.5 }
    }
}

impl Parameters for BlendParameters {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
        match field {
            "mix" => {
                let mut arr: [u8; 4] = Default::default();
                arr.copy_from_slice(data);
                self.mix = f32::from_be_bytes(arr);
            }
            _ => panic!("Unknown field {}", field),
        }
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug)]
pub struct PerlinNoiseParameters {
    scale: f32,
    octaves: u32,
    attenuation: f32,
}

impl Default for PerlinNoiseParameters {
    fn default() -> Self {
        PerlinNoiseParameters {
            scale: 3.0,
            octaves: 2,
            attenuation: 2.0,
        }
    }
}

impl Parameters for PerlinNoiseParameters {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
        match field {
            "scale" => {
                let mut arr: [u8; 4] = Default::default();
                arr.copy_from_slice(data);
                self.scale = f32::from_be_bytes(arr);
            }
            "octaves" => {
                let mut arr: [u8; 4] = Default::default();
                arr.copy_from_slice(data);
                self.octaves = u32::from_be_bytes(arr);
            }
            "attenuation" => {
                let mut arr: [u8; 4] = Default::default();
                arr.copy_from_slice(data);
                self.attenuation = f32::from_be_bytes(arr);
            }
            _ => panic!("Unknown field {}", field),
        }
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug)]
pub struct RgbParameters {
    rgb: [f32; 3],
}

impl Default for RgbParameters {
    fn default() -> Self {
        RgbParameters {
            rgb: [0.5, 0.7, 0.3],
        }
    }
}

impl Parameters for RgbParameters {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
        match field {
            "rgb" => {
                // TODO: rgb parameter
                // let mut arr: [u8; 12] = Default::default();
                // arr.copy_from_slice(data);
                // self.rgb = f32::from_be_bytes(arr);
            }
            _ => panic!("Unknown field {}", field),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Operator {
    Blend(BlendParameters),
    PerlinNoise(PerlinNoiseParameters),
    Rgb(RgbParameters),
    Image { path: std::path::PathBuf },
    Output { output_type: OutputType },
}

impl Operator {
    pub fn inputs(&self) -> HashMap<String, ImageType> {
        match self {
            Self::Blend(..) => hashmap! {
                "color1".to_string() => ImageType::Rgba,
                "color2".to_string() => ImageType::Rgba
            },
            Self::PerlinNoise(..) => HashMap::new(),
            Self::Rgb(..) => HashMap::new(),
            Self::Image { .. } => HashMap::new(),
            Self::Output { output_type } => match output_type {
                OutputType::Albedo => hashmap! { "albedo".to_string() => ImageType::Rgb },
                OutputType::Roughness => hashmap! { "roughness".to_string() => ImageType::Value },
                OutputType::Normal => hashmap! { "normal".to_string() => ImageType::Rgb },
                OutputType::Displacement => {
                    hashmap! { "displacement".to_string() => ImageType::Value }
                }
                OutputType::Value => hashmap! { "value".to_string() => ImageType::Value },
            },
        }
    }

    pub fn outputs(&self) -> HashMap<String, ImageType> {
        match self {
            Self::Blend(..) => hashmap! {
                "color".to_string() => ImageType::Rgba
            },
            Self::Rgb(..) => hashmap! {
                "color".to_string() => ImageType::Rgb
            },
            Self::PerlinNoise(..) => hashmap! { "noise".to_string() => ImageType::Value
            },
            Self::Image { .. } => hashmap! { "image".to_string() => ImageType::Rgba },
            Self::Output { .. } => HashMap::new(),
        }
    }

    pub fn default_name<'a>(&'a self) -> &'static str {
        match self {
            Self::Blend(..) => "blend",
            Self::PerlinNoise(..) => "perlin_noise",
            Self::Rgb(..) => "rgb",
            Self::Image { .. } => "image",
            Self::Output { .. } => "output",
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Blend(..) => "Blend",
            Self::PerlinNoise(..) => "Perlin Noise",
            Self::Rgb(..) => "RGB Color",
            Self::Image { .. } => "Image",
            Self::Output { .. } => "Output",
        }
    }

    pub fn all_default() -> Vec<Self> {
        vec![
            Self::Blend(BlendParameters::default()),
            Self::PerlinNoise(PerlinNoiseParameters::default()),
            Self::Rgb(RgbParameters::default()),
            Self::Image {
                path: PathBuf::new(),
            },
            Self::Output {
                output_type: OutputType::default(),
            },
        ]
    }

    pub fn is_output(&self) -> bool {
        match self {
            Self::Output { .. } => true,
            _ => false,
        }
    }
}

impl Parameters for Operator {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]) {
        match self {
            Self::Blend(p) => p.set_parameter(field, data),
            Self::PerlinNoise(p) => p.set_parameter(field, data),
            _ => panic!("Unsupported operator for parameter setting")
        }
    }
}

#[derive(Clone, Debug)]
pub enum Instruction {
    Execute(Resource, Operator),
    Move(Resource, Resource),
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ImageType {
    Rgb,
    Rgba,
    Value,
}

impl Default for ImageType {
    fn default() -> Self {
        ImageType::Value
    }
}

impl ImageType {
    pub fn gpu_bytes_per_pixel(&self) -> u8 {
        match self {
            Self::Rgb => 8,
            Self::Rgba => 8,
            Self::Value => 4,
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
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

        let scheme = (*pieces
            .get(0)
            .ok_or("Missing schema in resource identifier")?)
        .to_string();
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

    pub fn is_fragment_of(&self, other: &Resource) -> bool {
        other.scheme == self.scheme && other.resource_path == self.resource_path
    }
}

/// Events concerning node operation triggered by the user
#[derive(Clone, Debug)]
pub enum UserNodeEvent {
    NewNode(Operator),
    RemoveNode(Resource),
    ConnectSockets(Resource, Resource),
    DisconnectSockets(Resource, Resource),
    ParameterChange(Resource, &'static str, Vec<u8>),
    ForceRecompute,
}

#[derive(Clone, Debug)]
pub enum GraphEvent {
    NodeAdded(Resource, Operator),
    NodeRemoved(Resource),
    ConnectedSockets(Resource, Resource),
    DisconnectedSockets(Resource, Resource),
    Recomputed(Vec<Instruction>),
}

#[derive(Clone, Debug)]
pub enum UserEvent {
    Quit,
}

#[derive(Clone, Debug)]
pub enum Lang {
    UserNodeEvent(UserNodeEvent),
    UserEvent(UserEvent),
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
