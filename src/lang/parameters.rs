use enum_dispatch::*;
use serde_derive::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::cell::RefCell;

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
pub enum GraphParameterType {
    Real,
    Discrete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphParameter {
    pub graph_field: String,
    child_field: String,
    title: String,
    resource: super::Resource,
    ty: GraphParameterType,
    default_value: Vec<u8>,
}

impl GraphParameter {
    pub fn to_substitution(&self) -> ParamSubstitution {
        ParamSubstitution {
            resource: self.resource.clone(),
            field: self.child_field.clone(),
            value: self.default_value.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSubstitution {
    pub resource: super::Resource,
    pub field: String,
    pub value: Vec<u8>,
}

impl ParamSubstitution {
    pub fn substitute<T: Parameters>(&self, on: &mut T) {
        on.set_parameter(&self.field, &self.value);
    }
}

pub trait MessageWriter {
    fn transmit(&self, resource: super::Resource, data: &[u8]) -> super::Lang;
}

#[derive(Copy, Clone, Debug)]
pub struct Field(pub &'static str);

impl MessageWriter for Field {
    fn transmit(&self, resource: super::Resource, data: &[u8]) -> super::Lang {
        super::Lang::UserNodeEvent(super::UserNodeEvent::ParameterChange(
            resource,
            self.0,
            data.to_vec(),
        ))
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ResourceField {
    Name,
    Size,
    AbsoluteSize,
}

impl MessageWriter for ResourceField {
    fn transmit(&self, resource: super::Resource, data: &[u8]) -> super::Lang {
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
            Self::Size => {
                super::Lang::UserNodeEvent(super::UserNodeEvent::OutputSizeChange(
                    resource,
                    i32::from_data(data),
                ))
            }
            Self::AbsoluteSize => super::Lang::UserNodeEvent(
                super::UserNodeEvent::OutputSizeAbsolute(resource, data != [0]),
            ),
        }
    }
}

pub struct ParamBoxDescription<'a, T: MessageWriter> {
    pub box_title: &'a str,
    pub resource: Rc<RefCell<super::Resource>>,
    pub categories: &'a [ParamCategory<'a, T>],
}

pub struct ParamCategory<'a, T: MessageWriter> {
    pub name: &'static str,
    pub parameters: &'a [Parameter<'a, T>],
}

pub struct Parameter<'a, T: MessageWriter> {
    pub name: &'static str,
    pub transmitter: T,
    pub control: Control<'a>,
    pub available: bool,
}

pub enum Control<'a> {
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
        variants: &'static [&'static str],
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
        value: &'a str,
    },
}
