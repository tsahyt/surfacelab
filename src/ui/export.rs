use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

trait SwizzleBox {
    fn set_model<P: IsA<gtk::TreeModel>>(&self, model: Option<&P>);
}

struct SwizzleBoxesRgb {
    layout: gtk::Grid,
    swizzle_r: gtk::ComboBoxText,
    swizzle_g: gtk::ComboBoxText,
    swizzle_b: gtk::ComboBoxText,
}

impl SwizzleBoxesRgb {
    pub fn new() -> Self {
        let new = Self {
            layout: gtk::GridBuilder::new()
                .valign(gtk::Align::Center)
                .halign(gtk::Align::Center)
                .build(),
            swizzle_r: gtk::ComboBoxText::new(),
            swizzle_g: gtk::ComboBoxText::new(),
            swizzle_b: gtk::ComboBoxText::new(),
        };

        new.layout.attach(&gtk::Label::new(Some("Red")), 0, 0, 1, 1);
        new.layout.attach(&new.swizzle_r, 0, 1, 1, 1);
        new.layout
            .attach(&gtk::Label::new(Some("Green")), 1, 0, 1, 1);
        new.layout.attach(&new.swizzle_g, 1, 1, 1, 1);
        new.layout
            .attach(&gtk::Label::new(Some("Blue")), 2, 0, 1, 1);
        new.layout.attach(&new.swizzle_b, 2, 1, 1, 1);

        new
    }
}

impl SwizzleBox for SwizzleBoxesRgb {
    fn set_model<P: IsA<gtk::TreeModel>>(&self, model: Option<&P>) {
        self.swizzle_r.set_model(model);
        self.swizzle_g.set_model(model);
        self.swizzle_b.set_model(model);
    }
}

struct SwizzleBoxesRgba {
    layout: gtk::Grid,
    swizzle_r: gtk::ComboBoxText,
    swizzle_g: gtk::ComboBoxText,
    swizzle_b: gtk::ComboBoxText,
    swizzle_a: gtk::ComboBoxText,
}

impl SwizzleBoxesRgba {
    pub fn new() -> Self {
        let new = Self {
            layout: gtk::GridBuilder::new()
                .valign(gtk::Align::Center)
                .halign(gtk::Align::Center)
                .build(),
            swizzle_r: gtk::ComboBoxText::new(),
            swizzle_g: gtk::ComboBoxText::new(),
            swizzle_b: gtk::ComboBoxText::new(),
            swizzle_a: gtk::ComboBoxText::new(),
        };

        new.layout.attach(&gtk::Label::new(Some("Red")), 0, 0, 1, 1);
        new.layout.attach(&new.swizzle_r, 0, 1, 1, 1);
        new.layout
            .attach(&gtk::Label::new(Some("Green")), 1, 0, 1, 1);
        new.layout.attach(&new.swizzle_g, 1, 1, 1, 1);
        new.layout
            .attach(&gtk::Label::new(Some("Blue")), 2, 0, 1, 1);
        new.layout.attach(&new.swizzle_b, 2, 1, 1, 1);
        new.layout
            .attach(&gtk::Label::new(Some("Alpha")), 3, 0, 1, 1);
        new.layout.attach(&new.swizzle_a, 3, 1, 1, 1);

        new
    }
}

impl SwizzleBox for SwizzleBoxesRgba {
    fn set_model<P: IsA<gtk::TreeModel>>(&self, model: Option<&P>) {
        self.swizzle_r.set_model(model);
        self.swizzle_g.set_model(model);
        self.swizzle_b.set_model(model);
        self.swizzle_a.set_model(model);
    }
}

struct SwizzleBoxesGray {
    layout: gtk::Grid,
    swizzle_l: gtk::ComboBoxText,
}

impl SwizzleBoxesGray {
    pub fn new() -> Self {
        let new = Self {
            layout: gtk::GridBuilder::new()
                .valign(gtk::Align::Center)
                .halign(gtk::Align::Center)
                .build(),
            swizzle_l: gtk::ComboBoxText::new(),
        };

        new.layout.attach(&new.swizzle_l, 0, 1, 1, 1);

        new
    }
}

impl SwizzleBox for SwizzleBoxesGray {
    fn set_model<P: IsA<gtk::TreeModel>>(&self, model: Option<&P>) {
        self.swizzle_l.set_model(model);
    }
}

pub struct ExportRowPrivate {
    name_entry: gtk::Entry,
    swizzle_stack: gtk::Stack,
    swizzle_select: gtk::StackSwitcher,
    swizzle_rgba: SwizzleBoxesRgba,
    swizzle_rgb: SwizzleBoxesRgb,
    swizzle_grayscale: SwizzleBoxesGray,
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
            swizzle_stack: gtk::StackBuilder::new()
                .transition_type(gtk::StackTransitionType::SlideUp)
                .build(),
            swizzle_select: gtk::StackSwitcherBuilder::new()
                .orientation(gtk::Orientation::Vertical)
                .build(),
            swizzle_rgba: SwizzleBoxesRgba::new(),
            swizzle_rgb: SwizzleBoxesRgb::new(),
            swizzle_grayscale: SwizzleBoxesGray::new(),
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

        self.swizzle_rgba.set_model(Some(&store));
        self.swizzle_rgb.set_model(Some(&store));
        self.swizzle_grayscale.set_model(Some(&store));

        box_.pack_start(&self.name_entry, false, false, 0);
        box_.pack_start(&self.swizzle_select, true, true, 0);
        box_.pack_start(&self.swizzle_stack, true, true, 0);

        self.swizzle_select.set_stack(Some(&self.swizzle_stack));
        self.swizzle_stack
            .add_titled(&self.swizzle_rgba.layout, "rgba", "RGBA");
        self.swizzle_stack
            .add_titled(&self.swizzle_rgb.layout, "rgb", "RGB");
        self.swizzle_stack
            .add_titled(&self.swizzle_grayscale.layout, "gray", "Grayscale");
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
