use super::Resource;
use enum_dispatch::*;
use serde_derive::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[enum_dispatch]
pub trait Parameters {
    fn set_parameter(&mut self, field: &str, data: &[u8]);
}

pub type ParameterBool = u32;

pub trait ParameterField {
    fn from_data(data: &[u8]) -> Self;
    fn to_data(&self) -> Vec<u8>;
}

impl ParameterField for f32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        f32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for u32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        u32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for i32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        i32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for [f32; 3] {
    fn from_data(data: &[u8]) -> Self {
        let cols: Vec<f32> = data
            .chunks(4)
            .map(|z| {
                let mut arr: [u8; 4] = Default::default();
                arr.copy_from_slice(z);
                f32::from_be_bytes(arr)
            })
            .collect();
        [cols[0], cols[1], cols[2]]
    }

    fn to_data(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self[0] as f32).to_be_bytes());
        buf.extend_from_slice(&(self[1] as f32).to_be_bytes());
        buf.extend_from_slice(&(self[2] as f32).to_be_bytes());
        buf.extend_from_slice(&(1.0 as f32).to_be_bytes());
        buf
    }
}

impl ParameterField for PathBuf {
    fn from_data(data: &[u8]) -> Self {
        let path_str = unsafe { std::str::from_utf8_unchecked(&data) };
        Path::new(path_str).to_path_buf()
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_str().unwrap().as_bytes().to_vec()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphParameter {
    pub graph_field: String,
    pub parameter: Resource,
    pub title: String,
    pub control: Control,
}

impl GraphParameter {
    pub fn to_substitution(&self) -> ParamSubstitution {
        ParamSubstitution {
            resource: Resource::node(self.parameter.path(), None),
            field: self.parameter.fragment().unwrap().to_owned(),
            value: self.control.default_value(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParamSubstitution {
    resource: super::Resource,
    field: String,
    value: Vec<u8>,
}

impl ParamSubstitution {
    pub fn substitute<T: Parameters>(&self, on: &mut T) {
        on.set_parameter(&self.field, &self.value);
    }

    pub fn resource(&self) -> &super::Resource {
        &self.resource
    }

    pub fn set_value(&mut self, value: &[u8]) {
        self.value = value.to_vec();
    }
}

pub trait MessageWriter {
    fn transmit(&self, resource: Resource, data: &[u8]) -> super::Lang;

    fn as_field(&self) -> Option<&Field>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field(pub String);

impl MessageWriter for Field {
    fn transmit(&self, resource: Resource, data: &[u8]) -> super::Lang {
        super::Lang::UserNodeEvent(super::UserNodeEvent::ParameterChange(
            Resource::parameter(resource.path(), &self.0),
            data.to_vec(),
        ))
    }

    fn as_field(&self) -> Option<&Field> {
        Some(self)
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ResourceField {
    Name,
    Size,
    AbsoluteSize,
}

impl MessageWriter for ResourceField {
    fn transmit(&self, resource: Resource, data: &[u8]) -> super::Lang {
        match self {
            Self::Name => {
                let new = unsafe { std::str::from_utf8_unchecked(&data) };
                let mut res_new = resource.clone();
                res_new.modify_path(|p| {
                    p.pop();
                    p.push(new);
                });
                super::Lang::UserNodeEvent(super::UserNodeEvent::RenameNode(
                    resource.clone(),
                    res_new,
                ))
            }
            Self::Size => super::Lang::UserNodeEvent(super::UserNodeEvent::OutputSizeChange(
                resource,
                i32::from_data(data),
            )),
            Self::AbsoluteSize => super::Lang::UserNodeEvent(
                super::UserNodeEvent::OutputSizeAbsolute(resource, data != [0]),
            ),
        }
    }

    fn as_field(&self) -> Option<&Field> {
        None
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ParamBoxDescription<T: MessageWriter> {
    pub box_title: String,
    pub categories: Vec<ParamCategory<T>>,
}

impl<T> ParamBoxDescription<T>
where
    T: MessageWriter,
{
    pub fn empty() -> Self {
        ParamBoxDescription {
            box_title: "".to_string(),
            categories: vec![],
        }
    }

    /// Return the number of total parameters
    pub fn len(&self) -> usize {
        self.categories.iter().map(|c| c.parameters.len()).sum()
    }

    /// Return the number of categories
    pub fn categories(&self) -> usize {
        self.categories.len()
    }

    /// Return the number of controls, by control type
    pub fn control_counts(&self) -> ControlCounts {
        let mut counts = ControlCounts::default();

        for parameter in self.categories.iter().map(|c| c.parameters.iter()).flatten() {
            match parameter.control {
                Control::Slider { .. } => {
                    counts.sliders += 1;
                }
                Control::DiscreteSlider { .. } => {
                    counts.discrete_sliders += 1;
                }
                Control::RgbColor { .. } => {
                    counts.rgb_colors += 1;
                }
                Control::RgbaColor { .. } => {
                    counts.rgba_colors += 1;
                }
                Control::Enum { .. } => {
                    counts.enums += 1;
                }
                Control::File { .. } => {
                    counts.files += 1;
                }
                Control::Ramp { .. } => {
                    counts.ramps += 1;
                }
                Control::Toggle { .. } => {
                    counts.toggles += 1;
                }
                Control::Entry { .. } => {
                    counts.entries += 1;
                }
            }
        }

        counts
    }
}

#[derive(Default, Copy, Clone)]
pub struct ControlCounts {
    pub sliders: usize,
    pub discrete_sliders: usize,
    pub rgb_colors: usize,
    pub rgba_colors: usize,
    pub enums: usize,
    pub files: usize,
    pub ramps: usize,
    pub toggles: usize,
    pub entries: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub struct ParamCategory<T: MessageWriter> {
    pub name: &'static str,
    pub parameters: Vec<Parameter<T>>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Parameter<T: MessageWriter> {
    pub name: String,
    pub transmitter: T,
    pub control: Control,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Control {
    Slider {
        value: f32,
        min: f32,
        max: f32,
    },
    DiscreteSlider {
        value: i32,
        min: i32,
        max: i32,
    },
    RgbColor {
        value: [f32; 3],
    },
    RgbaColor {
        value: [f32; 4],
    },
    Enum {
        selected: usize,
        variants: Vec<String>,
    },
    File {
        selected: Option<std::path::PathBuf>,
    },
    Ramp {
        steps: Vec<[f32; 4]>,
    },
    Toggle {
        def: bool,
    },
    Entry {
        value: String,
    },
}

impl Control {
    fn default_value(&self) -> Vec<u8> {
        match self {
            Self::Slider { value, .. } => value.to_data(),
            Self::DiscreteSlider { value, .. } => value.to_data(),
            Self::RgbColor { value, .. } => value.to_data(),
            _ => unimplemented!(), // TODO: default values for other control types
        }
    }
}

#[enum_dispatch]
pub trait OperatorParamBox {
    fn param_box_description(&self) -> ParamBoxDescription<Field>;
}
