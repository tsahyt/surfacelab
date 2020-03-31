use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

type HSV = [f32; 3];

const THICKNESS: f64 = 16.0;
const SQUARE_PADDING: f64 = 2.0;

// Signals
pub const COLOR_PICKED: &str = "color-picked";

#[derive(Debug, Copy, Clone)]
enum Handle {
    HueHandle,
    SVHandle,
    None,
}

pub struct ColorWheelPrivate {
    hsv: Rc<Cell<HSV>>,
    handle: Rc<Cell<Handle>>,
    wheel_da: gtk::DrawingArea,
}

impl ObjectSubclass for ColorWheelPrivate {
    const NAME: &'static str = "ColorWheel";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {
        class.add_signal(
            COLOR_PICKED,
            glib::SignalFlags::empty(),
            &[
                glib::types::Type::F64,
                glib::types::Type::F64,
                glib::types::Type::F64,
            ],
            glib::types::Type::Unit,
        );
    }

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            hsv: Rc::new(Cell::new([0., 0., 0.9])),
            handle: Rc::new(Cell::new(Handle::None)),
            wheel_da: gtk::DrawingAreaBuilder::new()
                .width_request(192)
                .height_request(192)
                .events(
                    gdk::EventMask::BUTTON_PRESS_MASK
                        | gdk::EventMask::BUTTON_RELEASE_MASK
                        | gdk::EventMask::BUTTON1_MOTION_MASK,
                )
                .build(),
        }
    }
}

impl ObjectImpl for ColorWheelPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &Object) {
        let color_wheel_box = obj.clone().downcast::<gtk::Box>().unwrap();
        let color_wheel = obj.clone().downcast::<ColorWheel>().unwrap();
        color_wheel_box.set_orientation(gtk::Orientation::Horizontal);

        self.wheel_da.connect_draw(
            clone!(@strong self.hsv as hsv => move |w, da| wheel_draw(hsv.get(), w, da)),
        );

        self.wheel_da.connect_button_press_event(
            clone!(@strong self.handle as handle, @strong self.hsv as hsv => move |w, e| {
                let (x, y) = e.get_position();
                let new_handle = get_handle_at(hsv.get(), &w.get_allocation(), x as _, y as _);
                handle.set(new_handle);
                Inhibit(false)
            }),
        );

        self.wheel_da.connect_button_release_event(
            clone!(@strong self.handle as handle => move |_, _| {
                handle.set(Handle::None);
                Inhibit(false)
            }),
        );

        self.wheel_da.connect_motion_notify_event(
            clone!(@strong self.handle as handle, @strong self.hsv as hsv, @strong color_wheel => move |w, e| {
                match handle.get() {
                    Handle::None => {}
                    Handle::HueHandle => {
                        let allocation = w.get_allocation();
                        let (x,y) = e.get_position();
                        let center_x = allocation.width as f64 / 2.;
                        let center_y = allocation.height as f64 / 2.;

                        let dx = x as f64 - center_x;
                        let dy = y as f64 - center_y;
                        let mut angle = dy.atan2(dx);
                        if angle < 0. { angle += std::f64::consts::TAU; }

                        let mut old_hsv = hsv.get();
                        old_hsv[0] = (angle / std::f64::consts::TAU) as f32;
                        hsv.set(old_hsv);

                        w.queue_draw();
                        color_wheel.emit_color_picked();
                    }
                    Handle::SVHandle => {
                        let allocation = w.get_allocation();
                        let (x,y) = e.get_position();

                        let center_x = allocation.width as f64 / 2.;
                        let radius = center_x - THICKNESS / 2.0;
                        let square_size = std::f64::consts::SQRT_2 * (radius - THICKNESS / 2.0) - SQUARE_PADDING;
                        let offset = (allocation.width as f64 - square_size) / 2.;

                        let mut old_hsv = hsv.get();
                        old_hsv[1] = (((x as f64).clamp(offset, offset+square_size) - offset) / square_size) as f32;
                        old_hsv[2] = (((y as f64).clamp(offset, offset+square_size) - offset) / square_size) as f32;
                        hsv.set(old_hsv);

                        w.queue_draw();
                        color_wheel.emit_color_picked();
                    }
                }
                Inhibit(false)
            }),
        );

        color_wheel_box.pack_start(&self.wheel_da, true, true, 8);
    }
}

impl WidgetImpl for ColorWheelPrivate {}

impl ContainerImpl for ColorWheelPrivate {}

impl BoxImpl for ColorWheelPrivate {}

impl ColorWheelPrivate {}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0. {
        (v, v, v)
    } else {
        let mut hue = h * 6.0;
        if hue >= 6.0 {
            hue = 0.0;
        }

        let f = hue.fract();
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1. - f));

        match hue as u8 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            5 => (v, p, q),
            _ => unreachable!(),
        }
    }
}

fn hue_handle_position(hsv: HSV, allocation: &gtk::Allocation) -> (f64, f64) {
    let center_x = allocation.width as f64 / 2.;
    let center_y = allocation.height as f64 / 2.;
    let radius = center_x - THICKNESS / 2.0;

    (
        center_x + (hsv[0] as f64 * std::f64::consts::TAU).cos() * radius,
        center_y + (hsv[0] as f64 * std::f64::consts::TAU).sin() * radius,
    )
}

fn sv_handle_position(hsv: HSV, allocation: &gtk::Allocation) -> (f64, f64) {
    let center_x = allocation.width as f64 / 2.;
    let radius = center_x - THICKNESS / 2.0;
    let square_size = std::f64::consts::SQRT_2 * (radius - THICKNESS / 2.0) - SQUARE_PADDING;
    let offset = (allocation.width as f64 - square_size) / 2.;

    (
        offset + square_size * hsv[1] as f64,
        offset + square_size * hsv[2] as f64,
    )
}

fn get_handle_at(hsv: HSV, allocation: &gtk::Allocation, x: f64, y: f64) -> Handle {
    let hue_handle = hue_handle_position(hsv, allocation);

    if x > hue_handle.0 - THICKNESS
        && x < hue_handle.0 + THICKNESS
        && y > hue_handle.1 - THICKNESS
        && y < hue_handle.1 + THICKNESS
    {
        return Handle::HueHandle;
    }

    let sv_handle = sv_handle_position(hsv, allocation);

    if x > sv_handle.0 - THICKNESS
        && x < sv_handle.0 + THICKNESS
        && y > sv_handle.1 - THICKNESS
        && y < sv_handle.1 + THICKNESS
    {
        return Handle::SVHandle;
    }

    Handle::None
}

fn wheel_draw(hsv: HSV, drawing_area: &gtk::DrawingArea, cr: &cairo::Context) -> gtk::Inhibit {
    let allocation = drawing_area.get_allocation();
    let center_x = allocation.width as f64 / 2.;
    let center_y = allocation.height as f64 / 2.;
    let radius = center_x - THICKNESS / 2.0;
    let square_size = std::f64::consts::SQRT_2 * (radius - THICKNESS / 2.0) - SQUARE_PADDING;

    // Wheel Drawing
    let ring_img: image::RgbaImage = // use rgba here for alignment!
        image::ImageBuffer::from_fn(allocation.width as _, allocation.height as _, |x, y| {
            let dx = x as f64 - center_x;
            let dy = -(y as f64 - center_y);

            // TODO: optimization by limiting calculation to within-ring pixels

            let mut angle = dy.atan2(dx);
            if angle < 0. { angle += std::f64::consts::TAU; }
            let hue = angle / std::f64::consts::TAU;

            let (r,g,b) = hsv_to_rgb(hue, 1., 1.);

            image::Rgba([(r * 255. + 0.5) as u8, (g * 255. + 0.5) as u8, (b * 255. + 0.5) as u8, 255])
        });

    let src_img = cairo::ImageSurface::create_for_data(
        ring_img.into_raw(),
        cairo::Format::Rgb24,
        allocation.width,
        allocation.height,
        cairo::Format::Rgb24
            .stride_for_width(allocation.width as _)
            .expect("Error computing stride"),
    )
    .expect("Error creating color ring source image");

    cr.set_source_surface(&src_img, 0., 0.);
    cr.set_line_width(THICKNESS);
    cr.arc(center_x, center_y, radius, 0.0, std::f64::consts::TAU);
    cr.stroke();

    // Hue Marker
    let hue_marker = hue_handle_position(hsv, &allocation);
    draw_marker(cr, hue_marker.0, hue_marker.1);

    // Saturation/Value Rectangle
    let square_img: image::RgbaImage =
        image::ImageBuffer::from_fn(square_size as _, square_size as _, |x, y| {
            let xx = x as f64 / square_size;
            let yy = y as f64 / square_size;

            let (r, g, b) = hsv_to_rgb(hsv[0] as _, xx, yy);
            image::Rgba([
                (r * 255. + 0.5) as u8,
                (g * 255. + 0.5) as u8,
                (b * 255. + 0.5) as u8,
                255,
            ])
        });

    let src_img = cairo::ImageSurface::create_for_data(
        square_img.into_raw(),
        cairo::Format::Rgb24,
        square_size as _,
        square_size as _,
        cairo::Format::Rgb24
            .stride_for_width(square_size as _)
            .expect("Error computing stride"),
    )
    .expect("Error creating color square source image");

    let offset = (allocation.width as f64 - square_size) / 2.;
    cr.set_source_surface(&src_img, offset, offset);
    cr.rectangle(offset, offset, square_size, square_size);
    cr.fill();

    // Saturation and Value Marker
    let sv_marker = sv_handle_position(hsv, &allocation);
    draw_marker(cr, sv_marker.0, sv_marker.1);

    Inhibit(false)
}

fn draw_marker(cr: &cairo::Context, x: f64, y: f64) {
    cr.set_source_rgb(0.9, 0.9, 0.9);
    cr.set_line_width(4.5);
    cr.arc(x, y, THICKNESS / 3., 0.0, std::f64::consts::TAU);
    cr.stroke();
    cr.set_line_width(1.5);
    cr.set_source_rgb(0., 0., 0.);
    cr.arc(x, y, THICKNESS / 3., 0.0, std::f64::consts::TAU);
    cr.stroke();
}

glib_wrapper! {
    pub struct ColorWheel(
        Object<subclass::simple::InstanceStruct<ColorWheelPrivate>,
        subclass::simple::ClassStruct<ColorWheelPrivate>,
        ColorWheelClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

    match fn {
        get_type => || ColorWheelPrivate::get_type().to_glib(),
    }
}

impl ColorWheel {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }

    pub fn connect_color_picked<F: Fn(&Self, f64, f64, f64) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local(COLOR_PICKED, true, move |w| {
            let color_wheel = w[0]
                .clone()
                .downcast::<ColorWheel>()
                .unwrap()
                .get()
                .unwrap();
            let r: f64 = w[1].get_some().unwrap();
            let g: f64 = w[2].get_some().unwrap();
            let b: f64 = w[3].get_some().unwrap();
            f(&color_wheel, r, g, b);
            None
        })
        .unwrap()
    }

    fn emit_color_picked(&self) {
        let imp = ColorWheelPrivate::from_instance(self);
        let hsv = imp.hsv.get();
        let (r, g, b) = hsv_to_rgb(hsv[0].into(), hsv[1].into(), hsv[2].into());
        self.emit(COLOR_PICKED, &[&r, &g, &b]).unwrap();
    }
}

impl Default for ColorWheel {
    fn default() -> Self {
        Self::new()
    }
}
