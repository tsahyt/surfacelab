use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Copy, Clone, Debug)]
struct Step {
    color: [f32; 3],
    position: f32,
}

pub struct ColorRampPrivate {
    steps: Rc<RefCell<Vec<Step>>>,
    ramp_da: gtk::DrawingArea,
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
            ramp_da: gtk::DrawingArea::new(),
        }
    }
}

impl ObjectImpl for ColorRampPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &Object) {
        let color_ramp = obj.clone().downcast::<ColorRamp>().unwrap();

        self.ramp_da.connect_draw(
            clone!(@strong self.steps as steps => move |w, cr| Self::draw_ramp(&steps.borrow(), w, cr)),
        );

        self.ramp_da.set_size_request(256, 64);
        color_ramp.pack_end(&self.ramp_da, true, false, 0);
    }
}

impl WidgetImpl for ColorRampPrivate {}

impl ContainerImpl for ColorRampPrivate {}

impl BoxImpl for ColorRampPrivate {}

impl ColorRampPrivate {
    fn draw_ramp(ramp: &[Step], da: &gtk::DrawingArea, cr: &cairo::Context) -> gtk::Inhibit {
        let grad = cairo::LinearGradient::new(0., 0., 256., 1.);

        for step in ramp {
            grad.add_color_stop_rgba(
                step.position as _,
                step.color[0] as _,
                step.color[1] as _,
                step.color[2] as _,
                1.0
            );
        }

        cr.set_source(&grad);
        cr.rectangle(0., 0., 256., 32.);
        cr.fill();
        Inhibit(false)
    }
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
