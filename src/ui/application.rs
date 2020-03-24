use super::{node, node_area, render_area, render_events};
use crate::lang::*;

use gio::prelude::*;
use gio::subclass::application::ApplicationImplExt;
use gio::ApplicationFlags;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use once_cell::unsync::OnceCell;
use std::sync::Arc;

#[derive(Debug)]
struct WindowWidgets {
    node_area: node_area::NodeArea,
}

pub struct SurfaceLabWindowPrivate {
    widgets: OnceCell<WindowWidgets>,
}

impl ObjectSubclass for SurfaceLabWindowPrivate {
    const NAME: &'static str = "SurfaceLabWindow";
    type ParentType = gtk::ApplicationWindow;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            widgets: OnceCell::new(),
        }
    }
}

impl ObjectImpl for SurfaceLabWindowPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        self.parent_constructed(obj);
        let window = obj.downcast_ref::<SurfaceLabWindow>().unwrap();
        window.set_title("SurfaceLab");
        window.set_default_size(1024, 768);

        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 16);

        // Node Area
        let node_area = node_area::NodeArea::new();
        node_area.set_size_request(512, 512);

        // Buttons
        let button_box = {
            let button_box = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
            button_box.set_layout(gtk::ButtonBoxStyle::Expand);

            for op in Operator::all_default() {
                let new_button =
                    gtk::Button::new_with_label(format!("New {} Node", op.title()).as_ref());
                new_button.connect_clicked(move |_| {
                    super::emit(Lang::UserNodeEvent(UserNodeEvent::NewNode(op.clone())))
                });
                button_box.add(&new_button);
            }

            let force_button = gtk::Button::new_with_label("Force Recompute");
            force_button.connect_clicked(move |_| {
                super::emit(Lang::UserNodeEvent(UserNodeEvent::ForceRecompute));
            });
            button_box.add(&force_button);

            button_box
        };

        vbox.add(&button_box);

        // Test Render Area
        let render_area = render_area::RenderArea::new(RendererType::Renderer3D);
        let render_events = render_events::RenderEvents::new(render_area);
        paned.add1(&node_area);
        paned.add2(&render_events);
        vbox.pack_end(&paned, true, true, 0);

        window.add(&vbox);
        window.connect_delete_event(|_, _| {
            super::emit(Lang::UserEvent(UserEvent::Quit));
            Inhibit(false)
        });

        self.widgets
            .set(WindowWidgets { node_area })
            .expect("Failed to initialze widgets");
    }
}

impl SurfaceLabWindowPrivate {}

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

    fn get_widgets(&self) -> &WindowWidgets {
        let imp = SurfaceLabApplicationPrivate::from_instance(self);
        let win = imp
            .window
            .get()
            .expect("Failed to obtain Application Window");
        let winimp = SurfaceLabWindowPrivate::from_instance(win);
        winimp.widgets.get().expect("Failed to obtain widgets")
    }

    pub fn process_event(&self, event: Arc<Lang>) {
        let widgets = self.get_widgets();
        match &*event {
            Lang::GraphEvent(GraphEvent::NodeAdded(res, op)) => {
                let new_node = node::Node::new_from_operator(op.clone(), res.clone());
                widgets.node_area.add(&new_node);
                new_node.show_all();
            }
            Lang::GraphEvent(GraphEvent::NodeRemoved(res)) => {
                widgets.node_area.remove_by_resource(&res)
            }
            Lang::GraphEvent(GraphEvent::ConnectedSockets(source, sink)) => {
                widgets
                    .node_area
                    .add_connection(source.clone(), sink.clone());
            }
            Lang::GraphEvent(GraphEvent::DisconnectedSockets(source, sink)) => {
                widgets
                    .node_area
                    .remove_connection(source.clone(), sink.clone());
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
