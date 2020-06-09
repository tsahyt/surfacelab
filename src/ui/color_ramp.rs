use gdk::prelude::*;
use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct Step {
    color: [f32; 3],
    position: f32,
}

impl Default for Step {
    fn default() -> Self {
        Step {
            color: [0.5, 0.5, 0.5],
            position: 0.5,
        }
    }
}

// Signals
pub const COLOR_RAMP_CHANGED: &str = "color-ramp-changed";

pub struct ColorRampPrivate {
    steps: Rc<RefCell<Vec<Step>>>,
    ramp_da: gtk::DrawingArea,
    selected_handle: Rc<Cell<Option<usize>>>,
    wheel: super::color_wheel::ColorWheel,
    add_handle_button: gtk::Button,
    remove_handle_button: gtk::Button,
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
    fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {
        class.add_signal(
            COLOR_RAMP_CHANGED,
            glib::SignalFlags::empty(),
            &[],
            glib::types::Type::Unit,
        );
    }

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
                .width_request(256)
                .height_request(64)
                .events(
                    gdk::EventMask::BUTTON_PRESS_MASK
                        | gdk::EventMask::BUTTON_RELEASE_MASK
                        | gdk::EventMask::BUTTON1_MOTION_MASK,
                )
                .build(),
            selected_handle: Rc::new(Cell::new(Some(0))),
            wheel: super::color_wheel::ColorWheel::new(),
            add_handle_button: gtk::Button::new_with_label("Add"),
            remove_handle_button: gtk::Button::new_with_label("Remove"),
        }
    }
}

impl ObjectImpl for ColorRampPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &Object) {
        let color_ramp = obj.downcast_ref::<ColorRamp>().unwrap();
        color_ramp
            .clone()
            .upcast::<gtk::Box>()
            .set_orientation(gtk::Orientation::Vertical);

        self.ramp_da
            .connect_draw(clone!(@strong self.steps as steps,
                   @strong self.selected_handle as selected_handle => move |w, cr|
                   ramp_draw(&steps.borrow(), selected_handle.get(), w, cr)));

        self.ramp_da.connect_button_press_event(
            clone!(@strong self.selected_handle as selected_handle,
                   @strong self.steps as steps,
                   @strong color_ramp as color_ramp,
                   @strong self.wheel as wheel => move |w, e| {
                let (x, y) = e.get_position();
                let handle = ramp_get_handle(&steps.borrow(), w, x as _, y as _);
                selected_handle.set(handle);

                if let Some(handle_idx) = handle {
                    let rgb = steps.borrow()[handle_idx].color;
                    wheel.set_rgb(rgb[0].into(), rgb[1].into(), rgb[2].into());
                }
                color_ramp.emit(COLOR_RAMP_CHANGED, &[]).unwrap();
                Inhibit(false)
            }),
        );

        self.ramp_da.connect_motion_notify_event(
            clone!(@strong self.selected_handle as selected_handle,
                   @strong color_ramp as color_ramp,
                   @strong self.steps as steps => move |w, e| {
                if let Some(handle) = selected_handle.get() {
                    {
                        let mut bsteps = steps.borrow_mut();
                        let step = bsteps.get_mut(handle).unwrap();
                        step.position = ((e.get_position().0 / 256.) as f32).clamp(0.0, 1.0);
                        w.queue_draw();
                    }
                    color_ramp.emit(COLOR_RAMP_CHANGED, &[]).unwrap();
                }
                Inhibit(false)
            }),
        );

        self.wheel
            .connect_color_picked(clone!(@strong self.steps as steps,
            @strong self.selected_handle as selected_handle,
            @strong color_ramp as color_ramp,
            @strong self.ramp_da as ramp => move |_, r, g, b| {
             if let Some(handle) = selected_handle.get() {
                 {
                    let mut steps_data = steps.borrow_mut();
                    steps_data[handle].color[0] = r as f32;
                    steps_data[handle].color[1] = g as f32;
                    steps_data[handle].color[2] = b as f32;
                    ramp.queue_draw();
                 }
                 color_ramp.emit(COLOR_RAMP_CHANGED, &[]).unwrap();
             }
            }));

        color_ramp.pack_start(&self.ramp_da, true, false, 8);
        let aspect_frame = gtk::AspectFrame::new(None, 0.5, 0.5, 1., true);
        aspect_frame.add(&self.wheel);
        aspect_frame.set_shadow_type(gtk::ShadowType::None);
        color_ramp.pack_end(&aspect_frame, true, false, 8);

        self.add_handle_button
            .connect_clicked(clone!(@strong self.steps as steps,
                                    @strong color_ramp as color_ramp => move |_| {
                steps.borrow_mut().push(Step::default());
                color_ramp.emit(COLOR_RAMP_CHANGED, &[]).unwrap();
            }));

        self.remove_handle_button
            .connect_clicked(clone!(@strong self.steps as steps,
                                    @strong color_ramp as color_ramp,
                       @strong self.selected_handle as selected_handle => move |_| {
                if let Some(handle) = selected_handle.get() {
                    steps.borrow_mut().remove(handle);
                    selected_handle.set(Some(0));
                    color_ramp.emit(COLOR_RAMP_CHANGED, &[]).unwrap();
                }
            }));

        let button_box = gtk::ButtonBoxBuilder::new()
            .layout_style(gtk::ButtonBoxStyle::Expand)
            .build();
        button_box.add(&self.add_handle_button);
        button_box.add(&self.remove_handle_button);
        color_ramp.pack_start(&button_box, true, true, 8);
    }
}

impl WidgetImpl for ColorRampPrivate {}

impl ContainerImpl for ColorRampPrivate {}

impl BoxImpl for ColorRampPrivate {}

impl ColorRampPrivate {}

const HANDLE_SIZE: f64 = 6.;

fn ramp_draw(
    ramp: &[Step],
    selected_handle: Option<usize>,
    da: &gtk::DrawingArea,
    cr: &cairo::Context,
) -> gtk::Inhibit {
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

    // draw the position
    cr.set_source_rgba(0., 0., 0., 1.);
    for (i, step) in ramp.iter().enumerate() {
        let x = width * step.position as f64 + start_x;
        cr.move_to(x, 24. + start_y);
        cr.rel_line_to(0., 24.);
        cr.stroke();
        cr.set_source_rgb(
            step.color[0].into(),
            step.color[1].into(),
            step.color[2].into(),
        );
        cr.arc(x, 48. + start_y, HANDLE_SIZE, 0., std::f64::consts::TAU);
        cr.fill();
        if Some(i) == selected_handle {
            cr.set_line_width(4.5);
            cr.set_source_rgb(0.9, 0.9, 0.9);
            cr.arc(x, 48. + start_y, HANDLE_SIZE, 0., std::f64::consts::TAU);
            cr.stroke();
        }
        cr.set_line_width(1.5);
        cr.set_source_rgb(0., 0., 0.);
        cr.arc(x, 48. + start_y, HANDLE_SIZE, 0., std::f64::consts::TAU);
        cr.stroke();
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

    pub fn new_with_steps(steps: &[[f32; 4]]) -> Self {
        let ramp = Self::new();
        {
            let imp = ColorRampPrivate::from_instance(&ramp);
            let mut imp_steps = imp.steps.borrow_mut();

            *imp_steps = steps
                .iter()
                .map(|x| Step {
                    color: [x[0], x[1], x[2]],
                    position: x[3],
                })
                .collect();
        }
        ramp
    }

    pub fn get_ramp(&self) -> Vec<[f32; 4]> {
        let imp = ColorRampPrivate::from_instance(self);
        imp.steps
            .borrow()
            .iter()
            .map(|step| [step.color[0], step.color[1], step.color[2], step.position])
            .collect()
    }

    pub fn connect_color_ramp_changed<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local(COLOR_RAMP_CHANGED, true, move |w| {
            let color_wheel = w[0].downcast_ref::<ColorRamp>().unwrap().get().unwrap();
            f(&color_wheel);
            None
        })
        .unwrap()
    }
}

impl Default for ColorRamp {
    fn default() -> Self {
        Self::new()
    }
}
