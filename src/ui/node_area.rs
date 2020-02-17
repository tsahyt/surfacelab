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
                klass.realize = Some(extra_widget_realize::<NodeAreaPrivate>);
                klass.size_allocate = Some(extra_widget_size_allocate::<NodeAreaPrivate>);
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
}

impl NodeAreaPrivate {
    fn connecting_curve(cr: &cairo::Context, source: (f64, f64), sink: (f64, f64)) {
        cr.move_to(source.0, source.1);
        let d = (sink.0 - source.0).abs() / 2.0;
        cr.curve_to(source.0 + d, source.1, sink.0 - d, sink.1, sink.0, sink.1);
    }

    fn put(&self, container: &gtk::Container, widget: &Node, x: i32, y: i32) {
        // Make sure the widget is indeed a Node and does not yet have a parent
        debug_assert!(widget.get_parent().is_none());

        let child = NodeAreaChild { x, y };

        widget.set_parent(container);
        self.children.borrow_mut().insert(widget.clone(), child);

        // Connect to child signals
        let action = self.action.clone();
        let children = self.children.clone();
        let allocation = container.get_allocation();

        widget.connect_header_button_press_event(clone!(action => move |_, x, y| {
            action.replace(Some(Action::DragChild(allocation.x + x as i32, allocation.y + y as i32)));
        }));

        widget.connect_header_button_release_event(clone!(action => move |_| {
            action.replace(None);
        }));

        widget.connect_motion_notify_event(
            clone!(action, children, container => move |w, motion| {
                if let Some(Action::DragChild(offset_x, offset_y)) = action.borrow().as_ref() {
                    let pos = motion.get_root();

                    //dbg!(pos);

                    let mut children = children.borrow_mut();
                    let c_ref = children.get_mut(&w);
                    let child = c_ref.unwrap();
                    child.x = pos.0 as i32 - offset_x;
                    child.y = pos.1 as i32 - offset_y;

                    if w.get_visible() {
                        w.queue_resize();
                    }

                    container.queue_draw();
                }
                Inhibit(true)
            }),
        );

        widget.connect_close_clicked(clone!(container => move |w| {
            container.remove(w);
        }));
    }
}

impl gtk::subclass::widget::WidgetImpl for NodeAreaPrivate {
    fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        let container = widget.clone().downcast::<gtk::Container>().unwrap();

        for (node, _) in self.children.borrow().iter() {
            container.propagate_draw(node, cr);
        }

        Inhibit(false)
    }
}

impl WidgetImplExtra for NodeAreaPrivate {
    fn realize(&self, widget: &gtk::Widget) {
        widget.set_realized(true);
        let allocation = widget.get_allocation();
        let attributes = gdk::WindowAttr {
            window_type: gdk::WindowType::Child,
            x: Some(allocation.x),
            y: Some(allocation.y),
            width: allocation.width,
            wclass: gdk::WindowWindowClass::InputOutput,
            visual: widget.get_visual(),
            event_mask: {
                let mut em = widget.get_events();
                em.insert(gdk::EventMask::EXPOSURE_MASK);
                em.insert(gdk::EventMask::BUTTON_PRESS_MASK);
                em.bits() as _
            },
            ..gdk::WindowAttr::default()
        };

        let window = gdk::Window::new(
            Some(
                &widget
                    .get_parent_window()
                    .expect("Node Area must have parent"),
            ),
            &attributes,
        );
        widget.set_window(&window);
        widget.register_window(&window);
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        widget.set_allocation(allocation);

        let has_window = widget.get_has_window();

        if has_window && widget.get_realized() {
            widget.get_window().unwrap().move_resize(
                allocation.x,
                allocation.y,
                allocation.width,
                allocation.height,
            )
        }

        for (node, child) in self.children.borrow().iter() {
            if !node.get_visible() {
                continue;
            }
            let (child_requisition, _) = node.get_preferred_size();

            let mut child_allocation = gtk::Allocation {
                x: child.x + if !has_window { allocation.x } else { 0 },
                y: child.y + if !has_window { allocation.y } else { 0 },
                width: child_requisition.width,
                height: child_requisition.height,
            };

            node.size_allocate(&mut child_allocation)
        }
    }
}

impl gtk::subclass::container::ContainerImpl for NodeAreaPrivate {
    // Node Areas contain nodes and nothing else
    fn child_type(&self, _container: &gtk::Container) -> glib::Type {
        Node::static_type()
    }

    fn add(&self, container: &gtk::Container, widget: &gtk::Widget) {
        let widget = widget
            .clone()
            .downcast::<Node>()
            .expect("Node Area can only contain nodes!");
        self.put(container, &widget, 0, 0);
    }

    fn remove(&self, container: &gtk::Container, widget: &gtk::Widget) {
        let widget = widget
            .clone()
            .downcast::<Node>()
            .expect("Node Area can only contain nodes!");

        let resize_after = self.children.borrow().contains_key(&widget)
            && widget.get_visible()
            && container.get_visible();

        if let Some(_) = self.children.borrow_mut().remove(&widget) {
            widget.unparent()
        }

        if resize_after {
            container.queue_resize()
        }
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
