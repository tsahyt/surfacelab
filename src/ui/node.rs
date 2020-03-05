use super::node_socket::{NodeSocket, NodeSocketIO};
use super::param_box;
use super::subclass::*;
use crate::lang::*;

use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use once_cell::unsync::OnceCell;
use std::cell::RefCell;
use std::rc::Rc;

pub struct NodePrivate {
    sockets: RefCell<Vec<NodeSocket>>,
    header_label: gtk::Label,
    resource: OnceCell<Resource>,
    popover: Rc<gtk::Popover>,
}

const HEADER_SPACING: i32 = 16;
const MARGIN: i32 = 8;

// Signals
pub const HEADER_BUTTON_PRESS: &str = "header-button-press";
pub const HEADER_BUTTON_RELEASE: &str = "header-button-release";
pub const CLOSE_CLICKED: &str = "close-clicked";

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodePrivate {
    const NAME: &'static str = "Node";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {
        class.add_signal(
            HEADER_BUTTON_PRESS,
            glib::SignalFlags::empty(),
            &[glib::types::Type::F64, glib::types::Type::F64],
            glib::types::Type::Unit,
        );
        class.add_signal(
            HEADER_BUTTON_RELEASE,
            glib::SignalFlags::empty(),
            &[],
            glib::types::Type::Unit,
        );
        class.add_signal(
            CLOSE_CLICKED,
            glib::SignalFlags::empty(),
            &[],
            glib::types::Type::Unit,
        );
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            sockets: RefCell::new(Vec::new()),
            header_label: gtk::Label::new(Some("Node")),
            resource: OnceCell::new(),
            popover: Rc::new(gtk::Popover::new::<gtk::Widget>(None)),
        }
    }
}

impl ObjectImpl for NodePrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let node = obj.clone().downcast::<Node>().unwrap();

        node.set_has_window(false);

        // Set up the Box children
        node.clone()
            .upcast::<gtk::Box>()
            .set_orientation(gtk::Orientation::Vertical);
        node.set_spacing(8);

        // header
        {
            let header_box = gtk::Box::new(gtk::Orientation::Horizontal, HEADER_SPACING);
            let header_evbox = gtk::EventBox::new();
            self.header_label.set_halign(gtk::Align::Start);

            header_evbox.connect_button_press_event(clone!(@strong node => move |_, m| {
                let pos = m.get_position();
                node.emit(HEADER_BUTTON_PRESS, &[&pos.0, &pos.1]).unwrap();
                Inhibit(false)
            }));
            header_evbox.connect_button_release_event(clone!(@strong node => move |_, _| {
                node.emit(HEADER_BUTTON_RELEASE, &[]).unwrap();
                Inhibit(false)
            }));
            header_evbox.add(&self.header_label);
            header_box.pack_start(&header_evbox, false, false, 0);

            let close_image = gtk::Image::new_from_icon_name(
                Some("window-close-symbolic"),
                gtk::IconSize::Button,
            );
            let close_evbox = gtk::EventBox::new();
            close_evbox.connect_button_release_event(clone!(@strong node => move |_, _| {
                node.emit(CLOSE_CLICKED, &[]).unwrap();
                Inhibit(false)
            }));
            close_evbox.add(&close_image);
            header_box.pack_end(&close_evbox, false, false, 0);

            header_box.set_margin_start(MARGIN);
            header_box.set_margin_end(MARGIN);
            header_box.set_margin_top(MARGIN);
            node.add(&header_box);
        }

        // thumbnail
        {
            let thumbnail = gtk::DrawingArea::new();
            thumbnail.set_size_request(128, 128);

            thumbnail.connect_draw(|_, cr| {
                cr.set_source_rgba(0., 0., 0., 1.);
                cr.rectangle(0., 0., 128., 128.);
                cr.fill();
                Inhibit(false)
            });

            thumbnail.set_margin_start(MARGIN);
            thumbnail.set_margin_end(MARGIN);
            thumbnail.set_margin_bottom(MARGIN);

            self.popover.set_relative_to(Some(&thumbnail));
            self.popover.set_position(gtk::PositionType::Right);

            let ebox = gtk::EventBox::new();
            ebox.add(&thumbnail);

            let popover = self.popover.clone();
            ebox.connect_button_press_event(move |_, e| {
                if e.get_button() == (gdk_sys::GDK_BUTTON_SECONDARY as _) {
                    popover.show_all();
                    popover.popup();
                };
                Inhibit(false)
            });

            node.add(&ebox);
        }
    }
}

impl gtk::subclass::widget::WidgetImpl for NodePrivate {
    fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        use gtk::subclass::widget::WidgetImplExt;

        let allocation = widget.get_allocation();
        self.draw_frame(cr, allocation.width, allocation.height);
        self.parent_draw(widget, cr);

        Inhibit(false)
    }
}

impl WidgetImplExtra for NodePrivate {}

impl gtk::subclass::container::ContainerImpl for NodePrivate {}

impl gtk::subclass::box_::BoxImpl for NodePrivate {}

impl NodePrivate {
    fn get_style_node() -> gtk::StyleContext {
        let b = gtk::Button::new();
        b.get_style_context()
    }

    fn draw_frame(&self, cr: &cairo::Context, width: i32, height: i32) {
        let style_context = Self::get_style_node();
        style_context.save();
        gtk::render_background(&style_context, cr, 0., 0., width as _, height as _);
        gtk::render_frame(&style_context, cr, 0., 0., width as _, height as _);
        style_context.restore();
    }

    fn add_socket(&self, node: &Node, resource: Resource, io: NodeSocketIO) {
        let node_socket = NodeSocket::new();

        match io {
            NodeSocketIO::Source => {
                node_socket.set_rgba(0.3, 0.2, 0.5, 1.0);
                node_socket.set_halign(gtk::Align::End);
            }
            NodeSocketIO::Sink => {
                node_socket.set_rgba(0.3, 0.7, 0.3, 1.0);
                node_socket.set_halign(gtk::Align::Start);
            }
            _ => {}
        }
        node_socket.set_io(io);
        node_socket.set_socket_resource(resource);
        node_socket.connect_socket_connected(|_, to, from| {
            super::emit(Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(from, to)))
        });
        node.add(&node_socket);
        self.sockets.borrow_mut().push(node_socket);
    }
}

glib_wrapper! {
    pub struct Node(
        Object<subclass::simple::InstanceStruct<NodePrivate>,
        subclass::simple::ClassStruct<NodePrivate>,
        NodeClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

    match fn {
        get_type => || NodePrivate::get_type().to_glib(),
    }
}

impl Node {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }

    pub fn new_from_operator(op: Operator, resource: Resource) -> Self {
        let node = Self::new();
        let priv_ = NodePrivate::from_instance(&node);
        priv_.header_label.set_label(op.title());
        priv_.popover.add(&param_box::param_box_for_operator(&op, &resource));
        priv_
            .resource
            .set(resource.clone())
            .expect("Failed to set resource for new node");

        for (input, _) in op.inputs().iter() {
            let res = resource.extend_fragment(input);
            node.add_socket(res, NodeSocketIO::Sink);
        }

        for (output, _) in op.outputs().iter() {
            let res = resource.extend_fragment(output);
            node.add_socket(res, NodeSocketIO::Source);
        }

        node
    }

    pub fn get_resource(&self) -> Option<&Resource> {
        let priv_ = NodePrivate::from_instance(self);
        priv_.resource.get()
    }

    pub fn get_socket(&self, resource: &Resource) -> Option<NodeSocket> {
        let priv_ = NodePrivate::from_instance(self);
        priv_
            .sockets
            .borrow()
            .iter()
            .find(|socket| socket.get_socket_resource() == *resource)
            .cloned()
    }

    pub fn add_socket(&self, resource: Resource, io: NodeSocketIO) {
        let imp = NodePrivate::from_instance(self);
        imp.add_socket(self, resource, io);
    }

    pub fn connect_header_button_press_event<F: Fn(&Self, f64, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local(HEADER_BUTTON_PRESS, true, move |w| {
            let node = w[0].clone().downcast::<Node>().unwrap().get().unwrap();
            let x: f64 = w[1].get_some().unwrap();
            let y: f64 = w[2].get_some().unwrap();
            f(&node, x, y);
            None
        })
        .unwrap()
    }

    pub fn connect_header_button_release_event<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local(HEADER_BUTTON_RELEASE, true, move |w| {
            let node = w[0].clone().downcast::<Node>().unwrap().get().unwrap();
            f(&node);
            None
        })
        .unwrap()
    }

    pub fn connect_close_clicked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local(CLOSE_CLICKED, true, move |w| {
            let node = w[0].clone().downcast::<Node>().unwrap().get().unwrap();
            f(&node);
            None
        })
        .unwrap()
    }
}

impl Default for Node {
    fn default() -> Self {
        Self::new()
    }
}
