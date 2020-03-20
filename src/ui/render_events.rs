use crate::lang;

use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use once_cell::unsync::OnceCell;

pub struct RenderEventsPrivate {
    render_area: OnceCell<super::render_area::RenderArea>,
}

impl ObjectSubclass for RenderEventsPrivate {
    const NAME: &'static str = "RenderEvents";

    type ParentType = gtk::EventBox;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    // fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {}

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            render_area: OnceCell::new(),
        }
    }
}

impl gtk::subclass::container::ContainerImpl for RenderEventsPrivate {}

impl gtk::subclass::bin::BinImpl for RenderEventsPrivate {}

impl gtk::subclass::event_box::EventBoxImpl for RenderEventsPrivate {}

impl ObjectImpl for RenderEventsPrivate {
    glib_object_impl!();
}

impl gtk::subclass::widget::WidgetImpl for RenderEventsPrivate {
    fn motion_notify_event(&self, _widget: &gtk::Widget, event: &gdk::EventMotion) -> gtk::Inhibit {
        use gdk::ModifierType;
        let modifiers = event.get_state();
        if modifiers == (ModifierType::BUTTON1_MASK | ModifierType::SHIFT_MASK) {
            // TODO: Light movement
        } else if modifiers == ModifierType::BUTTON1_MASK {
            // TODO: Rotate
        } else if modifiers == ModifierType::BUTTON3_MASK {
            // TODO: Zoom
        }
        Inhibit(false)
    }
}

glib_wrapper! {
    pub struct RenderEvents(
        Object<subclass::simple::InstanceStruct<RenderEventsPrivate>,
        subclass::simple::ClassStruct<RenderEventsPrivate>,
        RenderEventsClass>)
        @extends gtk::Widget, gtk::Container, gtk::Bin, gtk::EventBox;

    match fn {
        get_type => || RenderEventsPrivate::get_type().to_glib(),
    }
}

impl RenderEvents {
    pub fn new(render_area: super::render_area::RenderArea) -> Self {
        let ebox: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        ebox.add(&render_area);
        let imp = RenderEventsPrivate::from_instance(&ebox);
        imp.render_area
            .set(render_area)
            .expect("Failed to set render area");
        ebox
    }
}
