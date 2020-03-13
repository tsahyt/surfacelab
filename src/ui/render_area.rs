use crate::lang::*;

use glib::subclass;
use glib::subclass::prelude::*;

use gdk::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::subclass::widget::WidgetImplExt;
use raw_window_handle::*;

#[link(name = "gdk-3")]
extern "C" {
    fn gdk_wayland_window_get_wl_surface(window: *const gdk_sys::GdkWindow) -> *mut libc::c_void;
    fn gdk_wayland_display_get_wl_display(display: *const gdk_sys::GdkDisplay)
        -> *mut libc::c_void;
}

fn gdk_wayland_handle(window: gdk::Window, display: gdk::Display) -> RawWindowHandle {
    let gdkwindow: *const gdk_sys::GdkWindow = window.to_glib_none().0;
    let gdkdisplay: *const gdk_sys::GdkDisplay = display.to_glib_none().0;
    let handle = unsafe {
        raw_window_handle::unix::WaylandHandle {
            surface: gdk_wayland_window_get_wl_surface(gdkwindow),
            display: gdk_wayland_display_get_wl_display(gdkdisplay),
            ..raw_window_handle::unix::WaylandHandle::empty()
        }
    };

    RawWindowHandle::Wayland(handle)
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
    fn draw(&self, widget: &gtk::Widget, _cr: &cairo::Context) -> gtk::Inhibit {
        if widget.get_realized() {
            super::emit(Lang::UIEvent(UIEvent::RendererRedraw(
                self.unique_identifier(),
            )));
        }
        Inhibit(false)
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &gtk::Allocation) {
        self.parent_size_allocate(widget, allocation);

        let width = allocation.width;
        let height = allocation.height;

        super::emit(Lang::UIEvent(UIEvent::RendererResize(
            self.unique_identifier(),
            width as _,
            height as _,
        )));
    }

    fn realize(&self, widget: &gtk::Widget) {
        // Realize parent first, such that we have a parent to work with
        self.parent_realize(widget);
        widget.show();

        let window = widget.get_window().expect("Drawing Area has no window!");
        let (w, h) = (window.get_width(), window.get_height());

        let gdk_window = gdk::Window::new(
            Some(&window),
            &gdk::WindowAttr {
                window_type: gdk::WindowType::Subsurface,
                ..gdk::WindowAttr::default()
            },
        );

        let gdk_display = gdk_window.get_display();
        gdk_window.set_transient_for(&window);
        gdk_window.show();

        let handle = gdk_wayland_handle(gdk_window, gdk_display);

        super::emit(Lang::UIEvent(UIEvent::RendererAdded(
            self.unique_identifier(),
            WindowHandle::new(handle),
            w as _,
            h as _,
        )));
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        // TODO: notify render backend of termination of window
        self.parent_unrealize(widget);

        // super::emit(Lang::UIEvent(UIEvent::RendererUnrealize));
    }
}

impl DrawingAreaImpl for RenderAreaPrivate {}

impl RenderAreaPrivate {
    /// Obtain a unique identifier for this render area.
    ///
    /// This is based on memory address of the private struct. Therefore it is
    /// guaranteed to be unique for each new instance.
    fn unique_identifier(&self) -> u64 {
        let x = self as *const Self;
        x as u64
    }
}

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

impl Default for RenderArea {
    fn default() -> Self {
        Self::new()
    }
}