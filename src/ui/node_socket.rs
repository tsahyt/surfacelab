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
                klass.realize = Some(extra_widget_realize::<NodeSocketPrivate>);
                klass.unrealize = Some(extra_widget_unrealize::<NodeSocketPrivate>);
                klass.map = Some(extra_widget_map::<NodeSocketPrivate>);
                klass.unmap = Some(extra_widget_unmap::<NodeSocketPrivate>);
                klass.size_allocate = Some(extra_widget_size_allocate::<NodeSocketPrivate>);
                // klass.motion_notify_event =
                //     Some(extra_widget_motion_notify_event::<NodeSocketPrivate>);
            }
        };
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            event_window: RefCell::new(None),
            rgba: RefCell::new((0.53, 0.71, 0.3, 1.)),
            radius: RefCell::new(8.0),
        }
    }
}

impl ObjectImpl for NodeSocketPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let node = obj.clone().downcast::<NodeSocket>().unwrap();
        node.set_has_window(true);
        node.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &[], gdk::DragAction::DEFAULT);
        node.drag_dest_set(gtk::DestDefaults::DROP, &[], gdk::DragAction::DEFAULT);
    }
}

impl gtk::subclass::widget::WidgetImpl for NodeSocketPrivate {
    fn get_preferred_width(&self, _widget: &gtk::Widget) -> (i32, i32) {
        let s = *self.radius.borrow() as i32 * 2;
        (s, s)
    }

    fn get_preferred_height(&self, _widget: &gtk::Widget) -> (i32, i32) {
        let s = *self.radius.borrow() as i32 * 2;
        (s, s)
    }

    fn draw(&self, _widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        let r = *self.radius.borrow();
        let (red, green, blue, alpha) = *self.rgba.borrow();

        cr.set_source_rgba(red, green, blue, alpha);
        cr.arc(r, r, r, 0.0, std::f64::consts::TAU);
        cr.fill();

        Inhibit(false)
    }
}

impl WidgetImplExtra for NodeSocketPrivate {
    fn map(&self, widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.show();
        }

        self.parent_map(widget);
    }

    fn unmap(&self, widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.hide();
        }

        self.parent_unmap(widget);
    }

    fn realize(&self, widget: &gtk::Widget) {
        widget.set_realized(true);
        let allocation = widget.get_allocation();
        let attributes = gdk::WindowAttr {
            window_type: gdk::WindowType::Child,
            x: Some(allocation.x),
            y: Some(allocation.y),
            width: allocation.width,
            height: allocation.height,
            wclass: gdk::WindowWindowClass::InputOnly,
            event_mask: {
                let mut em = widget.get_events();
                em.insert(gdk::EventMask::BUTTON_PRESS_MASK);
                em.insert(gdk::EventMask::BUTTON_RELEASE_MASK);
                em.insert(gdk::EventMask::POINTER_MOTION_MASK);
                em.insert(gdk::EventMask::ENTER_NOTIFY_MASK);
                em.insert(gdk::EventMask::LEAVE_NOTIFY_MASK);
                em.insert(gdk::EventMask::TOUCH_MASK);
                em.bits() as _
            },
            ..gdk::WindowAttr::default()
        };

        let window = widget
            .get_parent_window()
            .expect("Node Socket without parent!");
        widget.set_window(&window);
        // TODO: g_object_ref(window)?

        let event_window = gdk::Window::new(Some(&window), &attributes);
        widget.register_window(&event_window);
        self.event_window.replace(Some(event_window));
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        let mut delete_window = false;

        if let Some(ew) = self.event_window.borrow().as_ref() {
            widget.unregister_window(ew);
            ew.destroy();
            delete_window = true;
        }

        if delete_window {
            self.event_window.replace(None);
        }

        self.parent_unrealize(widget);
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.move_resize(
                allocation.x,
                allocation.y,
                allocation.width,
                allocation.height,
            );
        }

        self.parent_size_allocate(widget, allocation);
    }
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
