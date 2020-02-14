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
    rgba: RefCell<(f64, f64, f64, f64)>,
    radius: RefCell<f64>,
    input: RefCell<Option<NodeSocket>>,
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
                klass.motion_notify_event =
                    Some(extra_widget_motion_notify_event::<NodeSocketPrivate>);
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
            event_window: RefCell::new(None),
            io: NodeSocketIO::Disable,
            rgba: RefCell::new((1., 1., 1., 1.)),
            radius: RefCell::new(16.0),
            input: RefCell::new(None),
        }
    }
}

impl ObjectImpl for NodeSocketPrivate {
    glib_object_impl!();
}

impl gtk::subclass::widget::WidgetImpl for NodeSocketPrivate {
    fn get_preferred_width(&self, _widget: &gtk::Widget) -> (i32, i32) {
        let w = (2.0 * *self.radius.borrow()) as _;
        (w, w)
    }

    fn get_preferred_height(&self, _widget: &gtk::Widget) -> (i32, i32) {
        let w = (2.0 * *self.radius.borrow()) as _;
        (w, w)
    }

    fn button_press_event(&self, _widget: &gtk::Widget, _event: &gdk::EventButton) -> gtk::Inhibit {
        Inhibit(true)
    }

    fn drag_begin(&self, widget: &gtk::Widget, context: &gdk::DragContext) {
        self.set_drag_icon(context);
        context.get_drag_window().unwrap().hide();
        // TODO: emit drag begin signal
        self.drag_src_redirect(widget);
    }

    fn drag_motion(
        &self,
        _widget: &gtk::Widget,
        _context: &gdk::DragContext,
        _x: i32,
        _y: i32,
        _time: u32,
    ) -> gtk::Inhibit {
        return gtk::Inhibit(true);
    }

    fn drag_data_received(
        &self,
        widget: &gtk::Widget,
        context: &gdk::DragContext,
        x: i32,
        y: i32,
        selection_data: &gtk::SelectionData,
        info: u32,
        time: u32,
    ) {
        // TODO
    }

    fn drag_data_get(
        &self,
        _widget: &gtk::Widget,
        _context: &gdk::DragContext,
        selection_data: &gtk::SelectionData,
        _info: u32,
        _time: u32,
    ) {
        // TODO: socket
        selection_data.set(&selection_data.get_target(), 32, unimplemented!("socket"))
    }

    fn drag_failed(
        &self,
        widget: &gtk::Widget,
        context: &gdk::DragContext,
        result: gtk::DragResult,
    ) -> Inhibit {
        return Inhibit(true);
    }
}

impl WidgetImplExtra for NodeSocketPrivate {
    fn map(&self, widget: &gtk::Widget) {
        self.parent_map(widget);

        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.show()
        }
    }

    fn unmap(&self, widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.show()
        }

        self.parent_unmap(widget);
    }

    fn realize(&self, widget: &gtk::Widget) {
        widget.set_realized(true);
        let parent_window = widget
            .get_parent_window()
            .expect("Node Socket without parent window!");
        let allocation = widget.get_allocation();
        let size = 2.0 * *self.radius.borrow();

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
                width: size as _,
                height: size as _,
                wclass: gdk::WindowWindowClass::InputOnly,
                event_mask: event_mask.bits() as _,
                ..gdk::WindowAttr::default()
            },
        );

        widget.register_window(&window);
        self.event_window.replace(Some(window));
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        let mut window_destroyed = false;

        if let Some(ew) = self.event_window.borrow().as_ref() {
            widget.unregister_window(ew);
            ew.destroy();
            window_destroyed = true;
        }
        if window_destroyed {
            self.event_window.replace(None);
        }

        // TODO: emit node socket destroyed signal
        self.parent_unrealize(widget);
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        let size = 2.0 * *self.radius.borrow();
        widget.set_allocation(allocation);

        if widget.get_realized() {
            if let Some(ew) = self.event_window.borrow().as_ref() {
                ew.move_resize(allocation.x, allocation.y, size as _, size as _);
            }
        }
    }

    fn motion_notify_event(
        &self,
        _widget: &gtk::Widget,
        _event: &mut gdk::EventMotion,
    ) -> gtk::Inhibit {
        Inhibit(true)
    }
}

impl NodeSocketPrivate {
    fn set_drag_icon(&self, context: &gdk::DragContext) {
        let r = *self.radius.borrow();
        let size = (2.0 * r) as _;
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size)
            .expect("Failed to create cairo surface for drag icon");
        let cr = cairo::Context::new(&surface);

        cr.set_source_rgba(
            self.rgba.borrow().0,
            self.rgba.borrow().1,
            self.rgba.borrow().2,
            self.rgba.borrow().3,
        );
        cr.arc(r, r, r, 0., 2. * std::f64::consts::PI);
        cr.fill();

        context.drag_set_icon_surface(&surface);
    }

    fn drag_src_redirect(&self, widget: &gtk::Widget) {
        let mut disconnect = false;
        if let Some(source) = self.input.borrow().as_ref() {
            // TODO: disconnect signal handlers
            disconnect = true;

            // remove as drag source
            if let NodeSocketIO::Sink = self.io {
                widget.drag_source_unset();
            }

            // begin drag on previous source, so user can redirect connection
            source.drag_begin_with_coordinates(
                &gtk::TargetList::new(&[]),
                gdk::DragAction::COPY,
                gdk::ModifierType::BUTTON1_MASK.bits() as _,
                None,
                -1,
                -1,
            );
        }

        if disconnect {
            self.input.replace(None);
        }
    }

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
