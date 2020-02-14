use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::RefCell;

#[derive(Debug)]
pub enum NodeSocketIO {
    Source,
    Sink,
    Disable,
}

pub struct NodeSocketPrivate {
    event_window: RefCell<Option<gdk::Window>>,
    io: NodeSocketIO,
    rgba: gdk::RGBA,
    radius: f64,
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodeSocketPrivate {
    const NAME: &'static str = "NodeSocketPrivate";

    type ParentType = gtk::Widget;
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
        Self {
            event_window: RefCell::new(None),
            io: NodeSocketIO::Disable,
            rgba: gdk::RGBA::blue(),
            radius: 16.0,
        }
    }
}

impl ObjectImpl for NodeSocketPrivate {
    glib_object_impl!();
}

impl gtk::subclass::widget::WidgetImpl for NodeSocketPrivate {}

glib_wrapper! {
    pub struct NodeSocket(
        Object<subclass::simple::InstanceStruct<NodeSocketPrivate>,
        subclass::simple::ClassStruct<NodeSocketPrivate>,
        NodeSocketClass>)
        @extends gtk::Widget;

    match fn {
        get_type => || NodeSocketPrivate::get_type().to_glib(),
    }
}

impl NodeSocket {
    pub fn new() -> Self {
        let na: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        na.set_has_window(false);
        na
    }
}
