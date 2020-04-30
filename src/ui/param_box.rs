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
        let pbox = obj.clone().downcast::<ParamBox>().unwrap();
        pbox.add(&self.inner);
    }
}

impl WidgetImpl for ParamBoxPrivate {}

impl ContainerImpl for ParamBoxPrivate {}

impl BinImpl for ParamBoxPrivate {}

impl ParamBoxPrivate {
    fn construct(&self, description: &ParamBoxDescription) {
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
                    .construct(&description.resource, parameter.field);
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
    pub fn new(description: &ParamBoxDescription) -> Self {
        let pbox = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        let private = ParamBoxPrivate::from_instance(&pbox);
        private.construct(description);
        pbox
    }

    pub fn empty() -> Self {
        Self::new(&ParamBoxDescription {
            box_title: "",
            resource: Resource::unregistered_node(),
            categories: &[],
        })
    }
}

pub struct ParamBoxDescription {
    pub box_title: &'static str,
    pub resource: Resource,
    pub categories: &'static [ParamCategory],
}

pub struct ParamCategory {
    pub name: &'static str,
    pub parameters: &'static [Parameter],
}

pub struct Parameter {
    pub name: &'static str,
    pub field: &'static str,
    pub control: Control,
}

pub enum Control {
    Slider { min: f32, max: f32 },
    DiscreteSlider { min: i32, max: i32 },
    RgbColor,
    RgbaColor,
    Enum(&'static [&'static str]),
    File,
    Ramp,
    Toggle { def: bool },
}

impl Control {
    const SLIDER_WIDTH: i32 = 256;

    pub fn construct(&self, resource: &Resource, field: &'static str) -> gtk::Widget {
        match self {
            Self::Slider { min, max } => Self::construct_slider(*min, *max, resource, field),
            Self::DiscreteSlider { min, max } => {
                Self::construct_discrete_slider(*min, *max, resource, field)
            }
            Self::RgbColor => Self::construct_rgba(resource, field),
            Self::RgbaColor => Self::construct_rgba(resource, field),
            Self::Enum(entries) => Self::construct_enum(entries, resource, field),
            Self::File => Self::construct_file(resource, field),
            Self::Ramp => Self::construct_ramp(resource, field),
            Self::Toggle { def } => Self::construct_toggle(*def, resource, field),
        }
    }

    fn construct_toggle(default: bool, resource: &Resource, field: &'static str) -> gtk::Widget {
        let toggle = gtk::SwitchBuilder::new().active(default).build();

        toggle.connect_state_set(clone!(@strong resource => move |w, active| {
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                resource.to_owned(),
                field,
                (if active { 1 as u32 } else { 0 as u32 }).to_be_bytes().to_vec(),
            )));
            Inhibit(true)
        }));

        toggle.upcast()
    }

    fn construct_slider(
        min: f32,
        max: f32,
        resource: &Resource,
        field: &'static str,
    ) -> gtk::Widget {
        let adjustment = gtk::Adjustment::new(min as _, min as _, max as _, 0.01, 0.01, 0.);
        let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
        scale.set_size_request(Self::SLIDER_WIDTH, 0);

        adjustment.connect_value_changed(clone!(@strong resource => move |a| {
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                resource.to_owned(),
                field,
                (a.get_value() as f32).to_be_bytes().to_vec(),
            )));
        }));

        scale.upcast()
    }

    fn construct_discrete_slider(
        min: i32,
        max: i32,
        resource: &Resource,
        field: &'static str,
    ) -> gtk::Widget {
        let adjustment = gtk::Adjustment::new(min as _, min as _, max as _, 1., 1., 0.);
        let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
        scale.set_size_request(Self::SLIDER_WIDTH, 0);

        adjustment.connect_value_changed(clone!(@strong resource => move |a| {
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                resource.to_owned(),
                field,
                (a.get_value() as u32).to_be_bytes().to_vec(),
            )));
        }));

        scale.upcast()
    }

    fn construct_enum(entries: &[&str], resource: &Resource, field: &'static str) -> gtk::Widget {
        let combo = gtk::ComboBoxText::new();

        for (i, entry) in entries.iter().enumerate() {
            combo.insert_text(i as _, entry);
        }

        combo.connect_changed(clone!(@strong resource => move |c| {
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                resource.to_owned(),
                field,
                (c.get_active().unwrap_or(0)).to_be_bytes().to_vec()
            )))
        }));

        combo.upcast()
    }

    fn construct_rgba(resource: &Resource, field: &'static str) -> gtk::Widget {
        let wheel = super::color_wheel::ColorWheel::new();

        wheel.connect_color_picked(clone!(@strong resource => move |_, r, g, b| {
            let mut buf = Vec::new();
            buf.extend_from_slice(&(r as f32).to_be_bytes());
            buf.extend_from_slice(&(g as f32).to_be_bytes());
            buf.extend_from_slice(&(b as f32).to_be_bytes());
            buf.extend_from_slice(&(1.0 as f32).to_be_bytes());
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                resource.to_owned(),
                field,
                buf,
            )));
        }));

        wheel.upcast()
    }

    fn construct_file(resource: &Resource, field: &'static str) -> gtk::Widget {
        let button = gtk::FileChooserButton::new("Image", gtk::FileChooserAction::Open);

        button.connect_file_set(clone!(@strong resource => move |btn| {
            let buf = btn.get_filename().unwrap().to_str().unwrap().as_bytes().to_vec();
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                resource.to_owned(),
                field,
                buf
            )))
        }));

        button.upcast()
    }

    fn construct_ramp(resource: &Resource, field: &'static str) -> gtk::Widget {
        let ramp = super::color_ramp::ColorRamp::new();

        ramp.connect_color_ramp_changed(clone!(@strong resource => move |w| {
            let mut buf = Vec::new();
            for step in w.get_ramp() {
                buf.extend_from_slice(&step[0].to_be_bytes());
                buf.extend_from_slice(&step[1].to_be_bytes());
                buf.extend_from_slice(&step[2].to_be_bytes());
                buf.extend_from_slice(&step[3].to_be_bytes());
            }
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ParameterChange(
                resource.to_owned(),
                field,
                buf
            )));
        }));

        ramp.upcast()
    }
}

// Parameter Boxes for Nodes

pub fn param_box_for_operator(op: &Operator, res: &Resource) -> ParamBox {
    match op {
        Operator::Blend(..) => blend(res),
        Operator::PerlinNoise(..) => perlin_noise(res),
        Operator::Rgb(..) => rgb(res),
        Operator::Grayscale(..) => grayscale(res),
        Operator::Ramp(..) => ramp(res),
        Operator::NormalMap(..) => normal_map(res),
        Operator::Image { .. } => image(res),
        Operator::Output { .. } => output(res),
    }
}

pub fn blend(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Blend",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[
                Parameter {
                    name: "Blend Mode",
                    field: BlendParameters::BLEND_MODE,
                    control: Control::Enum(BlendMode::VARIANTS),
                },
                Parameter {
                    name: "Clamp",
                    field: BlendParameters::CLAMP,
                    control: Control::Toggle { def: false },
                },
                Parameter {
                    name: "Mix",
                    field: BlendParameters::MIX,
                    control: Control::Slider { min: 0., max: 1. },
                },
            ],
        }],
    })
}

pub fn perlin_noise(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Perlin Noise",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[
                Parameter {
                    name: "Scale",
                    field: PerlinNoiseParameters::SCALE,
                    control: Control::Slider { min: 0., max: 16. },
                },
                Parameter {
                    name: "Octaves",
                    field: PerlinNoiseParameters::OCTAVES,
                    control: Control::DiscreteSlider { min: 0, max: 24 },
                },
                Parameter {
                    name: "Attenuation",
                    field: PerlinNoiseParameters::ATTENUATION,
                    control: Control::Slider { min: 0., max: 4. },
                },
            ],
        }],
    })
}

pub fn rgb(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "RGB Color",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Color",
                field: RgbParameters::RGB,
                control: Control::RgbColor,
            }],
        }],
    })
}

pub fn output(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Output",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Output Type",
                field: "output_type",
                control: Control::Enum(OutputType::VARIANTS),
            }],
        }],
    })
}

pub fn image(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Image",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Image Path",
                field: "image_path",
                control: Control::File,
            }],
        }],
    })
}

pub fn grayscale(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Grayscale",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Conversion Mode",
                field: GrayscaleParameters::MODE,
                control: Control::Enum(GrayscaleMode::VARIANTS),
            }],
        }],
    })
}

pub fn ramp(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Ramp",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Gradient",
                field: RampParameters::RAMP,
                control: Control::Ramp,
            }],
        }],
    })
}

pub fn normal_map(res: &Resource) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Normal Map",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Basic Parameters",
            parameters: &[Parameter {
                name: "Strength",
                field: NormalMapParameters::STRENGTH,
                control: Control::Slider { min: 0., max: 2. },
            }],
        }],
    })
}
