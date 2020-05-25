use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

pub struct ExportRowPrivate {
    name_entry: gtk::Entry,
}

impl ObjectSubclass for ExportRowPrivate {
    const NAME: &'static str = "ExportRow";
    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            name_entry: gtk::EntryBuilder::new().valign(gtk::Align::Center).build(),
        }
    }
}

impl ObjectImpl for ExportRowPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::object::Object) {
        let box_ = obj.downcast_ref::<gtk::Box>().unwrap();
        box_.set_spacing(8);

        let store = gtk::TreeStore::new(&[glib::Type::String]);
        store.insert_with_values(None, None, &[0], &[&"One"]);
        store.insert_with_values(None, None, &[0], &[&"Two"]);
        store.insert_with_values(None, None, &[0], &[&"Three"]);
        store.insert_with_values(None, None, &[0], &[&"Four"]);
    }
}

glib_wrapper! {
    pub struct ExportRow (
        Object<subclass::simple::InstanceStruct<ExportRowPrivate>,
        subclass::simple::ClassStruct<ExportRowPrivate>,
        ExportRowClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

    match fn {
        get_type => || ExportRowPrivate::get_type().to_glib(),
    }
}

impl WidgetImpl for ExportRowPrivate {}
impl ContainerImpl for ExportRowPrivate {}
impl BoxImpl for ExportRowPrivate {}

impl ExportRow {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .expect("Failed to create ExportRow")
            .downcast::<ExportRow>()
            .expect("Created ExportRow is of wrong type")
    }
}

pub struct ExportDialogPrivate {}

impl ObjectSubclass for ExportDialogPrivate {
    const NAME: &'static str = "ExportDialog";
    type ParentType = gtk::Dialog;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {}
    }
}

impl ObjectImpl for ExportDialogPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::object::Object) {
        let dialog = obj.clone().downcast::<gtk::Dialog>().unwrap();
        dialog.set_title("Export Material");
        let header_bar = dialog
            .get_header_bar()
            .expect("Export Dialog Header Bar missing")
            .downcast::<gtk::HeaderBar>()
            .expect("Header Bar is not a HeaderBar");
        header_bar.set_show_close_button(false);

        let cancel_button = gtk::ButtonBuilder::new().label("Cancel").build();
        cancel_button.connect_clicked(clone!(@weak dialog => move |_| {
            dialog.response(gtk::ResponseType::Cancel);
        }));
        let export_button = gtk::ButtonBuilder::new().label("Export").build();
        export_button.connect_clicked(clone!(@weak dialog => move |_| {
            dialog.response(gtk::ResponseType::Ok);
        }));
        header_bar.pack_start(&cancel_button);
        header_bar.pack_end(&export_button);

        let box_ = dialog.get_content_area();

        let grid = gtk::GridBuilder::new()
            .row_spacing(4)
            .column_spacing(8)
            .margin(8)
            .build();
        let directory_label = gtk::LabelBuilder::new()
            .halign(gtk::Align::End)
            .label("Target Directory")
            .build();
        let directory_picker = gtk::FileChooserButtonBuilder::new()
            .action(gtk::FileChooserAction::SelectFolder)
            .build();
        let prefix_label = gtk::LabelBuilder::new()
            .halign(gtk::Align::End)
            .label("File Prefix")
            .build();
        let prefix_entry = gtk::Entry::new();
        let list = gtk::ListBoxBuilder::new()
            .height_request(128)
            .selection_mode(gtk::SelectionMode::None)
            .build();
        let new_image_button = gtk::Button::new_with_label("New Image");
        let default_outputs = gtk::Button::new_with_label("Create Defaults");

        list.insert(&ExportRow::new(), 0);

        grid.attach(&directory_label, 0, 0, 1, 1);
        grid.attach(&directory_picker, 1, 0, 1, 1);
        grid.attach(&prefix_label, 0, 1, 1, 1);
        grid.attach(&prefix_entry, 1, 1, 1, 1);
        grid.attach(&list, 0, 2, 2, 1);
        grid.attach(&new_image_button, 0, 3, 2, 1);
        grid.attach(&default_outputs, 0, 4, 2, 1);

        box_.add(&grid);

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
        glib::Object::new(Self::static_type(), &[("use-header-bar", &1i32)])
            .expect("Failed to create ExportDialog")
            .downcast::<ExportDialog>()
            .expect("Created ExportDialog is of wrong type")
    }
}
