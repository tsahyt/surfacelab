use crate::lang::*;

use glib::subclass;
use glib::subclass::prelude::*;

use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::subclass::widget::WidgetImplExt;

struct GdkRawWindowHandle(raw_window_handle::RawWindowHandle);

#[link(name = "gdk-3")]
extern "C" {
    fn gdk_wayland_window_get_wl_surface(window: *const gdk_sys::GdkWindow) -> *mut libc::c_void;
    fn gdk_wayland_display_get_wl_display(display: *const gdk_sys::GdkDisplay)
        -> *mut libc::c_void;
}

fn gdk_wayland_handle(window: gdk::Window, display: gdk::Display) -> GdkRawWindowHandle {
    let gdkwindow: *const gdk_sys::GdkWindow = window.to_glib_none().0;
    let gdkdisplay: *const gdk_sys::GdkDisplay = display.to_glib_none().0;
    let handle = unsafe {
        raw_window_handle::unix::WaylandHandle {
            surface: gdk_wayland_window_get_wl_surface(gdkwindow),
            display: gdk_wayland_display_get_wl_display(gdkdisplay),
            ..raw_window_handle::unix::WaylandHandle::empty()
        }
    };

    GdkRawWindowHandle(raw_window_handle::RawWindowHandle::Wayland(handle))
}

pub struct RenderAreaPrivate {}

impl ObjectSubclass for RenderAreaPrivate {
    const NAME: &'static str = "RenderArea";

    type ParentType = gtk::DrawingArea;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    //fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {}

    fn new() -> Self {
        RenderAreaPrivate {}
    }
}

impl ObjectImpl for RenderAreaPrivate {
    glib_object_impl!();
}

impl WidgetImpl for RenderAreaPrivate {
    fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        Inhibit(false)
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &gtk::Allocation) {
        // TODO: notify render backend of size changes to recreate swap chain
        self.parent_size_allocate(widget, allocation);
    }

    fn realize(&self, widget: &gtk::Widget) {
        // TODO: extract the window and send message to render backend containing handle
        self.parent_realize(widget);
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        // TODO: notify render backend of termination of window
        self.parent_unrealize(widget);
    }
}

impl DrawingAreaImpl for RenderAreaPrivate {}

glib_wrapper! {
    pub struct RenderArea(
        Object<subclass::simple::InstanceStruct<RenderAreaPrivate>,
        subclass::simple::ClassStruct<RenderAreaPrivate>,
        RenderAreaClass>)
        @extends gtk::Widget, gtk::DrawingArea;

    match fn {
        get_type => || RenderAreaPrivate::get_type().to_glib(),
    }
}

impl RenderArea {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }
}
