use crate::lang::*;

use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use once_cell::unsync::OnceCell;
use std::cell::Cell;

pub struct RenderEventsPrivate {
    render_area: OnceCell<super::render_area::RenderArea>,
    last_pos: Cell<(f32, f32)>,
    resolution: Cell<(f32, f32)>,
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
            last_pos: Cell::new((0., 0.)),
            resolution: Cell::new((0., 0.)),
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
    fn size_allocate(&self, widget: &gtk::Widget, allocation: &gtk::Allocation) {
        self.parent_size_allocate(widget, allocation);
        self.resolution
            .set((allocation.width as _, allocation.height as _));
    }

    fn button_press_event(&self, _widget: &gtk::Widget, event: &gdk::EventButton) -> gtk::Inhibit {
        let (x, y) = event.get_position();
        self.last_pos.set((x as _, y as _));
        Inhibit(false)
    }

    fn motion_notify_event(&self, _widget: &gtk::Widget, event: &gdk::EventMotion) -> gtk::Inhibit {
        use gdk::ModifierType;
        let modifiers = event.get_state();
        if modifiers == (ModifierType::BUTTON1_MASK | ModifierType::SHIFT_MASK) {
            let (x, y) = self.relative_movement(event.get_position());
            super::emit(Lang::UserRenderEvent(UserRenderEvent::LightMove(
                self.render_area.get().unwrap().unique_identifier(),
                x as _,
                y as _,
            )))
        } else if modifiers == ModifierType::BUTTON1_MASK {
            let (x, y) = self.relative_movement(event.get_position());
            super::emit(Lang::UserRenderEvent(UserRenderEvent::Rotate(
                self.render_area.get().unwrap().unique_identifier(),
                x as _,
                y as _,
            )))
        } else if modifiers == ModifierType::BUTTON2_MASK {
            let (x, y) = self.relative_movement(event.get_position());
            super::emit(Lang::UserRenderEvent(UserRenderEvent::Pan(
                self.render_area.get().unwrap().unique_identifier(),
                x as _,
                y as _,
            )))
        } else if modifiers == ModifierType::BUTTON3_MASK {
            let (_, y) = self.relative_movement(event.get_position());
            super::emit(Lang::UserRenderEvent(UserRenderEvent::Zoom(
                self.render_area.get().unwrap().unique_identifier(),
                y as _,
            )))
        }

        let (x, y) = event.get_position();
        self.last_pos.set((x as _, y as _));
        Inhibit(false)
    }
}

impl RenderEventsPrivate {
    fn relative_movement(&self, (xp, yp): (f64, f64)) -> (f32, f32) {
        let (xr, yr) = self.resolution.get();
        let (xs, ys) = self.last_pos.get();
        let x = xp as f32 - xs;
        let y = ys - yp as f32;
        (x / xr, y / yr)
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
        ebox.set_above_child(true);
        ebox.set_visible_window(false);
        ebox.add(&render_area);
        let imp = RenderEventsPrivate::from_instance(&ebox);
        imp.render_area
            .set(render_area)
            .expect("Failed to set render area");
        ebox
    }
}
