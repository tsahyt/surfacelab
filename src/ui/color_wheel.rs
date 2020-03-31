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

pub struct ColorWheelPrivate {
    hsv: Rc<Cell<HSV>>,
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
    fn class_init(_class: &mut subclass::simple::ClassStruct<Self>) {}

    // Called every time a new instance is created. This should return
    // a new instance of our type with its basic values.
    fn new() -> Self {
        Self {
            hsv: Rc::new(Cell::new([0., 0., 0.9])),
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
        color_wheel_box.set_orientation(gtk::Orientation::Horizontal);

        self.wheel_da.connect_draw(
            clone!(@strong self.hsv as hsv => move |w, da| wheel_draw(hsv.get(), w, da)),
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

fn wheel_draw(hsv: HSV, drawing_area: &gtk::DrawingArea, cr: &cairo::Context) -> gtk::Inhibit {
    let allocation = drawing_area.get_allocation();
    let thickness = 16.;
    let center_x = allocation.width as f64 / 2.;
    let center_y = allocation.height as f64 / 2.;
    let radius = center_x - thickness / 2.0;
    let square_padding = 8.0;
    let square_size = std::f64::consts::SQRT_2 * (radius - thickness / 2.0) - square_padding;

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
    cr.set_line_width(thickness);
    cr.arc(center_x, center_y, radius, 0.0, std::f64::consts::TAU);
    cr.stroke();

    // Hue Marker
    draw_marker(
        cr,
        center_x + (hsv[0] as f64 * std::f64::consts::TAU).cos() * radius,
        center_y + (hsv[0] as f64 * std::f64::consts::TAU).sin() * radius,
        thickness,
    );

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
    draw_marker(
        cr,
        offset + square_size * hsv[1] as f64,
        offset + square_size * hsv[2] as f64,
        thickness,
    );

    Inhibit(false)
}

fn draw_marker(cr: &cairo::Context, x: f64, y: f64, thickness: f64) {
    cr.set_source_rgb(0.9, 0.9, 0.9);
    cr.set_line_width(4.5);
    cr.arc(x, y, thickness / 3., 0.0, std::f64::consts::TAU);
    cr.stroke();
    cr.set_line_width(1.5);
    cr.set_source_rgb(0., 0., 0.);
    cr.arc(x, y, thickness / 3., 0.0, std::f64::consts::TAU);
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
}

impl Default for ColorWheel {
    fn default() -> Self {
        Self::new()
    }
}
