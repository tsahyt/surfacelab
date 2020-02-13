use super::node::Node;
use super::node_socket::NodeSocket;
use gdk::prelude::*;
use gdk::Rectangle;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use std::collections::HashMap;

use std::cell::RefCell;

#[derive(Debug)]
struct Child {
    south_east: (i32, i32),
    rectangle: Rectangle,
    drag_start: (i32, i32),
    drag_delta: (i32, i32),
}

#[derive(Debug)]
struct Connection {
    source: NodeSocket,
    sink: NodeSocket,
}

enum Action {
    DragChild,
    DragCon,
    Resize,
}

pub struct NodeAreaPrivate {
    nodes: RefCell<HashMap<Node, Child>>,
    connections: Vec<Connection>,
    event_window: RefCell<Option<gdk::Window>>,
    action: Option<Action>,
    node_id_counter: u32,
    drag_start: (f64, f64),
    drag_current: (f64, f64),
}

const RESIZE_RECTANGLE: i32 = 16;

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
    //
    // We use this to override additional methods
    fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {
        unsafe {
            // Extra overrides for container methods
            let container_class =
                &mut *(class as *mut _ as *mut <gtk::Container as ObjectType>::RustClassType);

            // Extra overrides for widget methods
            let widget_class = &mut *(container_class as *mut _
                as *mut <gtk::Widget as ObjectType>::RustClassType);
            {
                let klass =
                    &mut *(widget_class as *mut gtk::WidgetClass as *mut gtk_sys::GtkWidgetClass);
                klass.realize = Some(node_area_widget_realize);
                klass.unrealize = Some(node_area_widget_unrealize);
                klass.map = Some(node_area_widget_map);
                klass.unmap = Some(node_area_widget_unmap);
            }
        }
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            nodes: RefCell::new(HashMap::new()),
            connections: Vec::new(),
            event_window: RefCell::new(None),
            action: None,
            node_id_counter: 0,
            drag_start: (0., 0.),
            drag_current: (0., 0.),
        }
    }
}

impl ObjectImpl for NodeAreaPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let node_area = obj.clone().downcast::<NodeArea>().unwrap();
        node_area.set_has_window(false);
        node_area.set_size_request(100, 100);

        // TODO: DnD
    }
}

impl NodeAreaPrivate {
    fn move_child(&self, widget: &gtk::Widget, x: i32, y: i32) {
        let node = widget
            .clone()
            .downcast::<super::node::Node>()
            .expect("NodeArea can only contain nodes!");
        let mut nodes = self.nodes.borrow_mut();
        let child = nodes.get_mut(&node).expect("Trying to move a non-child");
        // TODO
    }

    fn map(&self, widget: &gtk::Widget) {
        widget.set_mapped(true);

        for child in self.nodes.borrow().keys() {
            if !child.get_visible() {
                continue;
            }
            if !child.get_mapped() {
                child.map();
            }
        }

        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.show();
        }
    }

    fn unmap(&self, _widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.hide();
        }

        // TODO: parent unmap
    }

    fn realize(&self, widget: &gtk::Widget) {
        // set up event window
        widget.set_realized(true);
        let allocation = widget.get_allocation();
        let parent = widget
            .get_parent_window()
            .expect("NodeArea without parent!");
        let window = gdk::Window::new(
            Some(&parent),
            &gdk::WindowAttr {
                window_type: gdk::WindowType::Child,
                wclass: gdk::WindowWindowClass::InputOutput,
                x: Some(allocation.x),
                y: Some(allocation.y),
                width: allocation.width,
                height: allocation.height,
                event_mask: {
                    let mut em = widget.get_events();
                    em.insert(gdk::EventMask::BUTTON_PRESS_MASK);
                    em.insert(gdk::EventMask::BUTTON_RELEASE_MASK);
                    em.insert(gdk::EventMask::POINTER_MOTION_MASK);
                    em.insert(gdk::EventMask::TOUCH_MASK);
                    em.insert(gdk::EventMask::ENTER_NOTIFY_MASK);
                    em.insert(gdk::EventMask::LEAVE_NOTIFY_MASK);
                    em.bits()
                } as _,
                ..gdk::WindowAttr::default()
            },
        );
        widget.register_window(&window);

        // parent children
        for child in self.nodes.borrow().keys() {
            child.set_parent_window(&window);
        }

        dbg!(&window);

        // store event_window
        self.event_window.replace(Some(window));
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            widget.unregister_window(ew);
            ew.destroy();
            self.event_window.replace(None);
        }

        // TODO: parent unrealize
    }

    fn size_allocate(&self, _widget: &gtk::Widget, _allocation: &gtk::Allocation) {}

    fn connecting_curve(cr: &cairo::Context, source: (f64, f64), sink: (f64, f64)) {
        cr.move_to(source.0, source.1);
        let d = (sink.0 - source.0).abs() / 2.0;
        cr.curve_to(source.0 + d, source.1, sink.0 - d, sink.1, sink.0, sink.1);
    }

    fn draw_socket_connection(&self, _widget: &gtk::Widget, cr: &cairo::Context, c: &Connection) {
        // get coordinates
        let start = {
            let parent_alloc = c.source.get_parent().unwrap().get_allocation();
            let alloc = c.source.get_allocation();
            (
                (alloc.x + alloc.width / 2 + parent_alloc.x) as f64,
                (alloc.y + alloc.width / 2 + parent_alloc.y) as f64,
            )
        };

        let end = {
            let parent_alloc = c.sink.get_parent().unwrap().get_allocation();
            let alloc = c.sink.get_allocation();
            (
                (alloc.x + alloc.width / 2 + parent_alloc.x) as f64,
                (alloc.y + alloc.width / 2 + parent_alloc.y) as f64,
            )
        };

        // set up gradient
        // TODO: get color values from sockets
        let gradient = cairo::LinearGradient::new(start.0, start.1, end.0, end.1);
        gradient.add_color_stop_rgba(0., 1., 0., 0., 1.);
        gradient.add_color_stop_rgba(1., 0., 1., 0., 1.);

        // draw
        cr.save();
        Self::connecting_curve(cr, start, end);
        cr.set_source(&gradient);
        cr.stroke();
        cr.restore();
    }
}

impl gtk::subclass::widget::WidgetImpl for NodeAreaPrivate {
    fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> Inhibit {
        if let Some(Action::DragCon) = self.action {
            cr.save();
            cr.set_source_rgba(1., 0.2, 0.2, 0.6);
            Self::connecting_curve(cr, self.drag_start, self.drag_current);
            cr.stroke();
            cr.restore();
        }

        for connection in &self.connections {
            self.draw_socket_connection(widget, cr, connection);
        }

        if gtk::cairo_should_draw_window(cr, self.event_window.borrow().as_ref().unwrap()) {
            use gtk::subclass::widget::WidgetImplExt;
            self.parent_draw(widget, cr);
        }

        Inhibit(false)
    }
}

impl gtk::subclass::container::ContainerImpl for NodeAreaPrivate {
    fn add(&self, container: &gtk::Container, widget: &gtk::Widget) {
        let node = widget
            .clone()
            .downcast::<super::node::Node>()
            .expect("NodeArea can only contain nodes!");

        // Initialize geometry
        let rectangle = Rectangle {
            x: 100,
            y: 100,
            width: 100,
            height: 100,
        };
        self.nodes.borrow_mut().insert(
            node,
            Child {
                south_east: (
                    rectangle.width - RESIZE_RECTANGLE,
                    rectangle.height - RESIZE_RECTANGLE,
                ),
                rectangle,
                drag_start: (0, 0),
                drag_delta: (0, 0),
            },
        );

        // TODO: register signals for child button presses etc

        // Set up parents and show
        if container.get_realized() {
            widget.set_parent_window(self.event_window.borrow().as_ref().unwrap());
        }

        widget.set_parent(container);
        widget.show_all();
    }

    fn remove(&self, _container: &gtk::Container, widget: &gtk::Widget) {
        let node = widget
            .clone()
            .downcast::<super::node::Node>()
            .expect("NodeArea can only contain nodes!");
        self.nodes.borrow_mut().remove(&node);
        widget.unparent();
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
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }
}

// Wrapper functions

unsafe extern "C" fn node_area_widget_realize(ptr: *mut gtk_sys::GtkWidget) {
    let instance = &*(ptr as *mut subclass::simple::InstanceStruct<NodeAreaPrivate>);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.realize(&wrap);
}

unsafe extern "C" fn node_area_widget_unrealize(ptr: *mut gtk_sys::GtkWidget) {
    let instance = &*(ptr as *mut subclass::simple::InstanceStruct<NodeAreaPrivate>);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.unrealize(&wrap);
}

unsafe extern "C" fn node_area_widget_map(ptr: *mut gtk_sys::GtkWidget) {
    let instance = &*(ptr as *mut subclass::simple::InstanceStruct<NodeAreaPrivate>);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.map(&wrap);
}

unsafe extern "C" fn node_area_widget_unmap(ptr: *mut gtk_sys::GtkWidget) {
    let instance = &*(ptr as *mut subclass::simple::InstanceStruct<NodeAreaPrivate>);
    let imp = instance.get_impl();
    let wrap: gtk::Widget = from_glib_borrow(ptr);
    imp.unmap(&wrap);
}

// unsafe extern "C" fn node_area_widget_size_allocate(
//     ptr: *mut gtk_sys::GtkWidget,
//     aptr: *mut gtk_sys::GtkAllocation,
// ) {
//     let instance = &*(ptr as *mut subclass::simple::InstanceStruct<NodeAreaPrivate>);
//     let imp = instance.get_impl();
//     let wrap: gtk::Widget = from_glib_borrow(ptr);
// }
