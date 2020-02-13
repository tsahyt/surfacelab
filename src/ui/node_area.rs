use super::node::Node;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use std::collections::HashMap;

use std::cell::RefCell;

#[derive(Debug)]
struct NodeAreaChild {
    south_east: (i32, i32),
    rectangle: (i32, i32),
    drag_start: (i32, i32),
    drag_delta: (i32, i32)
}

pub struct NodeAreaPrivate {
    nodes: RefCell<HashMap<Node, NodeAreaChild>>,
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodeAreaPrivate {
    const NAME: &'static str = "NodeAreaPrivate";

    type ParentType = gtk::Container;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    // fn class_init(_class: &mut subclass::simple::ClassStruct<Self>) {}

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            nodes: RefCell::new(HashMap::new()),
        }
    }
}

impl ObjectImpl for NodeAreaPrivate {
    glib_object_impl!();
}

impl NodeAreaPrivate {
    pub fn move_child(&self, widget: &gtk::Widget, x: i32, y: i32) {
        let node = widget
            .clone()
            .downcast::<super::node::Node>()
            .expect("NodeArea can only contain nodes!");
        let mut nodes = self.nodes.borrow_mut();
        let child = nodes.get_mut(&node).expect("Trying to move a non-child");
        // TODO
    }
}

impl gtk::subclass::widget::WidgetImpl for NodeAreaPrivate {
    fn get_preferred_width(&self, _widget: &gtk::Widget) -> (i32, i32) {
        (60, 60)
    }

    fn get_preferred_height(&self, _widget: &gtk::Widget) -> (i32, i32) {
        (60, 60)
    }

    fn draw(&self, _widget: &gtk::Widget, cr: &cairo::Context) -> Inhibit {
        Inhibit(false)
    }
}

impl gtk::subclass::container::ContainerImpl for NodeAreaPrivate {
    fn add(&self, _container: &gtk::Container, widget: &gtk::Widget) {
        let node = widget
            .clone()
            .downcast::<super::node::Node>()
            .expect("NodeArea can only contain nodes!");
        // self.nodes.borrow_mut().insert(
        //     node,
        //     NodeAreaChild {
        //         x: 0,
        //         y: 0,
        //         width: 32,
        //         height: 32,
        //     },
        // );
    }

    fn remove(&self, _container: &gtk::Container, widget: &gtk::Widget) {
        let node = widget
            .clone()
            .downcast::<super::node::Node>()
            .expect("NodeArea can only contain nodes!");
        self.nodes.borrow_mut().remove(&node);
    }
}

glib_wrapper! {
    pub struct NodeArea(
        Object<subclass::simple::InstanceStruct<NodeAreaPrivate>,
        subclass::simple::ClassStruct<NodeAreaPrivate>,
        NodeAreaClass>)
        @extends gtk::Widget, gtk::Container;

    match fn {
        get_type => || NodeAreaPrivate::get_type().to_glib(),
    }
}

impl NodeArea {
    pub fn new() -> Self {
        let na: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        na.set_has_window(false);
        na
    }
}
