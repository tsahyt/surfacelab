use super::node_area;
use crate::lang::*;

use gio::prelude::*;
use gtk::prelude::*;

use gio::subclass::application::ApplicationImplExt;
use gio::ApplicationFlags;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::subclass::prelude::*;

use once_cell::unsync::OnceCell;
use std::cell::Cell;

#[derive(Debug)]
struct WindowWidgets {
}

// This is the private part of our `SurfaceLabWindow` object.
// Its where state and widgets are stored when they don't
// need to be publicly accesible.
#[derive(Debug)]
pub struct SurfaceLabWindowPrivate {
    widgets: OnceCell<WindowWidgets>,
    counter: Cell<u64>,
}

impl ObjectSubclass for SurfaceLabWindowPrivate {
    const NAME: &'static str = "SurfaceLabWindowPrivate";
    type ParentType = gtk::ApplicationWindow;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            widgets: OnceCell::new(),
            counter: Cell::new(0),
        }
    }
}

impl ObjectImpl for SurfaceLabWindowPrivate {
    glib_object_impl!();

    // Here we are overriding the glib::Objcet::contructed
    // method. Its what gets called when we create our Object
    // and where we can initialize things.
    fn constructed(&self, obj: &glib::Object) {
        self.parent_constructed(obj);
        let window = obj.downcast_ref::<SurfaceLabWindow>().unwrap();
        window.set_title("SurfaceLab");
        window.set_default_size(1024, 768);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 16);

        // Node Area
        let node_area = node_area::NodeArea::new();

        // Buttons
        let button_box = {
            let button_box = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
            button_box.set_layout(gtk::ButtonBoxStyle::Expand);
            let new_image_node_button = gtk::Button::new_with_label("New Image Node");
            // new_image_node_button.connect_clicked(clone!(@weak bus_rc => move |_| {
            //     bus::emit(
            //         &*bus_rc,
            //         Lang::UserNodeEvent(UserNodeEvent::NewNode(
            //             Operator::Image {
            //                 path: std::path::PathBuf::from(""),
            //             },
            //         )),
            //     )
            // }));
            button_box.add(&new_image_node_button);

            // new_image_node_button.connect_clicked(clone!(node_area => move |_| {
            //     let new_node = node::Node::new();
            //     new_node.add_socket(lang::Resource::try_from("node:/foo:socket_in").unwrap(), node_socket::NodeSocketIO::Sink);
            //     new_node.add_socket(lang::Resource::try_from("node:/foo:socket_out").unwrap(), node_socket::NodeSocketIO::Source);
            //     node_area.add(&new_node);
            //     new_node.show_all();
            // }));

            button_box
        };

        vbox.add(&button_box);
        vbox.pack_end(&node_area, true, true, 0);

        window.add(&vbox);
    }
}

impl SurfaceLabWindowPrivate {
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

#[derive(Debug)]
pub struct SurfaceLabApplicationPrivate {
    window: OnceCell<SurfaceLabWindow>,
}

impl ObjectSubclass for SurfaceLabApplicationPrivate {
    const NAME: &'static str = "SurfaceLabApplicationPrivate";
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
}
