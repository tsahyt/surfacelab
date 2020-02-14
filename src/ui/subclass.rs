use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;

pub trait WidgetImplExtra: 'static {
    fn map(&self, widget: &gtk::Widget);
    fn unmap(&self, _widget: &gtk::Widget);
    fn realize(&self, widget: &gtk::Widget);
    fn unrealize(&self, widget: &gtk::Widget);
    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation);
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
