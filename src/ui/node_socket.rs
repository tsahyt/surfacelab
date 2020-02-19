use super::subclass::*;
use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use std::cell::RefCell;

#[derive(Debug)]
pub enum NodeSocketIO {
    Source,
    Sink,
    Disable,
}

pub struct NodeSocketPrivate {
    rgba: RefCell<(f64, f64, f64, f64)>,
    radius: RefCell<f64>,
    io: RefCell<NodeSocketIO>,
    drop_types: Vec<gtk::TargetEntry>,
    socket_uri: RefCell<std::string::String>,
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodeSocketPrivate {
    const NAME: &'static str = "NodeSocketPrivate";

    type ParentType = gtk::EventBox;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    fn class_init(_class: &mut subclass::simple::ClassStruct<Self>) {}

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            rgba: RefCell::new((0.53, 0.71, 0.3, 1.)),
            radius: RefCell::new(8.0),
            io: RefCell::new(NodeSocketIO::Disable),
            drop_types: vec![gtk::TargetEntry::new(
                "node-socket",
                gtk::TargetFlags::SAME_APP,
                0,
            )],
            socket_uri: RefCell::new("".to_string()),
        }
    }
}

impl gtk::subclass::container::ContainerImpl for NodeSocketPrivate {}

impl gtk::subclass::bin::BinImpl for NodeSocketPrivate {}

impl gtk::subclass::event_box::EventBoxImpl for NodeSocketPrivate {}

impl ObjectImpl for NodeSocketPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let node = obj.clone().downcast::<NodeSocket>().unwrap();
        node.set_has_window(true);
    }
}

impl gtk::subclass::widget::WidgetImpl for NodeSocketPrivate {
    fn get_preferred_width(&self, _widget: &gtk::Widget) -> (i32, i32) {
        let s = *self.radius.borrow() as i32 * 2;
        (s, s)
    }

    fn get_preferred_height(&self, _widget: &gtk::Widget) -> (i32, i32) {
        let s = *self.radius.borrow() as i32 * 2;
        (s, s)
    }

    fn draw(&self, _widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        let r = *self.radius.borrow();
        let (red, green, blue, alpha) = *self.rgba.borrow();

        cr.set_source_rgba(red, green, blue, alpha);
        cr.arc(r, r, r, 0.0, std::f64::consts::TAU);
        cr.fill();

        Inhibit(false)
    }

    fn drag_data_get(
        &self,
        widget: &gtk::Widget,
        _context: &gdk::DragContext,
        selection_data: &gtk::SelectionData,
        _info: u32,
        _time: u32,
    ) {
        log::trace!("Drag data get at {:?}", &widget);
        let uri = self.socket_uri.borrow().clone();
        selection_data.set(&selection_data.get_target(), 8, uri.as_ref());
    }

    fn drag_data_received(
        &self,
        widget: &gtk::Widget,
        _context: &gdk::DragContext,
        _x: i32,
        _y: i32,
        selection_data: &gtk::SelectionData,
        _info: u32,
        _time: u32,
    ) {
        let data = selection_data.get_data();
        let socket = std::str::from_utf8(&data).expect("Invalid drag and drop data!");
        log::trace!("Drag data received at {:?}: {:?}", &widget, socket);
    }

    fn drag_failed(
        &self,
        widget: &gtk::Widget,
        _context: &gdk::DragContext,
        result: gtk::DragResult,
    ) -> gtk::Inhibit {
        log::trace!("Drag failed {:?}: {:?}", &widget, &result);
        Inhibit(true)
    }
}

impl WidgetImplExtra for NodeSocketPrivate {}

impl NodeSocketPrivate {
    fn set_rgba(&self, red: f64, green: f64, blue: f64, alpha: f64) {
        self.rgba.replace((red, green, blue, alpha));
    }

    fn get_rgba(&self) -> (f64, f64, f64, f64) {
        self.rgba.borrow().clone()
    }

    fn set_radius(&self, radius: f64) {
        self.radius.replace(radius);
    }

    fn get_radius(&self) -> f64 {
        self.radius.borrow().clone()
    }

    fn set_io(&self, widget: &gtk::Widget, io: NodeSocketIO) {
        match io {
            NodeSocketIO::Source => {
                widget.drag_source_set(
                    gdk::ModifierType::BUTTON1_MASK,
                    &self.drop_types,
                    gdk::DragAction::COPY,
                );
            }
            NodeSocketIO::Sink => {
                widget.drag_dest_set(
                    gtk::DestDefaults::ALL,
                    &self.drop_types,
                    gdk::DragAction::COPY,
                );
            }
            _ => {}
        }
        self.io.replace(io);
    }

    fn set_socket_uri(&self, widget: &NodeSocket, uri: uriparse::URI) {
        let uris = uri.to_string();
        widget.set_tooltip_text(Some(&uris));
        self.socket_uri.replace(uri.to_string());
    }
}

glib_wrapper! {
    pub struct NodeSocket(
        Object<subclass::simple::InstanceStruct<NodeSocketPrivate>,
        subclass::simple::ClassStruct<NodeSocketPrivate>,
        NodeSocketClass>)
        @extends gtk::Widget, gtk::Container, gtk::Bin, gtk::EventBox;

    match fn {
        get_type => || NodeSocketPrivate::get_type().to_glib(),
    }
}

impl NodeSocket {
    pub fn new() -> Self {
        let socket: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        socket
    }

    pub fn set_rgba(&self, red: f64, green: f64, blue: f64, alpha: f64) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_rgba(red, green, blue, alpha);
    }

    pub fn get_rgba(&self) -> (f64, f64, f64, f64) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.get_rgba()
    }

    pub fn set_radius(&self, radius: f64) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_radius(radius);
    }

    pub fn get_radius(&self) -> f64 {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.get_radius()
    }

    pub fn set_io(&self, io: NodeSocketIO) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_io(&self.clone().upcast::<gtk::Widget>(), io);
    }

    pub fn set_socket_uri(&self, uri: uriparse::URI) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_socket_uri(self, uri);
    }
}
