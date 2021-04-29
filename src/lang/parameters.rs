use super::{resource::*, ImageType, OperatorSize, OperatorType, ShadingMode};
use enum_dispatch::*;
use serde_derive::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{collections::HashMap, fmt::Debug, sync::Arc};
use strum::VariantNames;
use thiserror::Error;

/// A trait for things that have parameters. Parameters can be set from a field
/// descriptor and some plain data. It is up to the implementation to interpret
/// this data.
///
/// The reason for the weak typing here is that parameters are sent over the
/// application bus and this way we can unify the message types and don't care
/// about the types on the frontend as much.
#[enum_dispatch]
pub trait Parameters {
    fn set_parameter(&mut self, field: &str, data: &[u8]);
}

/// A ParameterBool is just a 4 byte integer representing a bool for use in a
/// shader.
pub type ParameterBool = u32;

/// A ParameterField is a type that can be converted from/to data, with a given
/// fixed size. Specifically, anything that can be serialized and deserialized
/// can be used as a parameter field using bincode.
pub trait ParameterField<'a> {
    fn from_data(data: &'a [u8]) -> Self;
    fn to_data(&self) -> Vec<u8>;
}

impl<'a, T> ParameterField<'a> for T
where
    T: serde::Serialize + serde::Deserialize<'a>,
{
    fn from_data(data: &'a [u8]) -> Self {
        bincode::deserialize(data).unwrap()
    }

    fn to_data(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }
}

/// A GraphParameter is some exposed parameter of a node graph/layer stack. Each
/// such parameter references some (atomic) parameter inside the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphParameter {
    /// The field name of the parameter, used to address it in `set_parameter`.
    pub graph_field: String,

    /// The *internal* parameter referenced by it
    pub parameter: Resource<super::resource::Param>,

    /// Human readable name
    pub title: String,

    /// Control to be used
    pub control: Control,
}

impl GraphParameter {
    /// Convert a parameter with its current settings to a substitution for use
    /// in computation.
    pub fn to_substitution(&self) -> ParamSubstitution {
        ParamSubstitution {
            resource: self.parameter.clone(),
            value: self.control.value(),
        }
    }
}

/// Parameter substitutions are used during computation to alter the values used
/// in nodes in a graph based on values set in the complex operator using this
/// graph at this instance. This allows fully reusing the linearization and just
/// substituting in values for the uniforms as required.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct ParamSubstitution {
    resource: Resource<Param>,
    value: Vec<u8>,
}

impl ParamSubstitution {
    /// Perform the substitution
    pub fn substitute<T: Parameters>(&self, on: &mut T) {
        on.set_parameter(&self.resource.fragment().unwrap(), &self.value);
    }

    /// Get the parameter resource being changed by this substitution
    pub fn resource(&self) -> &Resource<Param> {
        &self.resource
    }

    /// Get the parameter resource being changed by this substitution, mutably
    pub fn resource_mut(&mut self) -> &mut Resource<Param> {
        &mut self.resource
    }

    /// Obtain the value being set by this substitution, as a slice
    pub fn get_value(&self) -> &[u8] {
        &self.value
    }

    /// Set the value being set by this substitution, from a slice
    pub fn set_value(&mut self, value: &[u8]) {
        self.value = value.to_vec();
    }
}

/// A trait to enable creation of "on change" messages for some kind of
/// resource. Note that this resource is not necessarily what we call a resource
/// elsewhere in the program. It's just an additional piece of information for
/// the message creation process, which should describe the "recipient" of the
/// message.
#[enum_dispatch]
pub trait MessageWriter: Clone {
    type Resource;

    /// Create a message given a resource and associated data
    fn transmit(&self, resource: &Self::Resource, data: &[u8]) -> super::Lang;

    fn as_field(&self) -> Option<&Field> {
        None
    }
}

/// Various combined MessageWriters, for use in mixed parameter boxes
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

/// A Field is a MessageWriter for operator parameters.
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

/// A ResourceField is a MessageWriter for metadata of nodes and layers
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ResourceField {
    Name,
    Size,
}

impl MessageWriter for ResourceField {
    type Resource = Resource<Node>;

    fn transmit(&self, resource: &Resource<Node>, data: &[u8]) -> super::Lang {
        match self {
            Self::Name => {
                let mut res_new = resource.clone();
                res_new.rename_file(&String::from_data(data));
                super::Lang::UserNodeEvent(super::UserNodeEvent::RenameNode(
                    resource.clone(),
                    res_new,
                ))
            }
            Self::Size => super::Lang::UserNodeEvent(super::UserNodeEvent::OutputSizeChange(
                resource.clone(),
                OperatorSize::from_data(data),
            )),
        }
    }
}

/// A LayerField is a MessageWriter for interacting with layers.
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

/// A GraphField is a MessageWriter for metadata of graphs.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum GraphField {
    Name,
}

impl MessageWriter for GraphField {
    type Resource = Resource<Graph>;

    fn transmit(&self, resource: &Resource<Graph>, data: &[u8]) -> super::Lang {
        let mut res_new = resource.clone();
        res_new.rename_file(&String::from_data(data));
        super::Lang::UserGraphEvent(super::UserGraphEvent::RenameGraph(
            resource.clone(),
            res_new,
        ))
    }
}

/// A MessageWriter for setting renderer parameters
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RenderField {
    TextureScale,
    DisplacementAmount,
    LightType,
    LightStrength,
    LightSize,
    FogStrength,
    Shadow,
    AoStrength,
    EnvironmentStrength,
    EnvironmentBlur,
    EnvironmentRotation,
    Hdri,
    Matcap,
    FocalLength,
    ApertureSize,
    ApertureBlades,
    ApertureRotation,
    FocalDistance,
    ObjectType,
    ShadingMode,
    ToneMap,
    SampleCount,
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
            RenderField::LightSize => super::Lang::UserRenderEvent(
                super::UserRenderEvent::LightSize(*renderer, f32::from_data(data)),
            ),
            RenderField::FogStrength => super::Lang::UserRenderEvent(
                super::UserRenderEvent::FogStrength(*renderer, f32::from_data(data)),
            ),
            RenderField::EnvironmentStrength => super::Lang::UserRenderEvent(
                super::UserRenderEvent::EnvironmentStrength(*renderer, f32::from_data(data)),
            ),
            RenderField::EnvironmentBlur => super::Lang::UserRenderEvent(
                super::UserRenderEvent::EnvironmentBlur(*renderer, f32::from_data(data)),
            ),
            RenderField::EnvironmentRotation => super::Lang::UserRenderEvent(
                super::UserRenderEvent::EnvironmentRotation(*renderer, f32::from_data(data)),
            ),
            RenderField::Shadow => super::Lang::UserRenderEvent(super::UserRenderEvent::SetShadow(
                *renderer,
                ParameterBool::from_data(data),
            )),
            RenderField::AoStrength => super::Lang::UserRenderEvent(
                super::UserRenderEvent::AoStrength(*renderer, f32::from_data(data)),
            ),
            RenderField::Hdri => super::Lang::UserRenderEvent(super::UserRenderEvent::LoadHdri(
                *renderer,
                <Option<PathBuf>>::from_data(data),
            )),
            RenderField::Matcap => super::Lang::UserRenderEvent(
                super::UserRenderEvent::LoadMatcap(*renderer, <Option<PathBuf>>::from_data(data)),
            ),
            RenderField::FocalLength => super::Lang::UserRenderEvent(
                super::UserRenderEvent::FocalLength(*renderer, f32::from_data(data)),
            ),
            RenderField::ApertureSize => super::Lang::UserRenderEvent(
                super::UserRenderEvent::ApertureSize(*renderer, f32::from_data(data)),
            ),
            RenderField::ApertureBlades => super::Lang::UserRenderEvent(
                super::UserRenderEvent::ApertureBlades(*renderer, i32::from_data(data)),
            ),
            RenderField::ApertureRotation => super::Lang::UserRenderEvent(
                super::UserRenderEvent::ApertureRotation(*renderer, f32::from_data(data)),
            ),
            RenderField::FocalDistance => super::Lang::UserRenderEvent(
                super::UserRenderEvent::FocalDistance(*renderer, f32::from_data(data)),
            ),
            RenderField::ObjectType => super::Lang::UserRenderEvent(
                super::UserRenderEvent::ObjectType(*renderer, super::ObjectType::from_data(data)),
            ),
            RenderField::ShadingMode => super::Lang::UserRenderEvent(
                super::UserRenderEvent::ShadingMode(*renderer, super::ShadingMode::from_data(data)),
            ),
            RenderField::ToneMap => super::Lang::UserRenderEvent(super::UserRenderEvent::ToneMap(
                *renderer,
                super::ToneMap::from_data(data),
            )),
            RenderField::SampleCount => super::Lang::UserRenderEvent(
                super::UserRenderEvent::SampleCount(*renderer, u32::from_data(data)),
            ),
        }
    }
}

/// A MessageWriter for setting surface properties
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SurfaceField {
    Resize,
}

impl MessageWriter for SurfaceField {
    type Resource = ();

    fn transmit(&self, _resource: &Self::Resource, data: &[u8]) -> super::Lang {
        super::Lang::UserIOEvent(super::UserIOEvent::SetParentSize(
            OperatorSize::from_data(data).absolute(1024),
        ))
    }
}

#[derive(Serialize, Deserialize)]
pub struct ParameterPreset(HashMap<String, Vec<u8>>);

#[derive(Debug, Error)]
pub enum ParameterPresetError {
    #[error("Error during file IO")]
    FileIO(#[from] std::io::Error),
    #[error("Error during file serialization")]
    Serialization(#[from] serde_cbor::Error),
}

impl ParameterPreset {
    pub fn write_to_file<P: AsRef<Path> + std::fmt::Debug>(
        &self,
        path: P,
    ) -> Result<(), ParameterPresetError> {
        use std::fs::File;

        log::info!("Saving preset to {:?}", path);

        let output_file = File::create(path)?;
        serde_cbor::to_writer(output_file, &self)?;

        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path> + std::fmt::Debug>(
        path: P,
    ) -> Result<Self, ParameterPresetError> {
        use std::fs::File;

        log::info!("Loading preset from {:?}", path);

        let input_file = File::open(path)?;
        let preset: Self = serde_cbor::from_reader(input_file)?;

        Ok(preset)
    }
}

/// A ParamBoxDescription describes a parameter box with its categories and
/// parameters. It is a structure that can then get interpreted by the frontend
/// in order to present the parameters to the user.
#[derive(Debug, PartialEq, Clone)]
pub struct ParamBoxDescription<T: MessageWriter> {
    pub box_title: String,
    pub categories: Vec<ParamCategory<T>>,
}

impl<T> ParamBoxDescription<T>
where
    T: MessageWriter,
{
    /// The empty parameter box
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

    pub fn set_expose_status(
        &mut self,
        parameter_name: &str,
        expose_status: Option<ExposeStatus>,
    ) -> Option<()> {
        self.parameters_mut()
            .find(|p| p.name == parameter_name)?
            .expose_status = expose_status;
        Some(())
    }

    /// Return an association vector of parameters with their controls.
    pub fn controls(&self) -> Vec<(String, Control)> {
        self.categories
            .iter()
            .flat_map(|cat| {
                cat.parameters
                    .iter()
                    .map(|param| (param.name.clone(), param.control.clone()))
            })
            .collect()
    }

    /// Turn the current state of this parameter box description into a
    /// parameter preset.
    pub fn to_preset(&self) -> ParameterPreset {
        ParameterPreset(
            self.categories
                .iter()
                .flat_map(|cat| {
                    cat.parameters.iter().filter_map(|param| {
                        if param.presetable {
                            Some((param.name.clone(), param.control.value()))
                        } else {
                            None
                        }
                    })
                })
                .collect(),
        )
    }

    /// Load data from a preset into this parameter box, mutating the relevant
    /// controls and creating events according to the stored transmitters.
    pub fn load_preset(
        &mut self,
        resource: &T::Resource,
        mut preset: ParameterPreset,
    ) -> Vec<super::Lang> {
        let mut events = Vec::new();

        for param in self.parameters_mut() {
            if let Some(data) = preset.0.remove(&param.name) {
                param.control.set_value(&data);
                events.push(param.transmitter.transmit(resource, &data));
            }
        }

        events
    }

    /// Map a function over each transmitter in the box. Essentially making the
    /// description a functor.
    pub fn transmitters_into<Q: MessageWriter + From<T>>(mut self) -> ParamBoxDescription<Q> {
        ParamBoxDescription {
            box_title: self.box_title,
            categories: self
                .categories
                .drain(0..)
                .map(|mut cat| ParamCategory {
                    name: cat.name,
                    is_open: cat.is_open,
                    visibility: cat.visibility,
                    parameters: cat
                        .parameters
                        .drain(0..)
                        .map(|param| Parameter {
                            name: param.name,
                            expose_status: param.expose_status,
                            control: param.control,
                            transmitter: param.transmitter.into(),
                            visibility: param.visibility,
                            presetable: param.presetable,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Extend categories from an iterator.
    pub fn extend_categories<I>(&mut self, cats: I)
    where
        I: IntoIterator<Item = ParamCategory<T>>,
    {
        self.categories.extend(cats);
    }

    /// Merge two parameter boxes.
    pub fn merge(mut self, other: Self) -> Self {
        self.extend_categories(other.categories.iter().cloned());
        self
    }

    /// Obtain a mutable iterator over all parameters.
    pub fn parameters_mut(&mut self) -> BoxParameters<'_, T> {
        BoxParameters::new(&mut self.categories)
    }
}

/// Iterator over the parameters in a ParamBoxDescription
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
    /// Construct a parameter box for node metadata. The node is considered
    /// scalable if and only if a size is supplied.
    pub fn node_parameters(res: &Resource<Node>, size: Option<OperatorSize>) -> Self {
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
            visibility: VisibilityFunction::default(),
            presetable: false,
        }];
        if let Some(op_size) = size {
            parameters.push(Parameter {
                name: "node-size".to_string(),
                transmitter: ResourceField::Size,
                control: Control::Size {
                    size: op_size,
                    allow_relative: true,
                },
                expose_status: None,
                visibility: VisibilityFunction::default(),
                presetable: false,
            })
        }
        ParamBoxDescription {
            box_title: "node-attributes".to_string(),
            categories: vec![ParamCategory {
                name: "node",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters,
            }],
        }
    }
}

impl ParamBoxDescription<RenderField> {
    /// Construct a parameter box for render parameters.
    pub fn render_parameters() -> Self {
        Self {
            box_title: "renderer".to_string(),
            categories: vec![
                ParamCategory {
                    name: "renderer",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![
                        Parameter {
                            name: "sample-count".to_string(),
                            control: Control::DiscreteSlider {
                                value: 24,
                                min: 0,
                                max: 256,
                            },
                            transmitter: RenderField::SampleCount,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "shading-mode".to_string(),
                            control: Control::Enum {
                                selected: 0,
                                variants: super::ShadingMode::VARIANTS
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect(),
                            },
                            transmitter: RenderField::ShadingMode,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "tone-map".to_string(),
                            control: Control::Enum {
                                selected: 0,
                                variants: super::ToneMap::VARIANTS
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect(),
                            },
                            transmitter: RenderField::ToneMap,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
                ParamCategory {
                    name: "geometry",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![
                        Parameter {
                            name: "object-type".to_string(),
                            control: Control::Enum {
                                selected: 2,
                                variants: super::ObjectType::VARIANTS
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect(),
                            },
                            transmitter: RenderField::ObjectType,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "displacement-amount".to_string(),
                            control: Control::Slider {
                                value: 0.1,
                                min: 0.0,
                                max: 1.0,
                            },
                            transmitter: RenderField::DisplacementAmount,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "tex-scale".to_string(),
                            control: Control::Slider {
                                value: 1.0,
                                min: 0.0,
                                max: 4.0,
                            },
                            transmitter: RenderField::TextureScale,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
                ParamCategory {
                    name: "environment",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![
                        Parameter {
                            name: "hdri-file".to_string(),
                            control: Control::File { selected: None },
                            transmitter: RenderField::Hdri,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
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
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "hdri-blur".to_string(),
                            control: Control::Slider {
                                value: 3.0,
                                min: 0.0,
                                max: 6.0,
                            },
                            transmitter: RenderField::EnvironmentBlur,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "hdri-rotation".to_string(),
                            control: Control::Slider {
                                value: 0.0,
                                min: 0.0,
                                max: std::f32::consts::TAU,
                            },
                            transmitter: RenderField::EnvironmentRotation,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "ambient-occlusion-strength".to_string(),
                            control: Control::Slider {
                                value: 0.5,
                                min: 0.0,
                                max: 2.0,
                            },
                            transmitter: RenderField::AoStrength,
                            expose_status: None,
                            visibility: VisibilityFunction::on_parameter("shading-mode", |c| {
                                if let Control::Enum { selected, .. } = c {
                                    unsafe { ShadingMode::from_unchecked(*selected as u32) }
                                        .has_lights()
                                } else {
                                    false
                                }
                            }),
                            presetable: false,
                        },
                        Parameter {
                            name: "fog-strength".to_string(),
                            control: Control::Slider {
                                value: 0.0,
                                min: 0.0,
                                max: 1.0,
                            },
                            transmitter: RenderField::FogStrength,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
                ParamCategory {
                    name: "matcap",
                    is_open: true,
                    visibility: VisibilityFunction::on_parameter("shading-mode", |c| {
                        if let Control::Enum { selected, .. } = c {
                            unsafe { ShadingMode::from_unchecked(*selected as u32) }.has_matcap()
                        } else {
                            false
                        }
                    }),
                    parameters: vec![Parameter {
                        name: "matcap-file".to_string(),
                        control: Control::File { selected: None },
                        transmitter: RenderField::Matcap,
                        expose_status: None,
                        visibility: VisibilityFunction::default(),
                        presetable: false,
                    }],
                },
                ParamCategory {
                    name: "light",
                    is_open: true,
                    visibility: VisibilityFunction::on_parameter("shading-mode", |c| {
                        if let Control::Enum { selected, .. } = c {
                            unsafe { ShadingMode::from_unchecked(*selected as u32) }.has_lights()
                        } else {
                            false
                        }
                    }),
                    parameters: vec![
                        Parameter {
                            name: "light-type".to_string(),
                            control: Control::Enum {
                                selected: 0,
                                variants: super::LightType::VARIANTS
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect(),
                            },
                            transmitter: RenderField::LightType,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
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
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "light-size".to_string(),
                            control: Control::Slider {
                                value: 1.0,
                                min: 0.01,
                                max: 2.0,
                            },
                            transmitter: RenderField::LightSize,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "shadow".to_string(),
                            control: Control::Toggle { def: true },
                            transmitter: RenderField::Shadow,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
                ParamCategory {
                    name: "camera",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![
                        Parameter {
                            name: "focal-length".to_string(),
                            control: Control::Slider {
                                value: 1.0,
                                min: 0.2,
                                max: 10.0,
                            },
                            transmitter: RenderField::FocalLength,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "aperture-size".to_string(),
                            control: Control::Slider {
                                value: 0.0,
                                min: 0.0,
                                max: 0.1,
                            },
                            transmitter: RenderField::ApertureSize,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "aperture-blades".to_string(),
                            control: Control::DiscreteSlider {
                                value: 6,
                                min: 0,
                                max: 12,
                            },
                            transmitter: RenderField::ApertureBlades,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "aperture-rotation".to_string(),
                            control: Control::Slider {
                                value: 0.0,
                                min: 0.0,
                                max: std::f32::consts::TAU,
                            },
                            transmitter: RenderField::ApertureRotation,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "focal-distance".to_string(),
                            control: Control::Slider {
                                value: 5.0,
                                min: 1.0,
                                max: 40.0,
                            },
                            transmitter: RenderField::FocalDistance,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
            ],
        }
    }
}

impl ParamBoxDescription<GraphField> {
    /// Construct a parameter box for graph metadata.
    pub fn graph_parameters(name: &str) -> Self {
        Self {
            box_title: "graph-tab".to_string(),
            categories: vec![ParamCategory {
                name: "graph-attributes",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![Parameter {
                    name: "graph-name".to_string(),
                    control: Control::Entry {
                        value: name.to_owned(),
                    },
                    transmitter: GraphField::Name,
                    expose_status: None,
                    visibility: VisibilityFunction::default(),
                    presetable: false,
                }],
            }],
        }
    }
}

impl ParamBoxDescription<LayerField> {
    /// Create a vector of monomorphized output sockets for an operator, with
    /// all polymorphic types set to grayscale.
    fn sockets<T: super::Socketed>(operator: &T) -> Vec<(String, OperatorType)> {
        use itertools::Itertools;
        operator
            .outputs()
            .iter()
            .sorted()
            .map(|(x, y)| {
                (
                    x.clone(),
                    match *y {
                        OperatorType::Monomorphic(_) => *y,
                        OperatorType::Polymorphic(_) => {
                            OperatorType::Monomorphic(ImageType::Grayscale)
                        }
                    },
                )
            })
            .collect()
    }

    /// Construct a parameter box for a fill layer.
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
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: super::MaterialChannel::iter()
                    .map(|chan| Parameter {
                        name: chan.to_string(),
                        control: Control::ChannelMap {
                            enabled: output_sockets.contains_key(&chan),
                            chan,
                            selected: operator
                                .outputs()
                                .keys()
                                .sorted()
                                .position(|x| Some(x) == output_sockets.get(&chan))
                                .unwrap_or(0),
                            sockets: Self::sockets(operator),
                        },
                        transmitter: LayerField::ConnectOutput(chan),
                        expose_status: None,
                        visibility: VisibilityFunction::default(),
                        presetable: false,
                    })
                    .collect(),
            }],
        }
    }

    /// Construct a parameter box for an FX layer.
    pub fn fx_layer_parameters<T: super::Socketed>(operator: &T) -> Self {
        use itertools::Itertools;
        use strum::IntoEnumIterator;

        Self {
            box_title: "layer".to_string(),
            categories: vec![
                ParamCategory {
                    name: "input-channels",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
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
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        })
                        .collect(),
                },
                ParamCategory {
                    name: "output-channels",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: super::MaterialChannel::iter()
                        .map(|chan| Parameter {
                            name: chan.to_string(),
                            control: Control::ChannelMap {
                                enabled: false,
                                chan,
                                selected: 0,
                                sockets: Self::sockets(operator),
                            },
                            transmitter: LayerField::ConnectOutput(chan),
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        })
                        .collect(),
                },
            ],
        }
    }
}

impl ParamBoxDescription<SurfaceField> {
    /// Construct a parameter box for surface parameters.
    pub fn surface_parameters() -> Self {
        Self {
            box_title: "surface-tab".to_string(),
            categories: vec![ParamCategory {
                name: "surface-attributes",
                is_open: true,
                visibility: VisibilityFunction::default(),
                parameters: vec![Parameter {
                    name: "parent-size".to_string(),
                    control: Control::Size {
                        size: OperatorSize::AbsoluteSize(1024),
                        allow_relative: false,
                    },
                    transmitter: SurfaceField::Resize,
                    expose_status: None,
                    visibility: VisibilityFunction::default(),
                    presetable: false,
                }],
            }],
        }
    }
}

/// Categories used in ParamBoxDescriptions
#[derive(Debug, PartialEq, Clone)]
pub struct ParamCategory<T: MessageWriter> {
    pub name: &'static str,
    pub is_open: bool,
    pub visibility: VisibilityFunction,
    pub parameters: Vec<Parameter<T>>,
}

/// Tracks whether a parameter is exposed or not.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ExposeStatus {
    Unexposed,
    Exposed,
}

/// A parameter used in ParamBoxDescriptions
#[derive(Debug, PartialEq, Clone)]
pub struct Parameter<T: MessageWriter> {
    pub name: String,
    pub transmitter: T,
    pub control: Control,
    pub expose_status: Option<ExposeStatus>,
    pub visibility: VisibilityFunction,
    pub presetable: bool,
}

#[derive(Clone)]
pub struct VisibilityFunction {
    inner: Arc<dyn Fn(&[(String, Control)]) -> bool + Send + Sync>,
}

impl VisibilityFunction {
    pub fn from_raw<F: Fn(&[(String, Control)]) -> bool + 'static + Send + Sync>(f: F) -> Self {
        Self { inner: Arc::new(f) }
    }

    pub fn run(&self, data: &[(String, Control)]) -> bool {
        (self.inner)(data)
    }

    pub fn on_parameter<F: Fn(&Control) -> bool + 'static + Send + Sync>(
        name: &'static str,
        f: F,
    ) -> Self {
        Self::from_raw(move |cs| cs.iter().any(|(n, c)| if n == name { f(c) } else { false }))
    }
}

impl Default for VisibilityFunction {
    fn default() -> Self {
        Self {
            inner: Arc::new(|_| true),
        }
    }
}

impl Debug for VisibilityFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VisibilityFunction")
    }
}

/// We hold these truths to be self-evident, that all VisibilityFunctions are
/// created equal.
impl PartialEq for VisibilityFunction {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Controls are used in ParamBoxDescription to hint at the frontend
/// implementation of a field. It is up to the frontend to use the correct
/// widgets.
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
    XYPad {
        value: [f32; 2],
        min: [f32; 2],
        max: [f32; 2],
    },
    RgbColor {
        value: [f32; 3],
    },
    Enum {
        selected: usize,
        variants: Vec<String>,
    },
    File {
        selected: Option<PathBuf>,
    },
    ImageResource {
        selected: Option<Resource<Img>>,
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
        chan: super::MaterialChannel,
        selected: usize,
        sockets: Vec<(String, OperatorType)>,
    },
    Size {
        size: OperatorSize,
        allow_relative: bool,
    },
}

impl Control {
    fn value(&self) -> Vec<u8> {
        match self {
            Self::Slider { value, .. } => value.to_data(),
            Self::DiscreteSlider { value, .. } => value.to_data(),
            Self::XYPad { value, .. } => value.to_data(),
            Self::RgbColor { value, .. } => value.to_data(),
            Self::Enum { selected, .. } => (*selected as u32).to_data(),
            Self::File { selected } => selected.to_data(),
            Self::ImageResource { selected } => selected.to_data(),
            Self::Ramp { steps } => steps.to_data(),
            Self::Toggle { def } => (if *def { 1_u32 } else { 0_u32 }).to_data(),
            Self::Entry { value } => value.to_data(),
            Self::ChannelMap {
                enabled, selected, ..
            } => ((if *enabled { 1_u32 } else { 0_u32 }), (*selected as u32)).to_data(),
            Self::Size { size, .. } => size.to_data(),
        }
    }

    pub fn set_value(&mut self, data: &[u8]) {
        match self {
            Self::Slider { value, .. } => *value = f32::from_data(data),
            Self::DiscreteSlider { value, .. } => *value = i32::from_data(data),
            Self::XYPad { value, .. } => *value = <[f32; 2]>::from_data(data),
            Self::RgbColor { value } => *value = <[f32; 3]>::from_data(data),
            Self::Enum { selected, .. } => *selected = u32::from_data(data) as usize,
            Self::File { selected } => *selected = <Option<PathBuf>>::from_data(data),
            Self::ImageResource { selected } => {
                *selected = <Option<Resource<Img>>>::from_data(data)
            }
            Self::Ramp { steps } => *steps = <Vec<[f32; 4]>>::from_data(data),
            Self::Toggle { def } => *def = u32::from_data(data) == 1,
            Self::Entry { value } => *value = String::from_data(data),
            Self::ChannelMap {
                enabled, selected, ..
            } => {
                let (n_enabled, n_selected) = <(u32, u32)>::from_data(data);
                *enabled = n_enabled == 1;
                *selected = n_selected as usize;
            }
            Self::Size { size, .. } => *size = OperatorSize::from_data(data),
        }
    }
}

/// Helper trait to obtain the parameter box from an operator.
#[enum_dispatch]
pub trait OperatorParamBox {
    fn param_box_description(&self) -> ParamBoxDescription<Field>;
}
