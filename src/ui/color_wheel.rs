use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::Cell;
use std::rc::Rc;
use palette::*;

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
    hsv: Rc<Cell<Hsv>>,
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
            hsv: Rc::new(Cell::new(Hsv::new(0., 0., 0.9))),
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
                        old_hsv.hue = RgbHue::from_radians(angle as f32);
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
                        old_hsv.saturation = (((x as f64).clamp(offset, offset+square_size) - offset) / square_size) as f32;
                        old_hsv.value = (((y as f64).clamp(offset, offset+square_size) - offset) / square_size) as f32;
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

fn hue_handle_position(hsv: Hsv, allocation: &gtk::Allocation) -> (f64, f64) {
    let center_x = allocation.width as f64 / 2.;
    let center_y = allocation.height as f64 / 2.;
    let radius = center_x - THICKNESS / 2.0;

    (
        center_x + hsv.hue.to_radians().cos() as f64 * radius,
        center_y + hsv.hue.to_radians().sin() as f64 * radius,
    )
}

fn sv_handle_position(hsv: Hsv, allocation: &gtk::Allocation) -> (f64, f64) {
    let center_x = allocation.width as f64 / 2.;
    let radius = center_x - THICKNESS / 2.0;
    let square_size = std::f64::consts::SQRT_2 * (radius - THICKNESS / 2.0) - SQUARE_PADDING;
    let offset = (allocation.width as f64 - square_size) / 2.;

    (
        offset + square_size * hsv.saturation as f64,
        offset + square_size * hsv.value as f64,
    )
}

fn get_handle_at(hsv: Hsv, allocation: &gtk::Allocation, x: f64, y: f64) -> Handle {
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

fn wheel_draw(hsv: Hsv, drawing_area: &gtk::DrawingArea, cr: &cairo::Context) -> gtk::Inhibit {
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
            let hue = angle;

            let col: LinSrgb = Hsv::new(hue as f32, 1., 1.).into_rgb();

            // For one reason or another GTK requires BGRA ordering without really telling anyone about it
            image::Rgba([(col.blue * 255. + 0.5) as u8, (col.green * 255. + 0.5) as u8, (col.red * 255. + 0.5) as u8, 255])
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

            let col: LinSrgb = Hsv::new(hsv.hue, xx as f32, yy as f32).into_rgb();

            image::Rgba([
                (col.blue * 255.5) as u8,
                (col.green * 255.5) as u8,
                (col.red * 255.5) as u8,
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
        wheel.set_rgb(LinSrgb::new(r as _, g as _, b as _));
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
        let rgb: LinSrgb = hsv.into_rgb();
        self.emit(COLOR_PICKED, &[&rgb.red, &rgb.green, &rgb.blue]).unwrap();
    }

    pub fn set_rgb(&self, rgb: LinSrgb) {
        let imp = ColorWheelPrivate::from_instance(self);
        let hsv = rgb.into_hsv();
        imp.hsv.set(hsv);
        self.queue_draw();
    }
}

impl Default for ColorWheel {
    fn default() -> Self {
        Self::new()
    }
}
