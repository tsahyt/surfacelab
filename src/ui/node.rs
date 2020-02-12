use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::Cell;

pub struct NodePrivate {
    foo: Cell<i32>,
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodePrivate {
    const NAME: &'static str = "NodePrivate";

    type ParentType = gtk::Container;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    // fn class_init(_class: &mut subclass::simple::ClassStruct<Self>) {}

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self { foo: Cell::new(12) }
    }
}

impl ObjectImpl for NodePrivate {
    glib_object_impl!();
}

impl gtk::subclass::widget::WidgetImpl for NodePrivate {}

impl gtk::subclass::container::ContainerImpl for NodePrivate {}

glib_wrapper! {
    pub struct Node(
        Object<subclass::simple::InstanceStruct<NodePrivate>,
        subclass::simple::ClassStruct<NodePrivate>,
        NodeClass>)
        @extends gtk::Widget, gtk::Container;

    match fn {
        get_type => || NodePrivate::get_type().to_glib(),
    }
}

impl Node {
    pub fn new() -> Self {
        let na: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        na.set_has_window(false);
        na
    }
}
