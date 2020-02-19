use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;

pub trait WidgetImplExtra: WidgetImplExtraExt + 'static {
    fn map(&self, widget: &gtk::Widget) {
        self.parent_map(widget);
    }

    fn unmap(&self, widget: &gtk::Widget) {
        self.parent_unmap(widget);
    }

    fn realize(&self, widget: &gtk::Widget) {
        self.parent_realize(widget);
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        self.parent_unrealize(widget);
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        self.parent_size_allocate(widget, allocation);
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
    fn parent_realize(&self, widget: &gtk::Widget);
    fn parent_unrealize(&self, widget: &gtk::Widget);
    fn parent_size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation);
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

    fn parent_realize(&self, widget: &gtk::Widget) {
        unsafe {
            let data = self.get_type_data();
            let parent_class = data.as_ref().get_parent_class() as *mut gtk_sys::GtkWidgetClass;
            let f = (*parent_class)
                .realize
                .expect("No parent class impl for \"realize\"");
            f(widget.to_glib_none().0)
        }
    }

    fn parent_unrealize(&self, widget: &gtk::Widget) {
        unsafe {
            let data = self.get_type_data();
            let parent_class = data.as_ref().get_parent_class() as *mut gtk_sys::GtkWidgetClass;
            let f = (*parent_class)
                .unrealize
                .expect("No parent class impl for \"unrealize\"");
            f(widget.to_glib_none().0)
        }
    }

    fn parent_size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        unsafe {
            let data = self.get_type_data();
            let parent_class = data.as_ref().get_parent_class() as *mut gtk_sys::GtkWidgetClass;
            let f = (*parent_class)
                .size_allocate
                .expect("No parent class impl for \"size_allocate\"");
            f(widget.to_glib_none().0, allocation.to_glib_none_mut().0)
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

pub unsafe extern "C" fn extra_widget_realize<T: ObjectSubclass>(ptr: *mut gtk_sys::GtkWidget)
where
    T: WidgetImplExtra,
{
    let instance = &*(ptr as *mut T::Instance);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.realize(&wrap);
}

pub unsafe extern "C" fn extra_widget_unrealize<T: ObjectSubclass>(ptr: *mut gtk_sys::GtkWidget)
where
    T: WidgetImplExtra,
{
    let instance = &*(ptr as *mut T::Instance);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.unrealize(&wrap);
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

pub unsafe extern "C" fn extra_widget_size_allocate<T: ObjectSubclass>(
    ptr: *mut gtk_sys::GtkWidget,
    aptr: *mut gtk_sys::GtkAllocation,
) where
    T: WidgetImplExtra,
{
    let instance = &*(ptr as *mut T::Instance);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    let mut alloc: gtk::Allocation = from_glib_borrow(aptr);

    imp.size_allocate(&wrap, &mut alloc);
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
