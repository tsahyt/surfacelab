use super::node::Node;
use super::subclass::*;
use crate::clone;
use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug)]
struct NodeAreaChild {
    x: i32,
    y: i32,
}

#[derive(Debug)]
enum Action {
    DragChild(i32, i32),
}

#[derive(Debug)]
pub struct NodeAreaPrivate {
    children: Rc<RefCell<HashMap<Node, NodeAreaChild>>>,
    action: Rc<RefCell<Option<Action>>>,
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodeAreaPrivate {
    const NAME: &'static str = "NodeAreaPrivate";

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
                klass.realize = Some(extra_widget_realize::<NodeAreaPrivate>);
                klass.size_allocate = Some(extra_widget_size_allocate::<NodeAreaPrivate>);
                klass.motion_notify_event =
                    Some(extra_widget_motion_notify_event::<NodeAreaPrivate>);
            }
        }
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            children: Rc::new(RefCell::new(HashMap::new())),
            action: Rc::new(RefCell::new(None)),
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
    }
}

impl NodeAreaPrivate {
    fn connecting_curve(cr: &cairo::Context, source: (f64, f64), sink: (f64, f64)) {
        cr.move_to(source.0, source.1);
        let d = (sink.0 - source.0).abs() / 2.0;
        cr.curve_to(source.0 + d, source.1, sink.0 - d, sink.1, sink.0, sink.1);
    }

    fn child_connect(&self, container: &gtk::Fixed, widget: &Node) {
        // Connect to child signals
        let action = self.action.clone();
        let allocation = container.get_allocation();
        let widget_u = widget.clone().upcast::<gtk::Widget>();

        widget.connect_header_button_press_event(clone!(action => move |_, x, y| {
            action.replace(Some(Action::DragChild(allocation.x + x as i32, allocation.y + y as i32)));
        }));

        widget.connect_header_button_release_event(clone!(action => move |_| {
            action.replace(None);
        }));

        widget.connect_motion_notify_event(
            clone!(action, widget_u, container => move |w, motion| {
                if let Some(Action::DragChild(offset_x, offset_y)) = action.borrow().as_ref() {
                    let pos = motion.get_root();

                    let new_x = pos.0 as i32 - offset_x;
                    let new_y = pos.1 as i32 - offset_y;

                    container.move_(&widget_u, new_x, new_y);

                    if w.get_visible() {
                        w.queue_resize();
                    }

                    container.queue_draw();
                }
                Inhibit(false)
            }),
        );

        widget.connect_close_clicked(clone!(container => move |w| {
            container.remove(w);
        }));
    }
}

impl gtk::subclass::widget::WidgetImpl for NodeAreaPrivate {
    // fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
    //     let container = widget.clone().downcast::<gtk::Container>().unwrap();

    //     for (node, _) in self.children.borrow().iter() {
    //         container.propagate_draw(node, cr);
    //     }

    //     Inhibit(false)
    // }

    fn button_press_event(&self, widget: &gtk::Widget, event: &gdk::EventButton) -> gtk::Inhibit {
        use gtk::subclass::widget::*;

        self.parent_button_press_event(widget, event);

        Inhibit(false)
    }
}

impl WidgetImplExtra for NodeAreaPrivate {}

impl gtk::subclass::container::ContainerImpl for NodeAreaPrivate {
    // Node Areas contain nodes and nothing else
    fn child_type(&self, _container: &gtk::Container) -> glib::Type {
        Node::static_type()
    }

    fn add(&self, container: &gtk::Container, widget: &gtk::Widget) {
        use gtk::subclass::container::*;
        debug_assert!(widget.get_type() == Node::static_type());

        let node = widget.clone().downcast::<Node>().unwrap();
        let fixed = container.clone().downcast::<gtk::Fixed>().unwrap();
        self.child_connect(&fixed, &node);

        self.parent_add(container, widget);
    }
}

impl gtk::subclass::fixed::FixedImpl for NodeAreaPrivate {}

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
