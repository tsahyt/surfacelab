use super::subclass::*;
use crate::clone;
use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;

use std::cell::RefCell;

pub struct NodePrivate {}

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

            class.add_signal(
                HEADER_BUTTON_PRESS,
                glib::SignalFlags::empty(),
                &[],
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
            let header_box = gtk::Box::new(gtk::Orientation::Horizontal, HEADER_SPACING);
            let header_label = gtk::Label::new(Some("Node"));
            let header_evbox = gtk::EventBox::new();
            header_label.set_halign(gtk::Align::Start);

            header_evbox.connect_button_press_event(clone!(node => move |_, _| {
                node.emit(HEADER_BUTTON_PRESS, &[]).unwrap();
                Inhibit(false)
            }));
            header_evbox.connect_button_release_event(clone!(node => move |_, _| {
                node.emit(HEADER_BUTTON_RELEASE, &[]).unwrap();
                Inhibit(false)
            }));
            header_evbox.add(&header_label);
            header_box.pack_start(&header_evbox, false, false, 0);

            let close_image = gtk::Image::new_from_icon_name(
                Some("window-close-symbolic"),
                gtk::IconSize::Button,
            );
            let close_evbox = gtk::EventBox::new();
            close_evbox.connect_button_release_event(clone!(node => move |_, _| {
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

            thumbnail.connect_draw(|w, cr| {
                cr.set_source_rgba(0., 0., 0., 1.);
                cr.rectangle(0., 0., 128., 128.);
                cr.fill();
                Inhibit(false)
            });

            thumbnail.set_margin_start(MARGIN);
            thumbnail.set_margin_end(MARGIN);
            thumbnail.set_margin_bottom(MARGIN);

            node.add(&thumbnail);
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
