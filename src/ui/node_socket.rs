use super::subclass::*;
use gdk::prelude::*;
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
    rgba: RefCell<(f64, f64, f64, f64)>,
    radius: RefCell<f64>,
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
    fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {
        unsafe {
            // Extra overrides for container methods
            let container_class =
                &mut *(class as *mut _ as *mut <gtk::Container as ObjectType>::RustClassType);

            // Extra overrides for widget methods
            let widget_class = &mut *(container_class as *mut _
                as *mut <gtk::Widget as ObjectType>::RustClassType);
            {
                let klass =
                    &mut *(widget_class as *mut gtk::WidgetClass as *mut gtk_sys::GtkWidgetClass);
                // klass.realize = Some(extra_widget_realize::<NodeSocketPrivate>);
                // klass.unrealize = Some(extra_widget_unrealize::<NodeSocketPrivate>);
                // klass.map = Some(extra_widget_map::<NodeSocketPrivate>);
                // klass.unmap = Some(extra_widget_unmap::<NodeSocketPrivate>);
                // klass.size_allocate = Some(extra_widget_size_allocate::<NodeSocketPrivate>);
                // klass.motion_notify_event =
                //     Some(extra_widget_motion_notify_event::<NodeSocketPrivate>);
            }
        };

        class.add_signal(
            "socket-drag-begin",
            glib::SignalFlags::RUN_FIRST,
            &[],
            glib::Type::Invalid,
        );
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            rgba: RefCell::new((1., 1., 1., 1.)),
            radius: RefCell::new(16.0),
        }
    }
}

impl ObjectImpl for NodeSocketPrivate {
    glib_object_impl!();
}

impl gtk::subclass::widget::WidgetImpl for NodeSocketPrivate {
}

impl WidgetImplExtra for NodeSocketPrivate {
}

impl NodeSocketPrivate {
    fn set_rgba(&self, red: f64, green: f64, blue: f64, alpha: f64) {
        self.rgba.replace((red, green, blue, alpha));
    }

    fn get_rgba(&self) -> (f64, f64, f64, f64) {
        self.rgba.borrow().clone()
    }

    fn set_radius(&self, radius: f64) {
        self.radius.replace(radius);
    }

    fn get_radius(&self) -> f64 {
        self.radius.borrow().clone()
    }
}

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

    pub fn set_rgba(&self, red: f64, green: f64, blue: f64, alpha: f64) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_rgba(red, green, blue, alpha);
    }

    pub fn get_rgba(&self) -> (f64, f64, f64, f64) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.get_rgba()
    }

    pub fn set_radius(&self, radius: f64) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_radius(radius);
    }

    pub fn get_radius(&self) -> f64 {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.get_radius()
    }
}
