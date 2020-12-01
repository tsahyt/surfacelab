use super::resource::*;
use enum_dispatch::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[enum_dispatch]
pub trait Parameters {
    fn set_parameter(&mut self, field: &str, data: &[u8]);
}

pub type ParameterBool = u32;

pub trait ParameterField {
    fn from_data(data: &[u8]) -> Self;
    fn to_data(&self) -> Vec<u8>;
    fn data_length() -> usize;
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

    fn data_length() -> usize {
        4
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

    fn data_length() -> usize {
        4
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

    fn data_length() -> usize {
        4
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

    fn data_length() -> usize {
        4 * 3
    }
}

impl<T, Q> ParameterField for (T, Q)
where
    T: ParameterField,
    Q: ParameterField,
{
    fn to_data(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(self.0.to_data().iter());
        v.extend(self.1.to_data().iter());
        v
    }

    fn from_data(data: &[u8]) -> Self {
        debug_assert!(T::data_length() != 0);

        let t_data = &data[0..T::data_length()];
        let q_data = &data[T::data_length()..];
        (T::from_data(t_data), Q::from_data(q_data))
    }

    fn data_length() -> usize {
        T::data_length() + Q::data_length()
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

    fn data_length() -> usize {
        0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphParameter {
    pub graph_field: String,
    pub parameter: Resource<super::resource::Param>,
    pub title: String,
    pub control: Control,
}

impl GraphParameter {
    pub fn to_substitution(&self) -> ParamSubstitution {
        ParamSubstitution {
            resource: self.parameter.clone(),
            value: self.control.value(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct ParamSubstitution {
    resource: Resource<Param>,
    value: Vec<u8>,
}

impl ParamSubstitution {
    pub fn substitute<T: Parameters>(&self, on: &mut T) {
        on.set_parameter(&self.resource.fragment().unwrap(), &self.value);
    }

    pub fn resource(&self) -> &Resource<Param> {
        &self.resource
    }

    pub fn resource_mut(&mut self) -> &mut Resource<Param> {
        &mut self.resource
    }

    pub fn get_value(&self) -> &[u8] {
        &self.value
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
    LayerField(LayerField),
}

impl MessageWriter for MessageWriters {
    type Resource = Resource<Node>;

    fn transmit(&self, resource: &Self::Resource, data: &[u8]) -> super::Lang {
        match self {
            MessageWriters::Field(x) => x.transmit(resource, data),
            MessageWriters::ResourceField(x) => x.transmit(resource, data),
            MessageWriters::LayerField(x) => x.transmit(resource, data),
        }
    }

    fn as_field(&self) -> Option<&Field> {
        match self {
            MessageWriters::Field(x) => Some(x),
            MessageWriters::ResourceField(_) => None,
            MessageWriters::LayerField(_) => None,
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

impl From<LayerField> for MessageWriters {
    fn from(x: LayerField) -> Self {
        MessageWriters::LayerField(x)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field(pub String);

impl MessageWriter for Field {
    type Resource = Resource<Node>;

    fn transmit(&self, resource: &Resource<Node>, data: &[u8]) -> super::Lang {
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
    type Resource = Resource<Node>;

    fn transmit(&self, resource: &Resource<Node>, data: &[u8]) -> super::Lang {
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

#[derive(Clone, Debug, PartialEq)]
pub enum LayerField {
    ConnectOutput(super::MaterialChannel),
    ConnectInput(String),
}

impl MessageWriter for LayerField {
    type Resource = Resource<Node>;

    fn transmit(&self, resource: &Self::Resource, data: &[u8]) -> super::Lang {
        match self {
            Self::ConnectOutput(channel) => {
                let (n_enabled, n_selected) = <(u32, u32)>::from_data(data);
                let enabled = n_enabled == 1;
                let selected = n_selected as usize;

                super::Lang::UserLayersEvent(super::UserLayersEvent::SetOutput(
                    resource.clone(),
                    *channel,
                    selected,
                    enabled,
                ))
            }
            LayerField::ConnectInput(input) => {
                use strum::IntoEnumIterator;
                super::Lang::UserLayersEvent(super::UserLayersEvent::SetInput(
                    resource.node_socket(&input),
                    super::MaterialChannel::iter()
                        .nth(u32::from_data(data) as usize)
                        .unwrap(),
                ))
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GraphField {
    Name,
}

impl MessageWriter for GraphField {
    type Resource = Resource<Graph>;

    fn transmit(&self, resource: &Resource<Graph>, data: &[u8]) -> super::Lang {
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
    TextureScale,
    DisplacementAmount,
    LightType,
    LightStrength,
    FogStrength,
    Shadow,
    AO,
    EnvironmentStrength,
    HDRI,
}

impl MessageWriter for RenderField {
    type Resource = super::RendererID;

    fn transmit(&self, renderer: &super::RendererID, data: &[u8]) -> super::Lang {
        match self {
            RenderField::TextureScale => super::Lang::UserRenderEvent(
                super::UserRenderEvent::TextureScale(*renderer, f32::from_data(data)),
            ),
            RenderField::DisplacementAmount => super::Lang::UserRenderEvent(
                super::UserRenderEvent::DisplacementAmount(*renderer, f32::from_data(data)),
            ),
            RenderField::LightType => super::Lang::UserRenderEvent(
                super::UserRenderEvent::LightType(*renderer, super::LightType::from_data(data)),
            ),
            RenderField::LightStrength => super::Lang::UserRenderEvent(
                super::UserRenderEvent::LightStrength(*renderer, f32::from_data(data)),
            ),
            RenderField::FogStrength => super::Lang::UserRenderEvent(
                super::UserRenderEvent::FogStrength(*renderer, f32::from_data(data)),
            ),
            RenderField::EnvironmentStrength => super::Lang::UserRenderEvent(
                super::UserRenderEvent::EnvironmentStrength(*renderer, f32::from_data(data)),
            ),
            RenderField::Shadow => super::Lang::UserRenderEvent(super::UserRenderEvent::SetShadow(
                *renderer,
                ParameterBool::from_data(data),
            )),
            RenderField::AO => super::Lang::UserRenderEvent(super::UserRenderEvent::SetAO(
                *renderer,
                ParameterBool::from_data(data),
            )),
            RenderField::HDRI => super::Lang::UserRenderEvent(super::UserRenderEvent::LoadHDRI(
                *renderer,
                PathBuf::from_data(data),
            )),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SurfaceField {
    Resize,
}

impl MessageWriter for SurfaceField {
    type Resource = ();

    fn transmit(&self, _resource: &Self::Resource, data: &[u8]) -> super::Lang {
        super::Lang::UserIOEvent(super::UserIOEvent::SetParentSize(
            u32::from_data(data) * 1024,
        ))
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

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
                Control::ChannelMap { .. } => {
                    counts.enums += 1;
                    counts.toggles += 1;
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

    pub fn parameters_mut(&mut self) -> BoxParameters<'_, T> {
        BoxParameters::new(&mut self.categories)
    }
}

pub struct BoxParameters<'a, T>
where
    T: MessageWriter,
{
    cats: std::slice::IterMut<'a, ParamCategory<T>>,
    params: Option<std::slice::IterMut<'a, Parameter<T>>>,
}

impl<'a, T> BoxParameters<'a, T>
where
    T: MessageWriter,
{
    pub fn new(categories: &'a mut [ParamCategory<T>]) -> Self {
        Self {
            cats: categories.iter_mut(),
            params: None,
        }
    }
}

impl<'a, T> Iterator for BoxParameters<'a, T>
where
    T: MessageWriter,
{
    type Item = &'a mut Parameter<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.params.is_none() {
            self.params = self.cats.next().map(|x| x.parameters.iter_mut());
        }

        match &mut self.params {
            None => None,
            Some(ps) => match ps.next() {
                None => {
                    self.params = None;
                    self.next()
                }
                x => x,
            },
        }
    }
}

impl ParamBoxDescription<ResourceField> {
    pub fn node_parameters(res: &Resource<Node>, scalable: bool) -> Self {
        let mut parameters = vec![Parameter {
            name: "node-resource".to_string(),
            transmitter: ResourceField::Name,
            control: Control::Entry {
                value: res
                    .path()
                    .file_name()
                    .and_then(|x| x.to_str())
                    .map(|x| x.to_string())
                    .unwrap(),
            },
            expose_status: None,
        }];
        if scalable {
            parameters.push(Parameter {
                name: "node-size".to_string(),
                transmitter: ResourceField::Size,
                control: Control::DiscreteSlider {
                    value: 0,
                    min: -16,
                    max: 16,
                },
                expose_status: None,
            });
            parameters.push(Parameter {
                name: "node-abs-size".to_string(),
                transmitter: ResourceField::AbsoluteSize,
                control: Control::Toggle { def: false },
                expose_status: None,
            });
        }
        ParamBoxDescription {
            box_title: "node-attributes".to_string(),
            categories: vec![ParamCategory {
                name: "node",
                parameters,
            }],
        }
    }
}

impl ParamBoxDescription<RenderField> {
    pub fn render_parameters() -> Self {
        Self {
            box_title: "renderer".to_string(),
            categories: vec![
                ParamCategory {
                    name: "geometry",
                    parameters: vec![
                        Parameter {
                            name: "displacement-amount".to_string(),
                            control: Control::Slider {
                                value: 0.5,
                                min: 0.0,
                                max: 2.0,
                            },
                            transmitter: RenderField::DisplacementAmount,
                            expose_status: None,
                        },
                        Parameter {
                            name: "tex-scale".to_string(),
                            control: Control::Slider {
                                value: 8.0,
                                min: 0.0,
                                max: 64.0,
                            },
                            transmitter: RenderField::TextureScale,
                            expose_status: None,
                        },
                    ],
                },
                ParamCategory {
                    name: "environment",
                    parameters: vec![
                        Parameter {
                            name: "hdri-file".to_string(),
                            control: Control::File { selected: None },
                            transmitter: RenderField::HDRI,
                            expose_status: None,
                        },
                        Parameter {
                            name: "hdri-strength".to_string(),
                            control: Control::Slider {
                                value: 1.0,
                                min: 0.0,
                                max: 4.0,
                            },
                            transmitter: RenderField::EnvironmentStrength,
                            expose_status: None,
                        },
                        Parameter {
                            name: "ao".to_string(),
                            control: Control::Toggle { def: false },
                            transmitter: RenderField::AO,
                            expose_status: None,
                        },
                        Parameter {
                            name: "fog-strength".to_string(),
                            control: Control::Slider {
                                value: 0.2,
                                min: 0.0,
                                max: 1.0,
                            },
                            transmitter: RenderField::FogStrength,
                            expose_status: None,
                        },
                    ],
                },
                ParamCategory {
                    name: "light",
                    parameters: vec![
                        Parameter {
                            name: "light-type".to_string(),
                            control: Control::Enum {
                                selected: 0,
                                variants: vec!["Point Light".to_string(), "Sun Light".to_string()],
                            },
                            transmitter: RenderField::LightType,
                            expose_status: None,
                        },
                        Parameter {
                            name: "light-strength".to_string(),
                            control: Control::Slider {
                                value: 100.0,
                                min: 0.0,
                                max: 1000.0,
                            },
                            transmitter: RenderField::LightStrength,
                            expose_status: None,
                        },
                        Parameter {
                            name: "shadow".to_string(),
                            control: Control::Toggle { def: true },
                            transmitter: RenderField::Shadow,
                            expose_status: None,
                        },
                    ],
                },
            ],
        }
    }
}

impl ParamBoxDescription<GraphField> {
    pub fn graph_parameters(name: &str) -> Self {
        Self {
            box_title: "graph-tab".to_string(),
            categories: vec![ParamCategory {
                name: "graph-attributes",
                parameters: vec![Parameter {
                    name: "graph-name".to_string(),
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

impl ParamBoxDescription<LayerField> {
    pub fn fill_layer_parameters<T: super::Socketed>(
        operator: &T,
        output_sockets: &HashMap<super::MaterialChannel, String>,
    ) -> Self {
        use itertools::Itertools;
        use strum::IntoEnumIterator;

        Self {
            box_title: "layer".to_string(),
            categories: vec![ParamCategory {
                name: "output-channels",
                parameters: super::MaterialChannel::iter()
                    .map(|chan| Parameter {
                        name: chan.to_string(),
                        control: Control::ChannelMap {
                            enabled: output_sockets.contains_key(&chan),
                            selected: operator
                                .outputs()
                                .keys()
                                .sorted()
                                .position(|x| Some(x) == output_sockets.get(&chan))
                                .unwrap_or(0),
                            sockets: operator.outputs().keys().sorted().cloned().collect(),
                        },
                        transmitter: LayerField::ConnectOutput(chan),
                        expose_status: None,
                    })
                    .collect(),
            }],
        }
    }

    pub fn fx_layer_parameters<T: super::Socketed>(operator: &T) -> Self {
        use itertools::Itertools;
        use strum::IntoEnumIterator;

        Self {
            box_title: "layer".to_string(),
            categories: vec![
                ParamCategory {
                    name: "input-channels",
                    parameters: operator
                        .inputs()
                        .keys()
                        .sorted()
                        .map(|input| Parameter {
                            name: input.to_owned(),
                            control: Control::Enum {
                                selected: 0,
                                variants: super::MaterialChannel::iter()
                                    .map(|c| c.to_string())
                                    .collect(),
                            },
                            transmitter: LayerField::ConnectInput(input.to_owned()),
                            expose_status: None,
                        })
                        .collect(),
                },
                ParamCategory {
                    name: "output-channels",
                    parameters: super::MaterialChannel::iter()
                        .map(|chan| Parameter {
                            name: chan.to_string(),
                            control: Control::ChannelMap {
                                enabled: false,
                                selected: 0,
                                sockets: operator.outputs().keys().sorted().cloned().collect(),
                            },
                            transmitter: LayerField::ConnectOutput(chan),
                            expose_status: None,
                        })
                        .collect(),
                },
            ],
        }
    }
}

impl ParamBoxDescription<SurfaceField> {
    pub fn surface_parameters() -> Self {
        Self {
            box_title: "surface-tab".to_string(),
            categories: vec![ParamCategory {
                name: "surface-attributes",
                parameters: vec![Parameter {
                    name: "parent-size".to_string(),
                    control: Control::DiscreteSlider {
                        value: 1,
                        min: 1,
                        max: 4,
                    },
                    transmitter: SurfaceField::Resize,
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
    ChannelMap {
        enabled: bool,
        selected: usize,
        sockets: Vec<String>,
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
            Self::ChannelMap {
                enabled, selected, ..
            } => (
                (if *enabled { 1 as u32 } else { 0 as u32 }),
                (*selected as u32),
            )
                .to_data(),
        }
    }

    pub fn set_value(&mut self, data: &[u8]) {
        match self {
            Control::Slider { value, .. } => *value = f32::from_data(data),
            Control::DiscreteSlider { value, .. } => *value = i32::from_data(data),
            Control::RgbColor { value } => *value = <[f32; 3]>::from_data(data),
            Control::Enum { selected, .. } => *selected = u32::from_data(data) as usize,
            Control::File { selected } => *selected = Some(PathBuf::from_data(data)),
            Control::Ramp { steps } => {
                *steps = data
                    .chunks(std::mem::size_of::<[f32; 4]>())
                    .map(|chunk| {
                        let fields: Vec<f32> = chunk
                            .chunks(4)
                            .map(|z| {
                                let mut arr: [u8; 4] = Default::default();
                                arr.copy_from_slice(z);
                                f32::from_be_bytes(arr)
                            })
                            .collect();
                        [fields[0], fields[1], fields[2], fields[3]]
                    })
                    .collect()
            }
            Control::Toggle { def } => *def = u32::from_data(data) == 1,
            Control::Entry { value } => {
                *value = unsafe { std::str::from_utf8_unchecked(data) }.to_owned()
            }
            Control::ChannelMap {
                enabled, selected, ..
            } => {
                let (n_enabled, n_selected) = <(u32, u32)>::from_data(data);
                *enabled = n_enabled == 1;
                *selected = n_selected as usize;
            }
        }
    }
}

#[enum_dispatch]
pub trait OperatorParamBox {
    fn param_box_description(&self) -> ParamBoxDescription<Field>;
}
