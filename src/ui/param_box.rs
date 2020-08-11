use crate::lang::parameters::ParameterField;
use crate::lang::*;

use enum_dispatch::*;

use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

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
    fn construct<T: 'static + MessageWriter + Copy>(&self, description: &ParamBoxDescription<T>) {
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

            for parameter in category.parameters.iter().filter(|x| x.available) {
                let param_layout = gtk::Box::new(gtk::Orientation::Horizontal, 16);

                let param_label = gtk::Label::new(Some(parameter.name));
                param_label_group.add_widget(&param_label);
                let param_control = construct(
                    &parameter.control,
                    description.resource.clone(),
                    parameter.transmitter,
                );
                param_control_group.add_widget(&param_control);

                param_layout.pack_start(&param_label, false, false, 4);
                param_layout.pack_end(&param_control, false, true, 4);

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
    pub fn new<T: 'static + MessageWriter + Copy>(description: &ParamBoxDescription<T>) -> Self {
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
            resource: Rc::new(RefCell::new(Resource::unregistered_node())),
            categories: &[],
        })
    }
}

const SLIDER_WIDTH: i32 = 256;

pub fn construct<T: 'static + MessageWriter>(
    control: &Control,
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
) -> gtk::Widget {
    match control {
        Control::Slider { value, min, max } => {
            construct_slider(*value, *min, *max, resource, transmitter)
        }
        Control::DiscreteSlider { value, min, max } => {
            construct_discrete_slider(*value, *min, *max, resource, transmitter)
        }
        Control::RgbColor { value } => {
            construct_rgba(resource, transmitter, [value[0], value[1], value[2], 1.0])
        }
        Control::RgbaColor { value } => construct_rgba(resource, transmitter, *value),
        Control::Enum { selected, variants } => {
            construct_enum(*selected, variants, resource, transmitter)
        }
        Control::File { selected } => construct_file(selected, resource, transmitter),
        Control::Ramp { steps } => construct_ramp(steps, resource, transmitter),
        Control::Toggle { def } => construct_toggle(*def, resource, transmitter),
        Control::Entry { value } => construct_entry(value, resource, transmitter),
    }
}

fn construct_entry<T: 'static + MessageWriter>(
    value: &str,
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
) -> gtk::Widget {
    let entry = gtk::EntryBuilder::new().text(value).build();

    entry.connect_activate(clone!(@strong resource => move |w| {
        let buf = w.get_text().to_string().as_bytes().to_vec();
        super::emit(transmitter.transmit(resource.borrow().clone(), &buf));
    }));

    entry.upcast()
}

fn construct_toggle<T: 'static + MessageWriter>(
    default: bool,
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
) -> gtk::Widget {
    let toggle = gtk::SwitchBuilder::new().active(default).build();

    toggle.connect_state_set(clone!(@strong resource => move |_, active| {
        super::emit(transmitter.transmit(resource.borrow().clone(),
            &(if active { 1 as u32 } else { 0 as u32 }).to_data()));
        Inhibit(true)
    }));

    toggle.upcast()
}

fn construct_slider<T: 'static + MessageWriter>(
    value: f32,
    min: f32,
    max: f32,
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
) -> gtk::Widget {
    let adjustment = gtk::Adjustment::new(min as _, min as _, max as _, 0.01, 0.01, 0.);
    adjustment.set_value(value as _);
    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
    scale.set_size_request(SLIDER_WIDTH, 0);

    adjustment.connect_value_changed(clone!(@strong resource => move |a| {
        super::emit(transmitter.transmit(resource.borrow().clone(), &(a.get_value() as f32).to_data()));
    }));

    scale.upcast()
}

fn construct_discrete_slider<T: 'static + MessageWriter>(
    value: i32,
    min: i32,
    max: i32,
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
) -> gtk::Widget {
    let adjustment = gtk::Adjustment::new(min as _, min as _, max as _, 1., 1., 0.);
    adjustment.set_value(value as _);
    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
    scale.set_size_request(SLIDER_WIDTH, 0);
    scale.set_digits(0);
    scale.set_round_digits(0);

    adjustment.connect_value_changed(clone!(@strong resource => move |a| {
        super::emit(transmitter.transmit(resource.borrow().clone(), &(a.get_value() as i32).to_data()));
    }));

    scale.upcast()
}

fn construct_enum<T: 'static + MessageWriter>(
    selected: usize,
    entries: &[&str],
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
) -> gtk::Widget {
    let combo = gtk::ComboBoxText::new();

    for (i, entry) in entries.iter().enumerate() {
        combo.insert_text(i as _, entry);
    }

    combo.set_active(Some(selected as _));

    combo.connect_changed(clone!(@strong resource => move |c| {
        super::emit(transmitter.transmit(resource.borrow().clone(), &(c.get_active().unwrap_or(0)).to_data()));
    }));

    combo.upcast()
}

fn construct_rgba<T: 'static + MessageWriter>(
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
    color: [f32; 4],
) -> gtk::Widget {
    let wheel =
        super::color_wheel::ColorWheel::new_with_rgb(color[0] as _, color[1] as _, color[2] as _);

    wheel.connect_color_picked(clone!(@strong resource => move |_, r, g, b| {
        super::emit(transmitter.transmit(resource.borrow().clone(), &[r as f32, g as f32, b as f32].to_data()));
    }));

    wheel.upcast()
}

fn construct_file<T: 'static + MessageWriter>(
    selected: &Option<std::path::PathBuf>,
    resource: Rc<RefCell<Resource>>,
    transmitter: T,
) -> gtk::Widget {
    let button = gtk::FileChooserButton::new("Image", gtk::FileChooserAction::Open);

    if let Some(p) = selected {
        button.set_filename(&p);
    }

    button.connect_file_set(clone!(@strong resource => move |btn| {
        let buf = btn.get_filename().unwrap().to_str().unwrap().as_bytes().to_vec();
        super::emit(transmitter.transmit(resource.borrow().clone(), &buf));
    }));

    button.upcast()
}

fn construct_ramp<T: 'static + MessageWriter>(
    steps: &[[f32; 4]],
    resource: Rc<RefCell<Resource>>,
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
        super::emit(transmitter.transmit(resource.borrow().clone(), &buf));
    }));

    ramp.upcast()
}

pub fn node_attributes(res: Rc<RefCell<Resource>>, scalable: bool) -> ParamBox {
    ParamBox::new(&ParamBoxDescription {
        box_title: "Node Attributes",
        resource: res.clone(),
        categories: &[ParamCategory {
            name: "Node",
            parameters: &[
                Parameter {
                    name: "Node Resource",
                    transmitter: ResourceField::Name,
                    control: Control::Entry {
                        value: res
                            .borrow()
                            .path()
                            .file_name()
                            .and_then(|x| x.to_str())
                            .unwrap(),
                    },
                    available: true,
                },
                Parameter {
                    name: "Size",
                    transmitter: ResourceField::Size,
                    control: Control::DiscreteSlider {
                        value: 0,
                        min: -16,
                        max: 16,
                    },
                    available: scalable,
                },
                Parameter {
                    name: "Absolute Size",
                    transmitter: ResourceField::AbsoluteSize,
                    control: Control::Toggle { def: false },
                    available: scalable,
                },
            ],
        }],
    })
}

// TODO: Ideally this should not return a parambox but just its description. decoupling UI from backend
#[enum_dispatch]
pub trait OperatorParamBox {
    fn param_box(&self, res: Rc<RefCell<Resource>>) -> ParamBox;
}

impl OperatorParamBox for ComplexOperator {
    fn param_box(&self, res: Rc<RefCell<Resource>>) -> ParamBox {
        ParamBox::new(&ParamBoxDescription {
            box_title: "Complex",
            resource: res.clone(),
            categories: (&[] as &[ParamCategory<Field>]),
        })
    }
}
