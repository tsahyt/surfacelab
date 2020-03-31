use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Copy, Clone, Debug)]
struct Step {
    color: [f32; 3],
    position: f32,
}

pub struct ColorRampPrivate {
    steps: Rc<RefCell<Vec<Step>>>,
    ramp_da: gtk::DrawingArea,
    ramp_adjust: Rc<Cell<Option<usize>>>,
}

impl ObjectSubclass for ColorRampPrivate {
    const NAME: &'static str = "ColorRamp";

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
            steps: Rc::new(RefCell::new(vec![
                Step {
                    color: [0., 0., 0.],
                    position: 0.,
                },
                Step {
                    color: [1., 1., 1.],
                    position: 1.,
                },
            ])),
            ramp_da: gtk::DrawingAreaBuilder::new()
                .events(
                    gdk::EventMask::BUTTON_PRESS_MASK
                        | gdk::EventMask::BUTTON_RELEASE_MASK
                        | gdk::EventMask::BUTTON1_MOTION_MASK,
                )
                .build(),
            ramp_adjust: Rc::new(Cell::new(None)),
        }
    }
}

impl ObjectImpl for ColorRampPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &Object) {
        let color_ramp = obj.clone().downcast::<ColorRamp>().unwrap();

        self.ramp_da.connect_draw(
            clone!(@strong self.steps as steps => move |w, cr| ramp_draw(&steps.borrow(), w, cr)),
        );

        self.ramp_da.connect_button_press_event(
            clone!(@strong self.ramp_adjust as ramp_adjust, @strong self.steps as steps => move |w, e| {
                let (x, y) = e.get_position();
                let handle = ramp_get_handle(&steps.borrow(), w, x as _, y as _);
                ramp_adjust.set(handle);
                Inhibit(false)
            }),
        );

        self.ramp_da.connect_button_release_event(
            clone!(@strong self.ramp_adjust as ramp_adjust => move |w, e| {
                ramp_adjust.set(None);
                Inhibit(false)
            }),
        );

        self.ramp_da.connect_motion_notify_event(
            clone!(@strong self.ramp_adjust as ramp_adjust, @strong self.steps as steps => move |w, e| {
                if let Some(handle) = ramp_adjust.get() {
                    let mut bsteps = steps.borrow_mut();
                    let step = bsteps.get_mut(handle).unwrap();
                    step.position = (e.get_position().0 / 256.) as f32;
                }
                w.queue_draw();
                Inhibit(false)
            }),
        );

        self.ramp_da.set_size_request(256, 64);
        color_ramp.pack_end(&self.ramp_da, true, false, 0);
    }
}

impl WidgetImpl for ColorRampPrivate {}

impl ContainerImpl for ColorRampPrivate {}

impl BoxImpl for ColorRampPrivate {}

impl ColorRampPrivate {}

const HANDLE_SIZE: f64 = 6.;

fn ramp_draw(ramp: &[Step], da: &gtk::DrawingArea, cr: &cairo::Context) -> gtk::Inhibit {
    let allocation = da.get_allocation();

    // padded geometry
    let padding = 16.;
    let width = allocation.width as f64 - padding;
    let start_x = padding / 2.;
    let start_y = padding / 2.;

    // draw the gradient
    let grad = cairo::LinearGradient::new(start_x, start_y, width, 0.);
    for step in ramp {
        grad.add_color_stop_rgba(
            step.position as _,
            step.color[0] as _,
            step.color[1] as _,
            step.color[2] as _,
            1.0,
        );
    }
    cr.set_source(&grad);
    cr.rectangle(start_x, start_y, width, 32. + start_y);
    cr.fill();
    cr.set_source_rgba(0., 0., 0., 1.);
    cr.rectangle(start_x, start_y, width, 32. + start_y);
    cr.stroke();

    // draw the position
    cr.set_source_rgba(0., 0., 0., 1.);
    for step in ramp {
        let x = width * step.position as f64 + start_x;
        cr.move_to(x, 24. + start_y);
        cr.rel_line_to(0., 24.);
        cr.stroke();
        cr.arc(x, 48. + start_y, HANDLE_SIZE, 0., std::f64::consts::TAU);
        cr.fill();
    }

    Inhibit(false)
}

fn ramp_get_handle(
    ramp: &[Step],
    da: &gtk::DrawingArea,
    cursor_x: f64,
    cursor_y: f64,
) -> Option<usize> {
    let allocation = da.get_allocation();

    // padded geometry
    let padding = 16.;
    let width = allocation.width as f64 - padding;
    let start_x = padding / 2.;
    let start_y = padding / 2.;

    const HANDLE_ZONE: f64 = HANDLE_SIZE / 1.5;

    for (i, step) in ramp.iter().enumerate() {
        let x = width * step.position as f64 + start_x;
        let y = start_y + 48.;

        if cursor_x > x - HANDLE_ZONE
            && cursor_x < x + HANDLE_ZONE
            && cursor_y > y - HANDLE_ZONE
            && cursor_y < y + HANDLE_ZONE
        {
            return Some(i);
        }
    }

    None
}

glib_wrapper! {
    pub struct ColorRamp(
        Object<subclass::simple::InstanceStruct<ColorRampPrivate>,
        subclass::simple::ClassStruct<ColorRampPrivate>,
        ColorRampClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

    match fn {
        get_type => || ColorRampPrivate::get_type().to_glib(),
    }
}

impl ColorRamp {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }
}

impl Default for ColorRamp {
    fn default() -> Self {
        Self::new()
    }
}
