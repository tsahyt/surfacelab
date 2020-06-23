use crate::lang::*;

use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::convert::TryFrom;
use std::path::PathBuf;

pub enum ExportImageType {
    RGBA,
    RGB,
    Grayscale,
}

pub struct ExportRowPrivate {
    name_entry: gtk::Entry,
    output_type_combobox: gtk::ComboBoxText,
    channel_r_combobox: gtk::ComboBoxText,
    channel_g_combobox: gtk::ComboBoxText,
    channel_b_combobox: gtk::ComboBoxText,
    channel_a_combobox: gtk::ComboBoxText,
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
            output_type_combobox: gtk::ComboBoxText::new(),
            channel_r_combobox: gtk::ComboBoxText::new(),
            channel_g_combobox: gtk::ComboBoxText::new(),
            channel_b_combobox: gtk::ComboBoxText::new(),
            channel_a_combobox: gtk::ComboBoxText::new(),
        }
    }
}

impl ObjectImpl for ExportRowPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::object::Object) {
        let box_ = obj.downcast_ref::<gtk::Box>().unwrap();
        box_.set_spacing(8);

        box_.pack_start(&self.name_entry, true, true, 0);

        self.output_type_combobox.append_text("RGBA");
        self.output_type_combobox.append_text("RGB");
        self.output_type_combobox.append_text("Grayscale");
        box_.pack_start(&self.output_type_combobox, true, true, 0);

        let channel_boxes = gtk::ButtonBoxBuilder::new()
            .layout_style(gtk::ButtonBoxStyle::Expand)
            .build();
        channel_boxes.add(&self.channel_r_combobox);
        channel_boxes.add(&self.channel_g_combobox);
        channel_boxes.add(&self.channel_b_combobox);
        channel_boxes.add(&self.channel_a_combobox);

        self.output_type_combobox
            .connect_changed(clone!(@strong self.channel_r_combobox as r,
                       @strong self.channel_g_combobox as g,
                       @strong self.channel_b_combobox as b,
                       @strong self.channel_a_combobox as a => move |cb| {
                match cb.get_active() {
                    Some(0) => { r.show(); g.show(); b.show(); a.show(); }
                    Some(1) => { r.show(); g.show(); b.show(); a.hide(); }
                    Some(2) => { r.show(); g.hide(); b.hide(); a.hide(); }
                    _       => {}
                }
            }));

        box_.pack_end(&channel_boxes, true, true, 0);
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

fn get_resource(string: &str) -> Option<(Resource, ImageChannel)> {
    let (left, right) = string.split_at(string.len() - 2);

    let (res_string, channel) = match right {
        "#R" => (left, ImageChannel::R),
        "#G" => (left, ImageChannel::G),
        "#B" => (left, ImageChannel::B),
        "#A" => (left, ImageChannel::A),
        _ => (string, ImageChannel::R),
    };

    let resource = Resource::try_from(format!("node:{}", res_string).as_ref()).ok()?;

    Some((resource, channel))
}

impl ExportRowPrivate {
    fn set_exportable_model(&self, model: &gtk::ListStore) {
        self.channel_r_combobox.set_model(Some(model));
        self.channel_g_combobox.set_model(Some(model));
        self.channel_b_combobox.set_model(Some(model));
        self.channel_a_combobox.set_model(Some(model));
    }

    fn get_channel(&self, channel: ImageChannel) -> Option<(Resource, ImageChannel)> {
        match channel {
            ImageChannel::R => {
                if self.channel_r_combobox.is_visible() {
                    let selected = self.channel_r_combobox.get_active_text()?.to_string();
                    get_resource(&selected)
                } else {
                    None
                }
            }
            ImageChannel::G => {
                if self.channel_g_combobox.is_visible() {
                    let selected = self.channel_g_combobox.get_active_text()?.to_string();
                    get_resource(&selected)
                } else {
                    None
                }
            }
            ImageChannel::B => {
                if self.channel_b_combobox.is_visible() {
                    let selected = self.channel_b_combobox.get_active_text()?.to_string();
                    get_resource(&selected)
                } else {
                    None
                }
            }
            ImageChannel::A => {
                if self.channel_a_combobox.is_visible() {
                    let selected = self.channel_a_combobox.get_active_text()?.to_string();
                    get_resource(&selected)
                } else {
                    None
                }
            }
        }
    }

    fn get_image_type(&self) -> Option<ExportImageType> {
        match self.output_type_combobox.get_active() {
            Some(0) => Some(ExportImageType::RGBA),
            Some(1) => Some(ExportImageType::RGB),
            Some(2) => Some(ExportImageType::Grayscale),
            _ => None,
        }
    }

    fn get_filename(&self) -> std::string::String {
        self.name_entry.get_text().to_string()
    }
}

impl ExportRow {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .expect("Failed to create ExportRow")
            .downcast::<ExportRow>()
            .expect("Created ExportRow is of wrong type")
    }

    pub fn set_exportable_model(&self, model: &gtk::ListStore) {
        let imp = ExportRowPrivate::from_instance(self);
        imp.set_exportable_model(model);
    }

    pub fn get_channel(&self, channel: ImageChannel) -> Option<(Resource, ImageChannel)> {
        let imp = ExportRowPrivate::from_instance(self);
        imp.get_channel(channel)
    }

    pub fn get_image_type(&self) -> Option<ExportImageType> {
        let imp = ExportRowPrivate::from_instance(self);
        imp.get_image_type()
    }

    pub fn get_filename(&self) -> std::string::String {
        let imp = ExportRowPrivate::from_instance(self);
        imp.get_filename()
    }
}

impl Default for ExportRow {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ExportDialogPrivate {
    exportable: gtk::ListStore,
    parent_size: gtk::Adjustment,
}

impl ObjectSubclass for ExportDialogPrivate {
    const NAME: &'static str = "ExportDialog";
    type ParentType = gtk::Dialog;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            exportable: gtk::ListStore::new(&[glib::types::Type::String]),
            parent_size: gtk::Adjustment::new(1024.0, 32.0, 16384.0, 32.0, 512.0, 1024.0),
        }
    }
}

impl ObjectImpl for ExportDialogPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::object::Object) {
        let dialog = obj.clone().downcast::<gtk::Dialog>().unwrap();
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
        let size_label = gtk::LabelBuilder::new()
            .halign(gtk::Align::End)
            .label("Output Size")
            .build();
        let size_spinner = gtk::SpinButtonBuilder::new()
            .adjustment(&self.parent_size)
            .build();

        let list = gtk::ListBoxBuilder::new()
            .height_request(128)
            .selection_mode(gtk::SelectionMode::None)
            .build();

        let new_image_button = gtk::Button::new_with_label("New Image");
        new_image_button.connect_clicked(
            clone!(@strong list, @strong self.exportable as model => move |_| {
                let row = ExportRow::new();
                row.set_exportable_model(&model);
                row.show_all();
                list.insert(&row, -1);
            }),
        );
        let default_outputs = gtk::Button::new_with_label("Create Defaults");

        grid.attach(&directory_label, 0, 0, 1, 1);
        grid.attach(&directory_picker, 1, 0, 1, 1);
        grid.attach(&prefix_label, 0, 1, 1, 1);
        grid.attach(&prefix_entry, 1, 1, 1, 1);
        grid.attach(&size_label, 0, 2, 1, 1);
        grid.attach(&size_spinner, 1, 2, 1, 1);
        grid.attach(&list, 0, 3, 2, 1);
        grid.attach(&new_image_button, 0, 4, 2, 1);
        grid.attach(&default_outputs, 0, 5, 2, 1);

        box_.add(&grid);

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
        export_button.connect_clicked(clone!(@weak dialog, @strong self.parent_size as psize, @strong list, @strong directory_picker, @strong prefix_entry => move |_| {
            let directory = directory_picker.get_filename().unwrap_or_else(|| PathBuf::from("/tmp/"));
            let prefix = prefix_entry.get_text().to_string();

            for child in list.get_children().iter() {
                let list_box_row = child.downcast_ref::<gtk::ListBoxRow>().unwrap().get_child().unwrap();
                let export_row = list_box_row.downcast::<ExportRow>().unwrap();

                let spec_r = export_row.get_channel(ImageChannel::R);
                let spec_g = export_row.get_channel(ImageChannel::G);
                let spec_b = export_row.get_channel(ImageChannel::B);
                let spec_a = export_row.get_channel(ImageChannel::A);

                let filename = export_row.get_filename();
                let mut path = directory.clone();
                path.push(format!("{}_{}.png", prefix, filename));

                match export_row.get_image_type() {
                    Some(ExportImageType::RGBA) => {
                        super::emit(Lang::UserIOEvent(
                            UserIOEvent::ExportImage(
                                ExportSpec::RGBA([
                                    spec_r.unwrap(),
                                    spec_g.unwrap(),
                                    spec_b.unwrap(),
                                    spec_a.unwrap()]),
                                psize.get_value() as u32,
                                path
                            )));
                    }
                    Some(ExportImageType::RGB) => {
                        super::emit(Lang::UserIOEvent(
                            UserIOEvent::ExportImage(
                                ExportSpec::RGB([
                                    spec_r.unwrap(),
                                    spec_g.unwrap(),
                                    spec_b.unwrap()]),
                                psize.get_value() as u32,
                                path
                            )));
                    }
                    Some(ExportImageType::Grayscale) => {
                        super::emit(Lang::UserIOEvent(
                            UserIOEvent::ExportImage(
                                ExportSpec::Grayscale(
                                    spec_r.unwrap()),
                                psize.get_value() as u32,
                                path
                            )));
                    }
                    None => {}
                }
            }
            dialog.response(gtk::ResponseType::Ok);
        }));

        header_bar.pack_start(&cancel_button);
        header_bar.pack_end(&export_button);

        dialog.show_all();
    }
}

impl WidgetImpl for ExportDialogPrivate {}
impl ContainerImpl for ExportDialogPrivate {}
impl BinImpl for ExportDialogPrivate {}
impl WindowImpl for ExportDialogPrivate {}
impl DialogImpl for ExportDialogPrivate {}

impl ExportDialogPrivate {
    fn fill_exportable_store(&self, exportable: &[(Resource, ImageType)]) {
        for (socket, ty) in exportable {
            match ty {
                ImageType::Rgb => {
                    self.exportable.insert_with_values(
                        None,
                        &[0],
                        &[&format!("{}#R", &socket.to_string()[5..])],
                    );
                    self.exportable.insert_with_values(
                        None,
                        &[0],
                        &[&format!("{}#G", &socket.to_string()[5..])],
                    );
                    self.exportable.insert_with_values(
                        None,
                        &[0],
                        &[&format!("{}#B", &socket.to_string()[5..])],
                    );
                }
                ImageType::Grayscale => {
                    self.exportable.insert_with_values(
                        None,
                        &[0],
                        &[&socket.to_string()[5..].to_string()],
                    );
                }
            }
        }
    }
}

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
    pub fn new(exportable: &[(Resource, ImageType)], parent_size: f64) -> Self {
        let dlg = glib::Object::new(Self::static_type(), &[("use-header-bar", &1i32)])
            .expect("Failed to create ExportDialog")
            .downcast::<ExportDialog>()
            .expect("Created ExportDialog is of wrong type");
        let imp = ExportDialogPrivate::from_instance(&dlg);
        imp.fill_exportable_store(exportable);
        imp.parent_size.set_value(parent_size);
        dlg
    }
}
