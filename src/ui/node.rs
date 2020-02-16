use super::node_socket;
use super::subclass::*;
use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::RefCell;

struct NodeChild {
    item: gtk::Widget,
    socket: node_socket::NodeSocket,
}

struct Border<T> {
    top: T,
    bottom: T,
    left: T,
    right: T,
}

pub struct NodePrivate {
    event_window: RefCell<Option<gdk::Window>>,
    children: Vec<NodeChild>,
    expander: gtk::Expander,
    padding: Border<i32>,
    margin: Border<i32>,
    allocation: RefCell<gtk::Allocation>,
}

// ObjectSubclass is the trait that defines the new type and
// contains all information needed by the GObject type system,
// including the new type's name, parent type, etc.
impl ObjectSubclass for NodePrivate {
    const NAME: &'static str = "NodePrivate";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
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
                klass.realize = Some(extra_widget_realize::<NodePrivate>);
                klass.unrealize = Some(extra_widget_unrealize::<NodePrivate>);
                klass.map = Some(extra_widget_map::<NodePrivate>);
                klass.unmap = Some(extra_widget_unmap::<NodePrivate>);
                // klass.size_allocate = Some(extra_widget_size_allocate::<NodePrivate>);
            }
        }

        // TODO: Signals
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            event_window: RefCell::new(None),
            children: Vec::new(),
            expander: gtk::Expander::new(Some("Node")),
            padding: Border {
                top: 8,
                bottom: 8,
                left: 8,
                right: 8,
            },
            margin: Border {
                top: 8,
                bottom: 8,
                left: 8,
                right: 8,
            },
            allocation: RefCell::new(gtk::Allocation {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            }),
        }
    }
}

impl ObjectImpl for NodePrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let node = obj.clone().downcast::<Node>().unwrap();
        node.set_has_window(false);
        node.set_size_request(100, 100);

        // Set box layout
        node.set_homogeneous(false);
        node.clone().upcast::<gtk::Box>().set_orientation(gtk::Orientation::Vertical);

        // Expander
        self.expander.set_expanded(true);
        node.pack_start(&self.expander, false, false, 0);
    }
}

impl gtk::subclass::widget::WidgetImpl for NodePrivate {
    fn draw(&self, widget: &gtk::Widget, cr: &cairo::Context) -> gtk::Inhibit {
        use gtk::subclass::widget::*;

        let mut allocation = widget.get_allocation();
        allocation.x = self.margin.left;
        allocation.y = self.margin.top;
        allocation.width -= self.margin.left + self.margin.right;
        allocation.height -= self.margin.top + self.margin.bottom;
       
        self.draw_frame(cr, &allocation);
        self.parent_draw(widget, cr);

        Inhibit(false)
    }

    fn adjust_size_request(
        &self,
        _widget: &gtk::Widget,
        orientation: gtk::Orientation,
        minimum_size: &mut i32,
        natural_size: &mut i32,
    ) {
        let h = self.padding.left + self.padding.right + self.margin.left + self.margin.right;
        let v = self.padding.top + self.padding.bottom + self.margin.top + self.margin.bottom;

        match orientation {
            gtk::Orientation::Horizontal => {
                *minimum_size += h + 25;
                *natural_size += h + 25;
            }
            gtk::Orientation::Vertical => {
                *minimum_size += v;
                *natural_size += v;
            }
            _ => unreachable!("Impossible orientation"),
        };
    }
}

impl WidgetImplExtra for NodePrivate {
    fn map(&self, widget: &gtk::Widget) {
        self.parent_map(widget);

        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.show();
        }
    }

    fn unmap(&self, widget: &gtk::Widget) {
        if let Some(ew) = self.event_window.borrow().as_ref() {
            ew.hide();
        }

        self.parent_unmap(widget);
    }

    fn realize(&self, widget: &gtk::Widget) {
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

        widget.set_window(&parent);
        widget.register_window(&window);

        for child in self.children.iter() {
            child.item.set_parent_window(&window);
            child.socket.set_parent_window(&window);
        }

        //self.expander.set_parent_window(&window);
        self.event_window.replace(Some(window));
    }

    fn unrealize(&self, widget: &gtk::Widget) {
        let mut destroy_window = false;
        if let Some(ew) = self.event_window.borrow().as_ref() {
            widget.unregister_window(ew);
            ew.destroy();
            destroy_window = true;
        }
        if destroy_window {
            self.event_window.replace(None);
        }
    }

    fn size_allocate(&self, widget: &gtk::Widget, allocation: &mut gtk::Allocation) {
        self.allocation.replace(*allocation);
        let spacing = Border {
            top: self.padding.top + self.margin.top,
            left: self.padding.left + self.margin.left,
            right: self.padding.right + self.margin.right,
            bottom: self.padding.bottom + self.margin.bottom,
        };

        allocation.x = spacing.left;
        allocation.y = spacing.top;
        allocation.width -= spacing.left + spacing.right;
        allocation.height -= spacing.top + spacing.bottom;

        // allocate the node items
        self.parent_size_allocate(widget, allocation);
    }
}

impl gtk::subclass::container::ContainerImpl for NodePrivate {
    fn add(&self, container: &gtk::Container, widget: &gtk::Widget) {
        use gtk::subclass::container::*;
        self.parent_add(container, widget);
        // TODO: add socket
    }

    fn remove(&self, container: &gtk::Container, widget: &gtk::Widget) {
        use gtk::subclass::container::*;
        self.parent_remove(container,widget);
        // TODO: remove socket
    }
}

impl gtk::subclass::box_::BoxImpl for NodePrivate {}

impl NodePrivate {
    fn draw_frame(&self, cr: &cairo::Context, allocation: &gtk::Allocation) {
       
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
        let na: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        na.set_has_window(false);
        na
    }
}
