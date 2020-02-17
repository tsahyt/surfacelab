use super::subclass::*;
use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::RefCell;

pub struct NodePrivate {}

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
                //klass.realize = Some(extra_widget_realize::<NodePrivate>);
                //klass.unrealize = Some(extra_widget_unrealize::<NodePrivate>);
                //klass.map = Some(extra_widget_map::<NodePrivate>);
                //klass.unmap = Some(extra_widget_unmap::<NodePrivate>);
                //klass.size_allocate = Some(extra_widget_size_allocate::<NodePrivate>);
            }
        }
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {}
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
            const HEADER_SPACING: i32 = 16;

            let header_box = gtk::Box::new(gtk::Orientation::Horizontal, HEADER_SPACING);
            let header_label = gtk::Label::new(Some("Node"));
            header_box.pack_start(&header_label, true, false, 0);

            let close_button = gtk::Button::new();
            close_button.set_relief(gtk::ReliefStyle::None);
            let close_image = gtk::Image::new_from_icon_name(
                Some("window-close-symbolic"),
                gtk::IconSize::Button,
            );
            close_button.add(&close_image);
            header_box.pack_end(&close_button, false, false, 0);

            node.add(&header_box);
        }

        // thumbnail
        {
            let thumbnail = gtk::DrawingArea::new();
            thumbnail.set_size_request(128, 128);

            thumbnail.connect_draw(|w, cr| {
                let allocation = w.get_allocation();
                cr.set_source_rgba(0., 0., 0., 1.);
                cr.rectangle(0., 0., allocation.width as _, allocation.height as _);
                cr.fill();
                Inhibit(false)
            });

            node.add(&thumbnail);
        }
    }
}

impl gtk::subclass::widget::WidgetImpl for NodePrivate {}

impl WidgetImplExtra for NodePrivate {}

impl gtk::subclass::container::ContainerImpl for NodePrivate {}

impl gtk::subclass::box_::BoxImpl for NodePrivate {}

impl NodePrivate {}

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
