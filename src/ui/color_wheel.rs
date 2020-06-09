use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::Cell;
use std::rc::Rc;

type HSV = [f64; 3];

const THICKNESS: f64 = 16.0;
const SQUARE_PADDING: f64 = 2.0;

// Signals
pub const COLOR_PICKED: &str = "color-picked";

#[derive(Debug, Copy, Clone)]
enum Handle {
    Hue,
    SV,
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
        let color_wheel_box = obj.downcast_ref::<gtk::Box>().unwrap();
        let color_wheel = obj.downcast_ref::<ColorWheel>().unwrap();
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
                    Handle::Hue => {
                        let allocation = w.get_allocation();
                        let (x,y) = e.get_position();
                        let center_x = allocation.width as f64 / 2.;
                        let center_y = allocation.height as f64 / 2.;

                        let dx = x as f64 - center_x;
                        let dy = y as f64 - center_y;
                        let mut angle = dy.atan2(dx);
                        if angle < 0. { angle += std::f64::consts::TAU; }

                        let mut old_hsv = hsv.get();
                        old_hsv[0] = angle / std::f64::consts::TAU;
                        hsv.set(old_hsv);

                        w.queue_draw();
                        color_wheel.emit_color_picked();
                    }
                    Handle::SV => {
                        let allocation = w.get_allocation();
                        let (x,y) = e.get_position();

                        let center_x = allocation.width as f64 / 2.;
                        let radius = center_x - THICKNESS / 2.0;
                        let square_size = std::f64::consts::SQRT_2 * (radius - THICKNESS / 2.0) - SQUARE_PADDING;
                        let offset = (allocation.width as f64 - square_size) / 2.;

                        let mut old_hsv = hsv.get();
                        old_hsv[1] = ((x as f64).clamp(offset, offset+square_size) - offset) / square_size;
                        old_hsv[2] = ((y as f64).clamp(offset, offset+square_size) - offset) / square_size;
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

fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (f64, f64, f64) {
    if saturation == 0. {
        (value, value, value)
    } else {
        let mut hue_mult = hue * 6.0;
        if hue_mult >= 6.0 {
            hue_mult = 0.0;
        }

        let fract = hue_mult.fract();
        let p = value * (1.0 - saturation);
        let q = value * (1.0 - saturation * fract);
        let t = value * (1.0 - saturation * (1. - fract));

        match hue_mult as u8 {
            0 => (value, t, p),
            1 => (q, value, p),
            2 => (p, value, t),
            3 => (p, q, value),
            4 => (t, p, value),
            5 => (value, p, q),
            _ => unreachable!(),
        }
    }
}

#[allow(clippy::float_cmp)]
fn rgb_to_hsv(red: f64, green: f64, blue: f64) -> (f64, f64, f64) {
    let (max, min, sep, coeff) = {
        let (max, min, sep, coeff) = if red > green {
            (red, green, green - blue, 0.0)
        } else {
            (green, red, blue - red, 2.0)
        };
        if blue > max {
            (blue, min, red - green, 4.0)
        } else {
            let min_val = if blue < min { blue } else { min };
            (max, min_val, sep, coeff)
        }
    };

    let mut h = 0.0;
    let mut s = 0.0;
    let v = max;

    if max != min {
        let d = max - min;
        s = d / max;
        h = ((sep / d) + coeff) * 60.0 / 360.0;
    };

    (h, s, v)
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
        return Handle::Hue;
    }

    let sv_handle = sv_handle_position(hsv, allocation);

    if x > sv_handle.0 - THICKNESS
        && x < sv_handle.0 + THICKNESS
        && y > sv_handle.1 - THICKNESS
        && y < sv_handle.1 + THICKNESS
    {
        return Handle::SV;
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
            let dy = y as f64 - center_y;

            // TODO: optimization by limiting calculation to within-ring pixels

            let mut angle = dy.atan2(dx);
            if angle < 0. { angle += std::f64::consts::TAU; }
            let hue = angle / std::f64::consts::TAU;

            let (r,g,b) = hsv_to_rgb(hue, 1., 1.);

            // WTF: This channel ordering is absolutely weird and I have no idea why it's required.
            image::Rgba([(b * 255. + 0.5) as u8, (g * 255. + 0.5) as u8, (r * 255. + 0.5) as u8, 255])
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
            // See above for WTF
            image::Rgba([
                (b.powf(1.0 / 2.2) * 255. + 0.5) as u8,
                (g.powf(1.0 / 2.2) * 255. + 0.5) as u8,
                (r.powf(1.0 / 2.2) * 255. + 0.5) as u8,
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

    pub fn new_with_rgb(r: f64, g: f64, b: f64) -> Self {
        let wheel = Self::new();
        wheel.set_rgb(r, g, b);
        wheel
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
        let (r, g, b) = hsv_to_rgb(hsv[0] as f64, hsv[1] as f64, hsv[2] as f64);
        self.emit(COLOR_PICKED, &[&r, &g, &b]).unwrap();
    }

    pub fn set_rgb(&self, red: f64, green: f64, blue: f64) {
        let imp = ColorWheelPrivate::from_instance(self);
        let hsv = rgb_to_hsv(red, green, blue);
        imp.hsv.set([hsv.0, hsv.1, hsv.2]);
        dbg!((red, green, blue), hsv);
        self.queue_draw();
    }
}

impl Default for ColorWheel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::*;
    use quickcheck::*;
    use rand::{Rng, RngCore};

    #[derive(Clone, Debug)]
    struct ColVal(f64);

    impl Arbitrary for ColVal {
        fn arbitrary<G: Gen>(g: &mut G) -> ColVal {
            ColVal(g.gen_range(0.0, 1.0))
        }
    }

    quickcheck! {
        fn rgb_hsv_roundtrip(r: ColVal, g: ColVal, b: ColVal) -> bool {
            let (h,s,v) = rgb_to_hsv(r.0, g.0, b.0);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            abs_diff_eq!(r.0, r2) && abs_diff_eq!(g.0, g2) && abs_diff_eq!(b.0, b2)
        }
    }

    #[test]
    fn rgb_hsv_roundtrip_m0() {
        let (r,g,b) = (0.653706138831377, 0.28974543928331586, 0.31952971618106907);
        let (h,s,v) = rgb_to_hsv(r, g, b);
        let (r2, g2, b2) = hsv_to_rgb(h, s, v);
        assert_abs_diff_eq!(r, r2);
        assert_abs_diff_eq!(g, g2);
        assert_abs_diff_eq!(b, b2)
    }

    #[test]
    fn hsv_pure_colors() {
        let (r, g, b) = hsv_to_rgb(0.0, 1.0, 1.0);
        assert_abs_diff_eq!(r, 1.0);
        assert_abs_diff_eq!(g, 0.0);
        assert_abs_diff_eq!(b, 0.0);

        let (r, g, b) = hsv_to_rgb(1.0 / 3.0, 1.0, 1.0);
        assert_abs_diff_eq!(r, 0.0);
        assert_abs_diff_eq!(g, 1.0);
        assert_abs_diff_eq!(b, 0.0);

        let (r, g, b) = hsv_to_rgb(2.0 / 3.0, 1.0, 1.0);
        assert_abs_diff_eq!(r, 0.0);
        assert_abs_diff_eq!(g, 0.0);
        assert_abs_diff_eq!(b, 1.0);
    }

    #[test]
    fn rgb_pure_colors() {
        let (h, s, v) = rgb_to_hsv(1.0, 0.0, 0.0);
        assert_abs_diff_eq!(h, 0.0);
        assert_abs_diff_eq!(s, 1.0);
        assert_abs_diff_eq!(v, 1.0);

        let (h, s, v) = rgb_to_hsv(0.0, 1.0, 0.0);
        assert_abs_diff_eq!(h, 1.0 / 3.0);
        assert_abs_diff_eq!(s, 1.0);
        assert_abs_diff_eq!(v, 1.0);

        let (h, s, v) = rgb_to_hsv(0.0, 0.0, 1.0);
        assert_abs_diff_eq!(h, 2.0 / 3.0);
        assert_abs_diff_eq!(s, 1.0);
        assert_abs_diff_eq!(v, 1.0);
    }
}
