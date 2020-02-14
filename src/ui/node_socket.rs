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
                klass.realize = Some(extra_widget_realize::<NodeSocketPrivate>);
                klass.unrealize = Some(extra_widget_unrealize::<NodeSocketPrivate>);
                klass.map = Some(extra_widget_map::<NodeSocketPrivate>);
                klass.unmap = Some(extra_widget_unmap::<NodeSocketPrivate>);
                klass.size_allocate = Some(extra_widget_size_allocate::<NodeSocketPrivate>);
            }
        }
    }


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

impl WidgetImplExtra for NodeSocketPrivate {
    fn map(&self, _widget: &gtk::Widget) {
        // TODO: parent map

        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.show()
        }
    }

    fn unmap(&self, _widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.show()
        }

        // TODO: parent unmap
    }

    fn realize(&self, widget: &gtk::Widget) {
        widget.set_realized(true);
        let parent_window = widget.get_parent_window().expect("Node Socket without parent window!");
        let allocation = widget.get_allocation();

        let mut event_mask = widget.get_events();
        event_mask.insert(gdk::EventMask::BUTTON_PRESS_MASK);
        event_mask.insert(gdk::EventMask::BUTTON_RELEASE_MASK);
        event_mask.insert(gdk::EventMask::POINTER_MOTION_MASK);
        event_mask.insert(gdk::EventMask::TOUCH_MASK);
        event_mask.insert(gdk::EventMask::ENTER_NOTIFY_MASK);
        event_mask.insert(gdk::EventMask::LEAVE_NOTIFY_MASK);

        let window = gdk::Window::new(
            Some(&parent_window),
            &gdk::WindowAttr {
                window_type: gdk::WindowType::Child,
                x: Some(allocation.x),
                y: Some(allocation.y),
                width: (2.0 * self.radius) as _,
                height: (2.0 * self.radius) as _,
                wclass: gdk::WindowWindowClass::InputOnly,
                event_mask: event_mask.bits() as _,
                ..gdk::WindowAttr::default()
            },
        );

        widget.register_window(&window);
        self.event_window.replace(Some(window));
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            widget.unregister_window(ew);
            ew.destroy();
            self.event_window.replace(None);
        }

        // TODO: emit node socket destroyed signal
        // TODO: unrealize parent
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        widget.set_allocation(allocation);

        if widget.get_realized() {
            if let Some(ew) = self.event_window.borrow().as_ref() {
                ew.move_resize(
                    allocation.x,
                    allocation.y,
                    (2.0 * self.radius) as _,
                    (2.0 * self.radius) as _,
                );
            }
        }
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
}
