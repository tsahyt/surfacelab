use gtk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::subclass::prelude::*;

pub struct ExportDialogPrivate {
}

impl ObjectSubclass for ExportDialogPrivate {
    const NAME: &'static str = "ExportDialog";
    type ParentType = gtk::Dialog;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
        }
    }
}

impl ObjectImpl for ExportDialogPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::object::Object) {
        let dialog = obj.clone().downcast::<gtk::Dialog>().unwrap();
        let box_ = dialog.get_content_area();

        let label = gtk::Label::new(Some("Foobar"));
        box_.add(&label);

        dialog.show_all();
    }
}

impl WidgetImpl for ExportDialogPrivate {}
impl ContainerImpl for ExportDialogPrivate {}
impl BinImpl for ExportDialogPrivate {}
impl WindowImpl for ExportDialogPrivate {}
impl DialogImpl for ExportDialogPrivate {}

glib_wrapper! {
    pub struct ExportDialog (
        Object<subclass::simple::InstanceStruct<ExportDialogPrivate>,
        subclass::simple::ClassStruct<ExportDialogPrivate>,
        ExportDialogClass>)
        @extends gtk::Widget, gtk::Container, gtk::Bin, gtk::Window, gtk::Dialog;

    match fn {
        get_type => || ExportDialogPrivate::get_type().to_glib(),
    }
}

impl ExportDialog {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .expect("Failed to create ExportDialog")
            .downcast::<ExportDialog>()
            .expect("Created ExportDialog is of wrong type")
    }
}
