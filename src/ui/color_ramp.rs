use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

pub struct ColorRampPrivate {}

impl ObjectSubclass for ColorRampPrivate {
    const NAME: &'static str = "ColorRamp";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    fn class_init(_class: &mut subclass::simple::ClassStruct<Self>) {}

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {}
    }
}

impl ObjectImpl for ColorRampPrivate {
    glib_object_impl!();
}

impl WidgetImpl for ColorRampPrivate {}

impl ContainerImpl for ColorRampPrivate {}

impl BoxImpl for ColorRampPrivate {}

glib_wrapper! {
    pub struct ColorRamp(
        Object<subclass::simple::InstanceStruct<ColorRampPrivate>,
        subclass::simple::ClassStruct<ColorRampPrivate>,
        ColorRampClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

    match fn {
        get_type => || ColorRampPrivate::get_type().to_glib(),
    }
}

impl ColorRamp {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }
}

impl Default for ColorRamp {
    fn default() -> Self {
        Self::new()
    }
}
