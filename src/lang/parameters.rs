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
            value: self.control.value(),
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

#[enum_dispatch]
pub trait MessageWriter: Clone {
    type Resource;

    fn transmit(&self, resource: &Self::Resource, data: &[u8]) -> super::Lang;

    fn as_field(&self) -> Option<&Field> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MessageWriters {
    Field(Field),
    ResourceField(ResourceField),
}

impl MessageWriter for MessageWriters {
    type Resource = Resource;

    fn transmit(&self, resource: &Self::Resource, data: &[u8]) -> super::Lang {
        match self {
            MessageWriters::Field(x) => x.transmit(resource, data),
            MessageWriters::ResourceField(x) => x.transmit(resource, data),
        }
    }

    fn as_field(&self) -> Option<&Field> {
        match self {
            MessageWriters::Field(x) => Some(x),
            MessageWriters::ResourceField(_) => None,
        }
    }
}

impl From<Field> for MessageWriters {
    fn from(x: Field) -> Self {
        MessageWriters::Field(x)
    }
}

impl From<ResourceField> for MessageWriters {
    fn from(x: ResourceField) -> Self {
        MessageWriters::ResourceField(x)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field(pub String);

impl MessageWriter for Field {
    type Resource = Resource;

    fn transmit(&self, resource: &Resource, data: &[u8]) -> super::Lang {
        super::Lang::UserNodeEvent(super::UserNodeEvent::ParameterChange(
            Resource::parameter(resource.path(), &self.0),
            data.to_vec(),
        ))
    }

    fn as_field(&self) -> Option<&Field> {
        Some(self)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ResourceField {
    Name,
    Size,
    AbsoluteSize,
}

impl MessageWriter for ResourceField {
    type Resource = Resource;

    fn transmit(&self, resource: &Resource, data: &[u8]) -> super::Lang {
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
                resource.clone(),
                i32::from_data(data),
            )),
            Self::AbsoluteSize => super::Lang::UserNodeEvent(
                super::UserNodeEvent::OutputSizeAbsolute(resource.clone(), data != [0]),
            ),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GraphField {
    Name,
}

impl MessageWriter for GraphField {
    type Resource = Resource;

    fn transmit(&self, resource: &Resource, data: &[u8]) -> super::Lang {
        let new = unsafe { std::str::from_utf8_unchecked(&data) };
        let mut res_new = resource.clone();
        res_new.modify_path(|p| {
            p.pop();
            p.push(new);
        });
        super::Lang::UserGraphEvent(super::UserGraphEvent::RenameGraph(
            resource.clone(),
            res_new,
        ))
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RenderField {
    DisplacementAmount,
    LightType,
}

impl MessageWriter for RenderField {
    type Resource = super::RendererID;

    fn transmit(&self, renderer: &super::RendererID, data: &[u8]) -> super::Lang {
        match self {
            RenderField::DisplacementAmount => super::Lang::UserRenderEvent(
                super::UserRenderEvent::DisplacementAmount(*renderer, f32::from_data(data)),
            ),
            RenderField::LightType => super::Lang::UserRenderEvent(
                super::UserRenderEvent::LightType(*renderer, super::LightType::from_data(data)),
            ),
        }
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

        for parameter in self
            .categories
            .iter()
            .map(|c| c.parameters.iter())
            .flatten()
        {
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

    pub fn map_transmitters<Q: MessageWriter, F: Fn(&T) -> Q>(
        self,
        f: F,
    ) -> ParamBoxDescription<Q> {
        ParamBoxDescription {
            box_title: self.box_title,
            categories: self
                .categories
                .iter()
                .map(|cat| ParamCategory {
                    name: cat.name,
                    parameters: cat
                        .parameters
                        .iter()
                        .map(|param| Parameter {
                            name: param.name.to_owned(),
                            expose_status: param.expose_status,
                            control: param.control.to_owned(),
                            transmitter: f(&param.transmitter),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    pub fn extend_categories<I>(&mut self, cats: I)
    where
        I: IntoIterator<Item = ParamCategory<T>>,
    {
        self.categories.extend(cats);
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.extend_categories(other.categories.iter().cloned());
        self
    }
}

impl ParamBoxDescription<RenderField> {
    pub fn render_parameters() -> Self {
        Self {
            box_title: "Renderer".to_string(),
            categories: vec![
                ParamCategory {
                    name: "Geometry",
                    parameters: vec![Parameter {
                        name: "Displacement Amount".to_string(),
                        control: Control::Slider {
                            value: 1.0,
                            min: 0.0,
                            max: 3.0,
                        },
                        transmitter: RenderField::DisplacementAmount,
                        expose_status: None,
                    }],
                },
                ParamCategory {
                    name: "Lighting",
                    parameters: vec![Parameter {
                        name: "Light Type".to_string(),
                        control: Control::Enum {
                            selected: 0,
                            variants: vec!["Point Light".to_string(), "Sun Light".to_string()],
                        },
                        transmitter: RenderField::LightType,
                        expose_status: None,
                    }],
                },
            ],
        }
    }
}

impl ParamBoxDescription<GraphField> {
    pub fn graph_parameters(name: &str) -> Self {
        Self {
            box_title: "Graph".to_string(),
            categories: vec![ParamCategory {
                name: "Graph Attributes",
                parameters: vec![Parameter {
                    name: "Graph Name".to_string(),
                    control: Control::Entry {
                        value: name.to_owned(),
                    },
                    transmitter: GraphField::Name,
                    expose_status: None,
                }],
            }],
        }
    }
}

#[derive(Default, Copy, Clone, Debug)]
pub struct ControlCounts {
    pub sliders: usize,
    pub discrete_sliders: usize,
    pub rgb_colors: usize,
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

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ExposeStatus {
    Unexposed,
    Exposed,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Parameter<T: MessageWriter> {
    pub name: String,
    pub transmitter: T,
    pub control: Control,
    pub expose_status: Option<ExposeStatus>,
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
    fn value(&self) -> Vec<u8> {
        match self {
            Self::Slider { value, .. } => value.to_data(),
            Self::DiscreteSlider { value, .. } => value.to_data(),
            Self::RgbColor { value, .. } => value.to_data(),
            Self::Enum { selected, .. } => (*selected as u32).to_data(),
            Self::File { selected } => selected.clone().unwrap().to_data(),
            Self::Ramp { steps } => {
                let mut buf = Vec::new();
                for step in steps.iter() {
                    buf.extend_from_slice(&step[0].to_be_bytes());
                    buf.extend_from_slice(&step[1].to_be_bytes());
                    buf.extend_from_slice(&step[2].to_be_bytes());
                    buf.extend_from_slice(&step[3].to_be_bytes());
                }
                buf
            }
            Self::Toggle { def } => (if *def { 1 as u32 } else { 0 as u32 }).to_data(),
            Self::Entry { value } => value.as_bytes().to_vec(),
        }
    }
}

#[enum_dispatch]
pub trait OperatorParamBox {
    fn param_box_description(&self) -> ParamBoxDescription<Field>;
}
