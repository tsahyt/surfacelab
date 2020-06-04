use crate::lang;

use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use std::cell::RefCell;
use std::convert::TryFrom;

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
    socket_resource: RefCell<lang::Resource>,
}

// Signals
pub const SOCKET_CONNECTED: &str = "socket-connected";

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodeSocketPrivate {
    const NAME: &'static str = "NodeSocket";

    type ParentType = gtk::EventBox;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {
        class.add_signal(
            SOCKET_CONNECTED,
            glib::SignalFlags::empty(),
            &[glib::types::Type::String],
            glib::types::Type::Unit,
        );
    }

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
            socket_resource: RefCell::new(lang::Resource::unregistered_node()),
        }
    }
}

impl gtk::subclass::container::ContainerImpl for NodeSocketPrivate {}

impl gtk::subclass::bin::BinImpl for NodeSocketPrivate {}

impl gtk::subclass::event_box::EventBoxImpl for NodeSocketPrivate {}

impl ObjectImpl for NodeSocketPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let node = obj.downcast_ref::<NodeSocket>().unwrap();
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
        let resource = self.socket_resource.borrow().clone();
        selection_data.set(
            &selection_data.get_target(),
            8,
            resource.to_string().as_ref(),
        );
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
        widget
            .emit(SOCKET_CONNECTED, &[&Value::from(socket)])
            .unwrap();

        // TODO: Become a drag source, for disconnects
        widget.drag_source_set(
            gdk::ModifierType::BUTTON1_MASK,
            &self.drop_types,
            gdk::DragAction::COPY,
        );
    }

    fn drag_failed(
        &self,
        widget: &gtk::Widget,
        _context: &gdk::DragContext,
        result: gtk::DragResult,
    ) -> gtk::Inhibit {
        Inhibit(true)
    }
}

impl NodeSocketPrivate {
    fn set_rgba(&self, red: f64, green: f64, blue: f64, alpha: f64) {
        self.rgba.replace((red, green, blue, alpha));
    }

    fn get_rgba(&self) -> (f64, f64, f64, f64) {
        *self.rgba.borrow()
    }

    fn set_radius(&self, radius: f64) {
        self.radius.replace(radius);
    }

    fn get_radius(&self) -> f64 {
        *self.radius.borrow()
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

    fn set_socket_resource(&self, widget: &NodeSocket, resource: lang::Resource) {
        let rs = resource.to_string();
        widget.set_tooltip_text(Some(&rs));
        self.socket_resource.replace(resource);
    }

    fn get_socket_uri(&self) -> lang::Resource {
        self.socket_resource.borrow().to_owned()
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

    pub fn get_center(&self) -> (i32, i32) {
        let alloc = self.get_allocation();
        let rad = self.get_radius();
        (alloc.x + rad as i32, alloc.y + rad as i32)
    }

    pub fn set_io(&self, io: NodeSocketIO) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_io(&self.clone().upcast::<gtk::Widget>(), io);
    }

    pub fn set_socket_resource(&self, resource: lang::Resource) {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.set_socket_resource(self, resource);
    }

    pub fn get_socket_resource(&self) -> lang::Resource {
        let imp = NodeSocketPrivate::from_instance(self);
        imp.get_socket_uri()
    }

    pub fn connect_socket_connected<F: Fn(&Self, lang::Resource, lang::Resource) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        let local_resource = self.get_socket_resource();

        self.connect_local(SOCKET_CONNECTED, true, move |w| {
            let node_socket = w[0]
                .clone()
                .downcast::<NodeSocket>()
                .unwrap()
                .get()
                .unwrap();
            let foreign_resource = lang::Resource::try_from(
                w[1].get::<std::string::String>().unwrap().unwrap().as_ref(),
            )
            .unwrap();

            f(&node_socket, local_resource.clone(), foreign_resource);
            None
        })
        .unwrap()
    }
}

impl Default for NodeSocket {
    fn default() -> Self {
        Self::new()
    }
}
