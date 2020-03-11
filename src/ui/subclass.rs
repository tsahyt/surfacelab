use glib::subclass::prelude::*;
use glib::translate::*;

// TODO: merge into gtk-rs
// TODO: submit issue to gtk-rs about how Ext traits should be exported from subclass::prelude
pub trait WidgetImplExtra: WidgetImplExtraExt + 'static {
    fn map(&self, widget: &gtk::Widget) {
        self.parent_map(widget);
    }

    fn unmap(&self, widget: &gtk::Widget) {
        self.parent_unmap(widget);
    }

    fn motion_notify_event(
        &self,
        widget: &gtk::Widget,
        event: &mut gdk::EventMotion,
    ) -> gtk::Inhibit {
        self.parent_motion_notify_event(widget, event)
    }
}

pub trait WidgetImplExtraExt {
    fn parent_map(&self, widget: &gtk::Widget);
    fn parent_unmap(&self, widget: &gtk::Widget);
    fn parent_motion_notify_event(
        &self,
        widget: &gtk::Widget,
        event: &mut gdk::EventMotion,
    ) -> gtk::Inhibit;
}

impl<T: WidgetImplExtra + ObjectImpl> WidgetImplExtraExt for T {
    fn parent_map(&self, widget: &gtk::Widget) {
        unsafe {
            let data = self.get_type_data();
            let parent_class = data.as_ref().get_parent_class() as *mut gtk_sys::GtkWidgetClass;
            let f = (*parent_class)
                .map
                .expect("No parent class impl for \"map\"");
            f(widget.to_glib_none().0)
        }
    }

    fn parent_unmap(&self, widget: &gtk::Widget) {
        unsafe {
            let data = self.get_type_data();
            let parent_class = data.as_ref().get_parent_class() as *mut gtk_sys::GtkWidgetClass;
            let f = (*parent_class)
                .unmap
                .expect("No parent class impl for \"unmap\"");
            f(widget.to_glib_none().0)
        }
    }

    fn parent_motion_notify_event(
        &self,
        widget: &gtk::Widget,
        event: &mut gdk::EventMotion,
    ) -> gtk::Inhibit {
        unsafe {
            let data = self.get_type_data();
            let parent_class = data.as_ref().get_parent_class() as *mut gtk_sys::GtkWidgetClass;
            let f = (*parent_class)
                .motion_notify_event
                .expect("No parent class impl for \"motion_notify_event\"");
            gtk::Inhibit(from_glib(f(
                widget.to_glib_none().0,
                event.to_glib_none_mut().0,
            )))
        }
    }
}

pub unsafe extern "C" fn extra_widget_map<T: ObjectSubclass>(ptr: *mut gtk_sys::GtkWidget)
where
    T: WidgetImplExtra,
{
    let instance = &*(ptr as *mut T::Instance);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.map(&wrap);
}

pub unsafe extern "C" fn extra_widget_unmap<T: ObjectSubclass>(ptr: *mut gtk_sys::GtkWidget)
where
    T: WidgetImplExtra,
{
    let instance = &*(ptr as *mut T::Instance);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.unmap(&wrap);
}

pub unsafe extern "C" fn extra_widget_motion_notify_event<T: ObjectSubclass>(
    ptr: *mut gtk_sys::GtkWidget,
    mptr: *mut gdk_sys::GdkEventMotion,
) -> glib_sys::gboolean
where
    T: WidgetImplExtra,
{
    let instance = &*(ptr as *mut T::Instance);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    let mut alloc: gdk::EventMotion = from_glib_borrow(mptr);

    imp.motion_notify_event(&wrap, &mut alloc).to_glib()
}
