use super::node::Node;
use super::node_socket;
use crate::lang::*;

use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug)]
enum Action {
    DragChild(i32, i32),
}

#[derive(Debug)]
struct Connection {
    source: node_socket::NodeSocket,
    sink: node_socket::NodeSocket,
}

#[derive(Debug)]
pub struct NodeAreaPrivate {
    children: Rc<RefCell<HashMap<Resource, Node>>>,
    connections: RefCell<Vec<Connection>>,
    action: Rc<RefCell<Option<Action>>>,
    popover_context: gtk::Popover,
}

/// Snapping values to multiples
trait Snap {
    fn snap(self, multiple: Self) -> Self;
}

impl Snap for i32 {
    fn snap(self, multiple: Self) -> Self {
        (self / multiple) * multiple
    }
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodeAreaPrivate {
    const NAME: &'static str = "NodeArea";

    type ParentType = gtk::Fixed;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    //
    // We use this to override additional methods
    fn class_init(_class: &mut subclass::simple::ClassStruct<Self>) {}

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            children: Rc::new(RefCell::new(HashMap::new())),
            connections: RefCell::new(Vec::new()),
            action: Rc::new(RefCell::new(None)),
            popover_context: gtk::PopoverBuilder::new()
                .modal(true)
                .position(gtk::PositionType::Bottom)
                .build(),
        }
    }
}

impl ObjectImpl for NodeAreaPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let node_area = obj.clone().downcast::<NodeArea>().unwrap();
        node_area.set_has_window(true);

        node_area.drag_dest_set(gtk::DestDefaults::MOTION, &[], gdk::DragAction::PRIVATE);
        node_area.drag_dest_set_track_motion(true);

        // Context Popover
        let lbox = gtk::ListBox::new();
        for (i, op) in Operator::all_default().iter().enumerate() {
            let row = gtk::ListBoxRow::new();
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let button =
                gtk::Button::new_from_icon_name(Some("list-add-symbolic"), gtk::IconSize::Menu);

            hbox.add(&gtk::Label::new(Some(op.title())));
            hbox.add(&button);
            row.add(&hbox);

            button.connect_clicked(clone!(@strong op => move |_| {
                super::emit(Lang::UserNodeEvent(UserNodeEvent::NewNode(op.clone())))
            }));
            lbox.insert(&row, i as _);
        }
        self.popover_context.add(&lbox);
    }
}

impl NodeAreaPrivate {
    fn connecting_curve(cr: &cairo::Context, source: (f64, f64), sink: (f64, f64)) {
        cr.move_to(source.0, source.1);
        let d = (sink.0 - source.0).abs() / 2.0;
        cr.curve_to(source.0 + d, source.1, sink.0 - d, sink.1, sink.0, sink.1);
    }

    fn child_connect(&self, container: &gtk::Fixed, widget: &Node, resource: &Resource) {
        // Connect to child signals
        let action = self.action.clone();
        let widget_u = widget.clone().upcast::<gtk::Widget>();

        widget.connect_header_button_press_event(clone!(@strong action => move |w, x, y| {
            let alloc = w.get_allocation();
            action.replace(Some(Action::DragChild(x as i32 - alloc.x, y as i32 - alloc.y)));
        }));

        widget.connect_header_button_release_event(clone!(@strong action => move |_| {
            action.replace(None);
        }));

        widget.connect_motion_notify_event(
            clone!(@strong action, @strong widget_u, @strong container => move |w, motion| {
                if let Some(Action::DragChild(offset_x, offset_y)) = action.borrow().as_ref() {
                    let pos = motion.get_root();

                    let new_x = (pos.0 as i32 - offset_x).snap(32);
                    let new_y = (pos.1 as i32 - offset_y).snap(32);

                    container.move_(&widget_u, new_x, new_y);

                    if w.get_visible() {
                        w.queue_resize();
                    }

                    container.queue_draw();
                }
                Inhibit(false)
            }),
        );

        widget.connect_close_clicked(clone!(@strong resource => move |_| {
            super::emit(Lang::UserNodeEvent(UserNodeEvent::RemoveNode(resource.to_owned())));
        }));
    }

    fn add_connection(&self, source: Resource, sink: Resource) -> Option<()> {
        let source_socket = self
            .children
            .borrow()
            .get(&source.drop_fragment())?
            .get_socket(&source)?;
        let sink_socket = self
            .children
            .borrow()
            .get(&sink.drop_fragment())?
            .get_socket(&sink)?;

        self.connections.borrow_mut().push(Connection {
            source: source_socket,
            sink: sink_socket,
        });

        Some(())
    }

    fn remove_connection(&self, source: Resource, sink: Resource) -> Option<()> {
        let source_socket = self
            .children
            .borrow()
            .get(&source.drop_fragment())?
            .get_socket(&source)?;
        let sink_socket = self
            .children
            .borrow()
            .get(&sink.drop_fragment())?
            .get_socket(&sink)?;

        {
            let mut conns = self.connections.borrow_mut();
            if let Some((idx, _)) = conns
                .iter()
                .enumerate()
                .find(|(_, c)| c.source == source_socket && c.sink == sink_socket)
            {
                conns.remove(idx);
            }
        }

        Some(())
    }

    fn remove_by_resource(&self, container: &gtk::Container, node: &Resource) {
        let lookup = self.children.borrow().get(node).cloned();

        match lookup {
            Some(widget) => container.remove(&widget),
            _ => log::error!("Tried to remove non-existent widget from NodeArea"),
        }
    }

    fn update_thumbnail(&self, node: &Resource, thumbnail: &[u8]) {
        if let Some(child) = self.children.borrow().get(node) {
            child.set_thumbnail(thumbnail);
        }
    }
}

impl WidgetImpl for NodeAreaPrivate {
    fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        for connection in self.connections.borrow().iter() {
            if !connection.source.get_visible() || !connection.sink.get_visible() {
                continue;
            }

            let source = {
                let alloc = connection.source.get_allocation();
                let radius = connection.source.get_radius();
                (alloc.x as f64 + radius, alloc.y as f64 + radius)
            };
            let sink = {
                let alloc = connection.sink.get_allocation();
                let radius = connection.sink.get_radius();
                (alloc.x as f64 + radius, alloc.y as f64 + radius)
            };
            Self::connecting_curve(cr, source, sink);
            cr.stroke();
        }

        self.parent_draw(widget, cr);
        Inhibit(false)
    }

    fn button_press_event(&self, widget: &gtk::Widget, event: &gdk::EventButton) -> gtk::Inhibit {
        if event.get_button() == 3 {
            let (x, y) = event.get_position();
            self.popover_context.set_pointing_to(&gdk::Rectangle {
                x: x as _,
                y: y as _,
                width: 1,
                height: 1,
            });
            self.popover_context.set_relative_to(Some(widget));
            self.popover_context.show_all();
            self.popover_context.popup();
        }

        Inhibit(false)
    }
}

impl ContainerImpl for NodeAreaPrivate {
    // Node Areas contain nodes and nothing else
    fn child_type(&self, _container: &gtk::Container) -> glib::Type {
        Node::static_type()
    }

    fn add(&self, container: &gtk::Container, widget: &gtk::Widget) {
        use gtk::subclass::container::*;
        debug_assert!(widget.get_type() == Node::static_type());

        let node = widget.clone().downcast::<Node>().unwrap();
        let fixed = container.clone().downcast::<gtk::Fixed>().unwrap();
        let resource = node
            .get_resource()
            .expect("Failed adding uninitialized node resource");
        self.child_connect(&fixed, &node, &resource);
        self.children.borrow_mut().insert(resource.clone(), node);

        self.parent_add(container, widget);
    }

    fn remove(&self, container: &gtk::Container, widget: &gtk::Widget) {
        use gtk::subclass::container::*;

        let node = widget.clone().downcast::<Node>().unwrap();
        let resource = node
            .get_resource()
            .expect("Failed removing uninitialized node resource");
        self.children.borrow_mut().remove(resource);

        self.parent_remove(container, widget);
    }
}

impl FixedImpl for NodeAreaPrivate {}

glib_wrapper! {
    pub struct NodeArea(
        Object<subclass::simple::InstanceStruct<NodeAreaPrivate>,
        subclass::simple::ClassStruct<NodeAreaPrivate>,
        NodeAreaClass>)
        @extends gtk::Widget, gtk::Container, gtk::Fixed;

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

    pub fn add_connection(&self, source: Resource, sink: Resource) {
        let imp = NodeAreaPrivate::from_instance(self);
        imp.add_connection(source, sink).unwrap();
        self.queue_draw();
    }

    pub fn remove_connection(&self, source: Resource, sink: Resource) {
        let imp = NodeAreaPrivate::from_instance(self);
        imp.remove_connection(source, sink).unwrap();
        self.queue_draw();
    }

    pub fn remove_by_resource(&self, node: &Resource) {
        let imp = NodeAreaPrivate::from_instance(self);
        imp.remove_by_resource(&self.clone().upcast::<gtk::Container>(), node);
    }

    pub fn update_thumbnail(&self, node: &Resource, thumbnail: &[u8]) {
        let imp = NodeAreaPrivate::from_instance(self);
        imp.update_thumbnail(node, thumbnail);
    }
}

impl Default for NodeArea {
    fn default() -> Self {
        Self::new()
    }
}
