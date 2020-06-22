use super::{export, node, node_area, renderer};
use crate::lang::*;

use gio::prelude::*;
use gio::subclass::prelude::*;
use gio::ApplicationFlags;
use glib::subclass;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use once_cell::unsync::OnceCell;
use std::sync::Arc;

pub struct SurfaceLabWindowPrivate {
    node_area: node_area::NodeArea,
    header_bar: gtk::HeaderBar,
    document_properties: gtk::Popover,
    parent_size: gtk::Adjustment,
}

impl ObjectSubclass for SurfaceLabWindowPrivate {
    const NAME: &'static str = "SurfaceLabWindow";
    type ParentType = gtk::ApplicationWindow;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            node_area: node_area::NodeArea::new(),
            header_bar: gtk::HeaderBarBuilder::new()
                .show_close_button(true)
                .title("SurfaceLab")
                .subtitle("<unsaved>")
                .build(),
            document_properties: gtk::Popover::new(None::<&gtk::Widget>),
            parent_size: gtk::Adjustment::new(1024.0, 32.0, 16384.0, 32.0, 512.0, 1024.0),
        }
    }
}

impl ObjectImpl for SurfaceLabWindowPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        self.parent_constructed(obj);

        let window = obj.downcast_ref::<gtk::ApplicationWindow>().unwrap();
        window.set_default_size(1920, 1080);

        // Header Bar
        self.header_bar.pack_start(&{
            let new_button = gtk::ButtonBuilder::new()
                .image(&gtk::Image::new_from_icon_name(
                    Some("document-new-symbolic"),
                    gtk::IconSize::Menu,
                ))
                .build();
            new_button.connect_clicked(|_| super::emit(Lang::UserIOEvent(UserIOEvent::NewSurface)));
            new_button
        });
        self.header_bar.pack_start(&{
            let btn_box = gtk::ButtonBoxBuilder::new()
                .layout_style(gtk::ButtonBoxStyle::Expand)
                .homogeneous(false)
                .build();
            let open = gtk::Button::new_with_label("Open");
            open.connect_clicked(clone!(@weak window => move |_| {
                Self::run_open_dialog(&window)
            }));
            let recent = gtk::MenuButtonBuilder::new()
                .image(&gtk::Image::new_from_icon_name(
                    Some("pan-down-symbolic"),
                    gtk::IconSize::Menu,
                ))
                .build();
            btn_box.add(&open);
            btn_box.add(&recent);
            btn_box
        });
        self.header_bar.pack_start(&{
            let btn_box = gtk::ButtonBoxBuilder::new()
                .layout_style(gtk::ButtonBoxStyle::Expand)
                .homogeneous(false)
                .build();
            let save = gtk::Button::new_with_label("Save");
            save.connect_clicked(clone!(@weak window => move |_| {
                Self::run_save_dialog(&window)
            }));
            let recent = gtk::MenuButtonBuilder::new()
                .image(&gtk::Image::new_from_icon_name(
                    Some("pan-down-symbolic"),
                    gtk::IconSize::Menu,
                ))
                .build();
            let export = gtk::Button::new_from_icon_name(
                Some("insert-object-symbolic"),
                gtk::IconSize::Menu,
            );

            export.connect_clicked(|_| {
                super::emit(Lang::UserIOEvent(UserIOEvent::RequestExport(None)));
            });

            btn_box.add(&save);
            btn_box.add(&recent);
            btn_box.add(&export);
            btn_box
        });
        self.header_bar
            .pack_start(&gtk::Separator::new(gtk::Orientation::Vertical));
        self.header_bar.pack_start(&{
            let btn_box = gtk::ButtonBoxBuilder::new()
                .layout_style(gtk::ButtonBoxStyle::Expand)
                .build();
            let undo = gtk::ButtonBuilder::new()
                .image(&gtk::Image::new_from_icon_name(
                    Some("edit-undo-symbolic"),
                    gtk::IconSize::Menu,
                ))
                .build();
            let redo = gtk::ButtonBuilder::new()
                .image(&gtk::Image::new_from_icon_name(
                    Some("edit-redo-symbolic"),
                    gtk::IconSize::Menu,
                ))
                .build();
            btn_box.add(&undo);
            btn_box.add(&redo);
            btn_box
        });

        self.header_bar.pack_end(
            &gtk::MenuButtonBuilder::new()
                .image(&gtk::Image::new_from_icon_name(
                    Some("open-menu-symbolic"),
                    gtk::IconSize::Menu,
                ))
                .build(),
        );
        self.header_bar.pack_end(
            &gtk::MenuButtonBuilder::new()
                .image(&gtk::Image::new_from_icon_name(
                    Some("document-properties-symbolic"),
                    gtk::IconSize::Menu,
                ))
                .popover(&self.document_properties)
                .build(),
        );
        window.set_titlebar(Some(&self.header_bar));

        // Document Properties
        let document_properties_box = gtk::BoxBuilder::new()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin(8)
            .build();
        {
            let parent_size_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            self.parent_size.connect_value_changed(|a| {
                super::emit(Lang::UserIOEvent(UserIOEvent::SetParentSize(
                    a.get_value() as u32
                )));
            });
            parent_size_box.add(&gtk::Label::new(Some("Size")));
            parent_size_box.add(
                &gtk::SpinButtonBuilder::new()
                    .adjustment(&self.parent_size)
                    .build(),
            );
            let quick_sizes = gtk::ButtonBoxBuilder::new()
                .layout_style(gtk::ButtonBoxStyle::Expand)
                .build();
            for (lbl, res) in [("½k", 512), ("1k", 1024), ("2k", 2048), ("4k", 4096)].iter() {
                let btn = gtk::Button::new_with_label(lbl);
                btn.connect_clicked(
                    clone!(@strong self.parent_size as psize => move |_| psize.set_value(*res as f64)),
                );
                quick_sizes.add(&btn);
            }
            parent_size_box.add(&quick_sizes);
            document_properties_box.add(&parent_size_box);
        }
        self.document_properties.add(&document_properties_box);
        self.document_properties.show_all();

        // Main Views
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        hbox.pack_end(&gtk::Label::new(Some("Parameter Section")), false, false, 0);
        hbox.pack_end(
            &gtk::Separator::new(gtk::Orientation::Vertical),
            false,
            false,
            0,
        );
        hbox.pack_start(
            &gtk::Label::new(Some("Library and Explorer type stuff")),
            false,
            false,
            0,
        );
        hbox.pack_start(
            &gtk::Separator::new(gtk::Orientation::Vertical),
            false,
            false,
            0,
        );

        let main_area = {
            let outer_paned = gtk::PanedBuilder::new()
                .orientation(gtk::Orientation::Horizontal)
                .build();
            let inner_paned = gtk::PanedBuilder::new()
                .orientation(gtk::Orientation::Vertical)
                .build();

            let node_area = {
                let vadj = gtk::Adjustment::new(0.0, 0.0, 4096.0, 1.0, 1.0, 1.0);
                let hadj = gtk::Adjustment::new(0.0, 0.0, 4096.0, 1.0, 1.0, 1.0);

                let vp = gtk::Viewport::new(Some(&hadj), Some(&vadj));
                let sw = gtk::ScrolledWindow::new(Some(&hadj), Some(&vadj));
                vp.add(&self.node_area);
                sw.add(&vp);
                sw
            };
            let view_3d = renderer::Renderer3DView::new();
            let view_2d = renderer::Renderer2DView::new();

            inner_paned.pack1(&view_3d, true, true);
            inner_paned.pack2(&view_2d, true, true);
            inner_paned.set_position(640);
            outer_paned.pack1(&node_area, true, true);
            outer_paned.pack2(&inner_paned, true, true);
            outer_paned.set_position(720);

            outer_paned
        };
        hbox.pack_start(&main_area, true, true, 8);

        window.add(&hbox);

        // Quit on Delete Event
        window.connect_delete_event(|_, _| {
            super::emit(Lang::UserIOEvent(UserIOEvent::Quit));
            Inhibit(false)
        });
    }
}

pub fn file_filters() -> Vec<gtk::FileFilter> {
    let surf = gtk::FileFilter::new();
    surf.set_name(Some("SurfaceLab Surface"));
    surf.add_pattern("*.surf");

    vec![surf]
}

impl SurfaceLabWindowPrivate {
    fn run_export_dialog(&self, exportable: &[(Resource, ImageType)]) {
        let dialog = export::ExportDialog::new(exportable);
        let _response = dialog.run();
        dialog.close();
    }

    fn run_open_dialog(window: &gtk::ApplicationWindow) {
        let dialog = gtk::FileChooserDialog::with_buttons(
            Some("Open Surface"),
            Some(window),
            gtk::FileChooserAction::Open,
            &[
                ("_Cancel", gtk::ResponseType::Cancel),
                ("_Open", gtk::ResponseType::Accept),
            ],
        );
        for f in file_filters() {
            dialog.add_filter(&f);
        }

        if let gtk::ResponseType::Accept = dialog.run() {
            if let Some(path) = dialog.get_filename() {
                super::emit(Lang::UserIOEvent(UserIOEvent::OpenSurface(path)));
            }
        }
        dialog.close();
    }

    fn run_save_dialog(window: &gtk::ApplicationWindow) {
        let dialog = gtk::FileChooserDialog::with_buttons(
            Some("Save Surface"),
            Some(window),
            gtk::FileChooserAction::Save,
            &[
                ("_Cancel", gtk::ResponseType::Cancel),
                ("_Save", gtk::ResponseType::Accept),
            ],
        );
        for f in file_filters() {
            dialog.add_filter(&f);
        }

        if let gtk::ResponseType::Accept = dialog.run() {
            if let Some(path) = dialog.get_filename() {
                super::emit(Lang::UserIOEvent(UserIOEvent::SaveSurface(path)));
            }
        }
        dialog.close();
    }
}

impl WidgetImpl for SurfaceLabWindowPrivate {}
impl ContainerImpl for SurfaceLabWindowPrivate {}
impl BinImpl for SurfaceLabWindowPrivate {}
impl WindowImpl for SurfaceLabWindowPrivate {}
impl ApplicationWindowImpl for SurfaceLabWindowPrivate {}

glib_wrapper! {
    pub struct SurfaceLabWindow(
        Object<subclass::simple::InstanceStruct<SurfaceLabWindowPrivate>,
        subclass::simple::ClassStruct<SurfaceLabWindowPrivate>,
        SurfaceLabAppWindowClass>)
        @extends gtk::Widget, gtk::Container, gtk::Bin, gtk::Window, gtk::ApplicationWindow;

    match fn {
        get_type => || SurfaceLabWindowPrivate::get_type().to_glib(),
    }
}

impl SurfaceLabWindow {
    pub fn new(app: &gtk::Application) -> Self {
        glib::Object::new(Self::static_type(), &[("application", app)])
            .expect("Failed to create SurfaceLabWindow")
            .downcast::<SurfaceLabWindow>()
            .expect("Created SurfaceLabWindow is of wrong type")
    }
}

pub struct SurfaceLabApplicationPrivate {
    window: OnceCell<SurfaceLabWindow>,
}

impl ObjectSubclass for SurfaceLabApplicationPrivate {
    const NAME: &'static str = "SurfaceLabApplication";
    type ParentType = gtk::Application;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            window: OnceCell::new(),
        }
    }
}

impl ObjectImpl for SurfaceLabApplicationPrivate {
    glib_object_impl!();
}

// When our application starts, the `startup` signal will be fired.
// This gives us a chance to perform initialisation tasks that are not directly
// related to showing a new window. After this, depending on how
// the application is started, either `activate` or `open` will be called next.
impl ApplicationImpl for SurfaceLabApplicationPrivate {
    // Gets called when the application is launched by the desktop environment and
    // asked to present itself.
    fn activate(&self, _app: &gio::Application) {
        let window = self
            .window
            .get()
            .expect("Should always be initialized in gio_application_startup");
        window.show_all();
        window.present();
    }

    // `gio::Application` is bit special. It does not get initialized
    // when `new` is called and the object created, but rather
    // once the `startup` signal is emitted and the `gio::Application::startup`
    // is called.
    //
    // Due to this, we create and initialize the `SurfaceLabWindow` widget
    // here. Widgets can't be created before `startup` has been called.
    fn startup(&self, app: &gio::Application) {
        self.parent_startup(app);

        let app = app.downcast_ref::<gtk::Application>().unwrap();
        let window = SurfaceLabWindow::new(&app);
        self.window
            .set(window)
            .expect("Failed to initialize application window");
    }
}

impl GtkApplicationImpl for SurfaceLabApplicationPrivate {}

glib_wrapper! {
    pub struct SurfaceLabApplication(
        Object<subclass::simple::InstanceStruct<SurfaceLabApplicationPrivate>,
        subclass::simple::ClassStruct<SurfaceLabApplicationPrivate>,
        SurfaceLabApplicationClass>)
        @extends gio::Application, gtk::Application;

    match fn {
        get_type => || SurfaceLabApplicationPrivate::get_type().to_glib(),
    }
}

impl SurfaceLabApplication {
    pub fn new() -> Self {
        glib::Object::new(
            Self::static_type(),
            &[
                ("application-id", &"com.mechaneia.surfacelab"),
                ("flags", &ApplicationFlags::empty()),
            ],
        )
        .expect("Failed to create SurfaceLabApplication")
        .downcast()
        .expect("Created application is of wrong type")
    }

    fn get_app_window(&self) -> &SurfaceLabWindowPrivate {
        let imp = SurfaceLabApplicationPrivate::from_instance(self);
        let win = imp
            .window
            .get()
            .expect("Failed to obtain Application Window");
        SurfaceLabWindowPrivate::from_instance(win)
    }

    pub fn process_event(&self, event: Arc<Lang>) {
        let app_window = self.get_app_window();
        match &*event {
            Lang::GraphEvent(GraphEvent::NodeAdded(res, op, pos)) => {
                let new_node = node::Node::new_from_operator(op.clone(), res.clone());
                app_window
                    .node_area
                    .add_at(&new_node, pos.unwrap_or_default());
                new_node.show_all();
            }
            Lang::GraphEvent(GraphEvent::NodeRemoved(res)) => {
                app_window.node_area.remove_by_resource(&res)
            }
            Lang::GraphEvent(GraphEvent::NodeRenamed(from, to)) => {
                app_window.node_area.rename_node(from, to)
            }
            Lang::GraphEvent(GraphEvent::ConnectedSockets(source, sink)) => {
                app_window
                    .node_area
                    .add_connection(source.clone(), sink.clone());
            }
            Lang::GraphEvent(GraphEvent::DisconnectedSockets(source, sink)) => {
                app_window
                    .node_area
                    .remove_connection(source.clone(), sink.clone());
            }
            Lang::GraphEvent(GraphEvent::Cleared) => {
                app_window.node_area.clear();
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailGenerated(res, thumb)) => {
                app_window.node_area.update_thumbnail(res, thumb);
            }
            Lang::UserIOEvent(UserIOEvent::RequestExport(Some(exp))) => {
                app_window.run_export_dialog(&exp);
            }
            _ => {}
        }
    }
}

impl Default for SurfaceLabApplication {
    fn default() -> Self {
        Self::new()
    }
}
