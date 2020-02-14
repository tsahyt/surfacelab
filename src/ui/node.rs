use super::subclass::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::Cell;

pub struct NodePrivate {
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodePrivate {
    const NAME: &'static str = "NodePrivate";

    type ParentType = gtk::Box;
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
                klass.realize = Some(extra_widget_realize::<NodePrivate>);
                klass.unrealize = Some(extra_widget_unrealize::<NodePrivate>);
                klass.map = Some(extra_widget_map::<NodePrivate>);
                klass.unmap = Some(extra_widget_unmap::<NodePrivate>);
                klass.size_allocate = Some(extra_widget_size_allocate::<NodePrivate>);
            }
        }
    }


    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {}
    }
}

impl ObjectImpl for NodePrivate {
    glib_object_impl!();
}

impl gtk::subclass::widget::WidgetImpl for NodePrivate {
    fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        // TODO: draw
        Inhibit(false)
    }

    fn button_press_event(&self, widget: &gtk::Widget, event: &gdk::EventButton) -> gtk::Inhibit {
        // TODO: button_press_event
        Inhibit(false)
    }

    fn button_release_event(&self, widget: &gtk::Widget, event: &gdk::EventButton) -> gtk::Inhibit {
        // TODO: button_release_event
        Inhibit(false)
    }
}

impl WidgetImplExtra for NodePrivate {
    fn map(&self, widget: &gtk::Widget) {
        // TODO: map
    }

    fn unmap(&self, widget: &gtk::Widget) {
        // TODO: unmap
    }

    fn realize(&self, widget: &gtk::Widget) {
        // TODO: realize
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        // TODO: unrealize
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        // TODO: size_allocate
    }
}

impl gtk::subclass::container::ContainerImpl for NodePrivate {
    fn add(&self, container: &gtk::Container, widget: &gtk::Widget) {
        // TODO: add
    }

    fn remove(&self, container: &gtk::Container, widget: &gtk::Widget) {
        // TODO: remove
    }

    // TODO: ContainerImplExtras: forall, {get,set}_child_property
}

impl gtk::subclass::box_::BoxImpl for NodePrivate {}

glib_wrapper! {
    pub struct Node(
        Object<subclass::simple::InstanceStruct<NodePrivate>,
        subclass::simple::ClassStruct<NodePrivate>,
        NodeClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

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
