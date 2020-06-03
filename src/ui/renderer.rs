use super::render_area;
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

    fn new() -> Self {
        Self {
            render_area: OnceCell::new(),
            last_pos: Cell::new((0., 0.)),
            resolution: Cell::new((0., 0.)),
        }
    }
}

impl ContainerImpl for RenderEventsPrivate {}

impl BinImpl for RenderEventsPrivate {}

impl EventBoxImpl for RenderEventsPrivate {}

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

    pub fn unique_identifier(&self) -> u64 {
        let imp = RenderEventsPrivate::from_instance(self);
        imp.render_area.get().unwrap().unique_identifier()
    }
}

pub struct Renderer3DViewPrivate {
    event_area: RenderEvents,
}

impl ObjectSubclass for Renderer3DViewPrivate {
    const NAME: &'static str = "Renderer3DView";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            event_area: RenderEvents::new(render_area::RenderArea::new(RendererType::Renderer3D)),
        }
    }
}

impl ObjectImpl for Renderer3DViewPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let box_ = obj.downcast_ref::<gtk::Box>().unwrap();
        box_.set_orientation(gtk::Orientation::Vertical);
        box_.pack_end(&self.event_area, true, true, 0);
        box_.show_all();
    }
}

impl WidgetImpl for Renderer3DViewPrivate {}

impl ContainerImpl for Renderer3DViewPrivate {}

impl BoxImpl for Renderer3DViewPrivate {}

glib_wrapper! {
    pub struct Renderer3DView(
        Object<subclass::simple::InstanceStruct<Renderer3DViewPrivate>,
        subclass::simple::ClassStruct<Renderer3DViewPrivate>,
        Renderer3DViewClass>)
        @extends gtk::Widget, gtk::Container, gtk::Bin, gtk::EventBox;

    match fn {
        get_type => || Renderer3DViewPrivate::get_type().to_glib(),
    }
}

impl Renderer3DView {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }
}

impl Default for Renderer3DView {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Renderer2DViewPrivate {
    event_area: RenderEvents,
}

impl ObjectSubclass for Renderer2DViewPrivate {
    const NAME: &'static str = "Renderer2DView";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    fn new() -> Self {
        Self {
            event_area: RenderEvents::new(render_area::RenderArea::new(RendererType::Renderer2D)),
        }
    }
}

impl ObjectImpl for Renderer2DViewPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let box_ = obj.downcast_ref::<gtk::Box>().unwrap();
        box_.set_orientation(gtk::Orientation::Vertical);
        box_.pack_end(&self.event_area, true, true, 0);
        box_.show_all();

        let renderer_id = self.event_area.unique_identifier();

        let toolbox = gtk::BoxBuilder::new()
            .orientation(gtk::Orientation::Horizontal)
            .build();

        let channel_box = gtk::ButtonBoxBuilder::new()
            .layout_style(gtk::ButtonBoxStyle::Expand)
            .build();

        let displacement_btn = gtk::RadioButtonBuilder::new()
            .label("D")
            .draw_indicator(false)
            .build();
        displacement_btn.connect_toggled(move |_| {
            super::emit(Lang::UserRenderEvent(UserRenderEvent::ChannelChange2D(
                renderer_id,
                RenderChannel::Displacement,
            )))
        });
        let albedo_btn = gtk::RadioButtonBuilder::new()
            .label("A")
            .draw_indicator(false)
            .build();
        albedo_btn.join_group(Some(&displacement_btn));
        albedo_btn.connect_toggled(move |_| {
            super::emit(Lang::UserRenderEvent(UserRenderEvent::ChannelChange2D(
                renderer_id,
                RenderChannel::Albedo,
            )))
        });
        let normal_btn = gtk::RadioButtonBuilder::new()
            .label("N")
            .draw_indicator(false)
            .build();
        normal_btn.join_group(Some(&displacement_btn));
        normal_btn.connect_toggled(move |_| {
            super::emit(Lang::UserRenderEvent(UserRenderEvent::ChannelChange2D(
                renderer_id,
                RenderChannel::Normal,
            )))
        });
        let roughness_btn = gtk::RadioButtonBuilder::new()
            .label("R")
            .draw_indicator(false)
            .build();
        roughness_btn.join_group(Some(&displacement_btn));
        roughness_btn.connect_toggled(move |_| {
            super::emit(Lang::UserRenderEvent(UserRenderEvent::ChannelChange2D(
                renderer_id,
                RenderChannel::Roughness,
            )))
        });

        channel_box.add(&displacement_btn);
        channel_box.add(&albedo_btn);
        channel_box.add(&normal_btn);
        channel_box.add(&roughness_btn);
        toolbox.pack_end(&channel_box, false, false, 8);

        box_.pack_start(&toolbox, false, true, 0);
    }
}

impl WidgetImpl for Renderer2DViewPrivate {}

impl ContainerImpl for Renderer2DViewPrivate {}

impl BoxImpl for Renderer2DViewPrivate {}

glib_wrapper! {
    pub struct Renderer2DView(
        Object<subclass::simple::InstanceStruct<Renderer2DViewPrivate>,
        subclass::simple::ClassStruct<Renderer2DViewPrivate>,
        Renderer2DViewClass>)
        @extends gtk::Widget, gtk::Container, gtk::Bin, gtk::EventBox;

    match fn {
        get_type => || Renderer2DViewPrivate::get_type().to_glib(),
    }
}

impl Renderer2DView {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }
}

impl Default for Renderer2DView {
    fn default() -> Self {
        Self::new()
    }
}
