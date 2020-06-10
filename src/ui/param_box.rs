use crate::lang::*;

use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use strum::VariantNames;

pub struct ParamBoxPrivate {
    inner: gtk::Box,
}

impl ObjectSubclass for ParamBoxPrivate {
    const NAME: &'static str = "ParamBox";

    type ParentType = gtk::Bin;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    //fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {}

    fn new() -> Self {
        ParamBoxPrivate {
            inner: gtk::Box::new(gtk::Orientation::Vertical, 8),
        }
    }
}

impl ObjectImpl for ParamBoxPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let pbox = obj.downcast_ref::<ParamBox>().unwrap();
        pbox.add(&self.inner);
    }
}

impl WidgetImpl for ParamBoxPrivate {}

impl ContainerImpl for ParamBoxPrivate {}

impl BinImpl for ParamBoxPrivate {}

impl ParamBoxPrivate {
    fn construct<T: 'static + Transmitter + Copy>(&self, description: &ParamBoxDescription<T>) {
        // size groups
        let param_label_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        let param_control_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);

        // title
        let title_label = gtk::Label::new(Some(description.box_title));
        self.inner.add(&title_label);

        for category in description.categories {
            let cat_expander = gtk::Expander::new(Some(category.name));
            self.inner.add(&cat_expander);

            let cat_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
            cat_expander.add(&cat_box);

            for parameter in category.parameters {
                let param_layout = gtk::Box::new(gtk::Orientation::Horizontal, 16);

                let param_label = gtk::Label::new(Some(parameter.name));
                param_label_group.add_widget(&param_label);
                let param_control = parameter
                    .control
                    .construct(&description.resource, parameter.transmitter);
                param_control_group.add_widget(&param_control);

                param_layout.pack_start(&param_label, false, false, 4);
                param_layout.pack_end(&param_control, true, true, 4);

                cat_box.add(&param_layout);
            }
        }
    }
}

glib_wrapper! {
    pub struct ParamBox(
        Object<subclass::simple::InstanceStruct<ParamBoxPrivate>,
        subclass::simple::ClassStruct<ParamBoxPrivate>,
        ParamBoxClass>)
        @extends gtk::Widget, gtk::Container, gtk::Bin;

    match fn {
        get_type => || ParamBoxPrivate::get_type().to_glib(),
    }
}

impl ParamBox {
    pub fn new<T: 'static + Transmitter + Copy>(description: &ParamBoxDescription<T>) -> Self {
        let pbox = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        let private = ParamBoxPrivate::from_instance(&pbox);
        private.construct(description);
        pbox
    }

    pub fn empty() -> Self {
        Self::new::<Field>(&ParamBoxDescription {
            box_title: "",
            resource: Resource::unregistered_node(),
            categories: &[],
        })
    }
}

pub trait Transmitter {
    fn transmit(&self, resource: &Resource, data: &[u8]);
}

#[derive(Copy, Clone, Debug)]
struct Field(&'static str);

impl Transmitter for Field {
    fn transmit(&self, resource: &Resource, data: &[u8]) {
        super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
            resource.to_owned(),
            self.0,
            data.to_vec(),
        )))
    }
}

pub struct ParamBoxDescription<'a, T: Transmitter> {
    pub box_title: &'static str,
    pub resource: Resource,
    pub categories: &'a [ParamCategory<'a, T>],
}

pub struct ParamCategory<'a, T: Transmitter> {
    pub name: &'static str,
    pub parameters: &'a [Parameter<T>],
}

pub struct Parameter<T: Transmitter> {
    pub name: &'static str,
    pub transmitter: T,
    pub control: Control,
}

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
    Entry,
}

impl Control {
    const SLIDER_WIDTH: i32 = 256;

    pub fn construct<T: 'static + Transmitter>(
        &self,
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        match self {
            Self::Slider { value, min, max } => {
                Self::construct_slider(*value, *min, *max, resource, transmitter)
            }
            Self::DiscreteSlider { value, min, max } => {
                Self::construct_discrete_slider(*value, *min, *max, resource, transmitter)
            }
            Self::RgbColor { value } => {
                Self::construct_rgba(resource, transmitter, [value[0], value[1], value[2], 1.0])
            }
            Self::RgbaColor { value } => Self::construct_rgba(resource, transmitter, *value),
            Self::Enum { selected, variants } => {
                Self::construct_enum(*selected, variants, resource, transmitter)
            }
            Self::File { selected } => Self::construct_file(selected, resource, transmitter),
            Self::Ramp { steps } => Self::construct_ramp(steps, resource, transmitter),
            Self::Toggle { def } => Self::construct_toggle(*def, resource, transmitter),
            Self::Entry => Self::construct_entry(resource, transmitter),
        }
    }

    fn construct_entry<T: 'static + Transmitter>(
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        gtk::Entry::new().upcast()
    }

    fn construct_toggle<T: 'static + Transmitter>(
        default: bool,
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        let toggle = gtk::SwitchBuilder::new().active(default).build();

        toggle.connect_state_set(clone!(@strong resource => move |_, active| {
            transmitter.transmit(&resource,
                &(if active { 1 as u32 } else { 0 as u32 }).to_be_bytes());
            Inhibit(true)
        }));

        toggle.upcast()
    }

    fn construct_slider<T: 'static + Transmitter>(
        value: f32,
        min: f32,
        max: f32,
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        let adjustment = gtk::Adjustment::new(min as _, min as _, max as _, 0.01, 0.01, 0.);
        adjustment.set_value(value as _);
        let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
        scale.set_size_request(Self::SLIDER_WIDTH, 0);

        adjustment.connect_value_changed(clone!(@strong resource => move |a| {
            transmitter.transmit(&resource, &(a.get_value() as f32).to_be_bytes());
        }));

        scale.upcast()
    }

    fn construct_discrete_slider<T: 'static + Transmitter>(
        value: i32,
        min: i32,
        max: i32,
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        let adjustment = gtk::Adjustment::new(min as _, min as _, max as _, 1., 1., 0.);
        adjustment.set_value(value as _);
        let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
        scale.set_size_request(Self::SLIDER_WIDTH, 0);
        scale.set_digits(0);
        scale.set_round_digits(0);

        adjustment.connect_value_changed(clone!(@strong resource => move |a| {
            transmitter.transmit(&resource, &(a.get_value() as u32).to_be_bytes());
        }));

        scale.upcast()
    }

    fn construct_enum<T: 'static + Transmitter>(
        selected: usize,
        entries: &[&str],
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        let combo = gtk::ComboBoxText::new();

        for (i, entry) in entries.iter().enumerate() {
            combo.insert_text(i as _, entry);
        }

        combo.set_active(Some(selected as _));

        combo.connect_changed(clone!(@strong resource => move |c| {
            transmitter.transmit(&resource, &(c.get_active().unwrap_or(0)).to_be_bytes());
        }));

        combo.upcast()
    }

    fn construct_rgba<T: 'static + Transmitter>(
        resource: &Resource,
        transmitter: T,
        color: [f32; 4],
    ) -> gtk::Widget {
        let wheel = super::color_wheel::ColorWheel::new_with_rgb(
            color[0] as _,
            color[1] as _,
            color[2] as _,
        );

        wheel.connect_color_picked(clone!(@strong resource => move |_, r, g, b| {
            let mut buf = Vec::new();
            buf.extend_from_slice(&(r as f32).to_be_bytes());
            buf.extend_from_slice(&(g as f32).to_be_bytes());
            buf.extend_from_slice(&(b as f32).to_be_bytes());
            buf.extend_from_slice(&(1.0 as f32).to_be_bytes());
            transmitter.transmit(&resource, &buf);
        }));

        wheel.upcast()
    }

    fn construct_file<T: 'static + Transmitter>(
        selected: &Option<std::path::PathBuf>,
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        let button = gtk::FileChooserButton::new("Image", gtk::FileChooserAction::Open);

        if let Some(p) = selected {
            button.set_filename(&p);
        }

        button.connect_file_set(clone!(@strong resource => move |btn| {
            let buf = btn.get_filename().unwrap().to_str().unwrap().as_bytes().to_vec();
            transmitter.transmit(&resource, &buf);
        }));

        button.upcast()
    }

    fn construct_ramp<T: 'static + Transmitter>(
        steps: &[[f32; 4]],
        resource: &Resource,
        transmitter: T,
    ) -> gtk::Widget {
        let ramp = super::color_ramp::ColorRamp::new_with_steps(steps);

        ramp.connect_color_ramp_changed(clone!(@strong resource => move |w| {
            let mut buf = Vec::new();
            for step in w.get_ramp() {
                buf.extend_from_slice(&step[0].to_be_bytes());
                buf.extend_from_slice(&step[1].to_be_bytes());
                buf.extend_from_slice(&step[2].to_be_bytes());
                buf.extend_from_slice(&step[3].to_be_bytes());
            }
            transmitter.transmit(&resource, &buf);
        }));

        ramp.upcast()
    }
}

// Parameter Boxes for Nodes

pub fn param_box_for_operator(op: &Operator, res: &Resource) -> ParamBox {
    match op {
        Operator::Blend(params) => blend(res, params),
        Operator::PerlinNoise(params) => perlin_noise(res, params),
        Operator::Rgb(params) => rgb(res, params),
        Operator::Grayscale(params) => grayscale(res, params),
        Operator::Ramp(params) => ramp(res, params),
        Operator::NormalMap(params) => normal_map(res, params),
        Operator::Image { path } => image(res, path.to_owned()),
        Operator::Output { output_type } => output(res, *output_type),
    }
}

pub fn node_attributes(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Node Attributes",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Node",
            parameters: &[
                Parameter {
                    name: "Node Resource",
                    transmitter: Field(""),
                    control: Control::Entry,
                },
                Parameter {
                    name: "Node Description",
                    transmitter: Field(""),
                    control: Control::Entry,
                },
            ],
        }],
    })
}

pub fn blend(res: &Resource, params: &BlendParameters) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Blend",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[
                Parameter {
                    name: "Blend Mode",
                    transmitter: Field(BlendParameters::BLEND_MODE),
                    control: Control::Enum {
                        selected: params.blend_mode as usize,
                        variants: BlendMode::VARIANTS,
                    },
                },
                Parameter {
                    name: "Clamp",
                    transmitter: Field(BlendParameters::CLAMP_OUTPUT),
                    control: Control::Toggle {
                        def: params.clamp_output == 1,
                    },
                },
                Parameter {
                    name: "Mix",
                    transmitter: Field(BlendParameters::MIX),
                    control: Control::Slider {
                        value: params.mix,
                        min: 0.,
                        max: 1.,
                    },
                },
            ],
        }],
    })
}

pub fn perlin_noise(res: &Resource, params: &PerlinNoiseParameters) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Perlin Noise",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[
                Parameter {
                    name: "Scale",
                    transmitter: Field(PerlinNoiseParameters::SCALE),
                    control: Control::Slider {
                        value: params.scale,
                        min: 0.,
                        max: 16.,
                    },
                },
                Parameter {
                    name: "Octaves",
                    transmitter: Field(PerlinNoiseParameters::OCTAVES),
                    control: Control::DiscreteSlider {
                        value: params.octaves as _,
                        min: 0,
                        max: 24,
                    },
                },
                Parameter {
                    name: "Attenuation",
                    transmitter: Field(PerlinNoiseParameters::ATTENUATION),
                    control: Control::Slider {
                        value: params.attenuation,
                        min: 0.,
                        max: 4.,
                    },
                },
            ],
        }],
    })
}

pub fn rgb(res: &Resource, params: &RgbParameters) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "RGB Color",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Color",
                transmitter: Field(RgbParameters::RGB),
                control: Control::RgbColor { value: params.rgb },
            }],
        }],
    })
}

pub fn output(res: &Resource, output_type: OutputType) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Output",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Output Type",
                transmitter: Field("output_type"),
                control: Control::Enum {
                    selected: output_type as usize,
                    variants: OutputType::VARIANTS,
                },
            }],
        }],
    })
}

pub fn image(res: &Resource, path: std::path::PathBuf) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Image",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Image Path",
                transmitter: Field("image_path"),
                control: Control::File {
                    selected: Some(path),
                },
            }],
        }],
    })
}

pub fn grayscale(res: &Resource, params: &GrayscaleParameters) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Grayscale",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Conversion Mode",
                transmitter: Field(GrayscaleParameters::MODE),
                control: Control::Enum {
                    selected: params.mode as usize,
                    variants: GrayscaleMode::VARIANTS,
                },
            }],
        }],
    })
}

pub fn ramp(res: &Resource, params: &RampParameters) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Ramp",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Gradient",
                transmitter: Field(RampParameters::RAMP),
                control: Control::Ramp {
                    steps: params.get_steps(),
                },
            }],
        }],
    })
}

pub fn normal_map(res: &Resource, params: &NormalMapParameters) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Normal Map",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Strength",
                transmitter: Field(NormalMapParameters::STRENGTH),
                control: Control::Slider {
                    value: params.strength,
                    min: 0.,
                    max: 2.,
                },
            }],
        }],
    })
}
