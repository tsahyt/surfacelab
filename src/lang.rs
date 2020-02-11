use maplit::hashmap;
use std::collections::HashMap;
pub use uriparse::uri::URI;

#[repr(C)]
#[derive(Clone, Debug)]
pub struct BlendParameters {
    mix: f32,
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

    pub fn default_name(&self) -> String {
        match self {
            Operator::Blend(..) => "blend".to_string(),
            Operator::PerlinNoise(..) => "perlin_noise".to_string(),
            Operator::Image {..} => "image".to_string(),
            Operator::Output {..} => "output".to_string(),
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

/// Events concerning node operation triggered by the user
#[derive(Clone, Debug)]
pub enum UserNodeEvent {
    NewNode(Operator),
    RemoveNode(URI<'static>),
    ConnectSockets(URI<'static>, URI<'static>),
    DisconnectSockets(URI<'static>, URI<'static>),
}

#[derive(Clone, Debug)]
pub enum Lang {
    UserNodeEvent(UserNodeEvent)
}
