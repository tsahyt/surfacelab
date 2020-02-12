use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::Cell;

pub struct NodeAreaPrivate {
    foo: Cell<i32>,
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodeAreaPrivate {
    const NAME: &'static str = "NodeAreaPrivate";

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

impl ObjectImpl for NodeAreaPrivate {
    glib_object_impl!();
}

impl gtk::subclass::widget::WidgetImpl for NodeAreaPrivate {
    fn get_preferred_width(&self, _widget: &gtk::Widget) -> (i32, i32) {
        (60, 60)
    }

    fn get_preferred_height(&self, _widget: &gtk::Widget) -> (i32, i32) {
        (60, 60)
    }

    fn draw(&self, _widget: &gtk::Widget, cr: &cairo::Context) -> Inhibit {
        cr.move_to(0., 0.);
        cr.line_to(16., 16.);
        cr.paint();
        Inhibit(false)
    }
}

impl gtk::subclass::container::ContainerImpl for NodeAreaPrivate {}

glib_wrapper! {
    pub struct NodeArea(
        Object<subclass::simple::InstanceStruct<NodeAreaPrivate>,
        subclass::simple::ClassStruct<NodeAreaPrivate>,
        NodeAreaClass>)
        @extends gtk::Widget, gtk::Container;

    match fn {
        get_type => || NodeAreaPrivate::get_type().to_glib(),
    }
}

impl NodeArea {
    pub fn new() -> Self {
        let na: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        na.set_has_window(false);
        na
    }
}
