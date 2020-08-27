use conrod_core::*;

const SCROLL_SPEED: f32 = 1.0 / 20.0;
const PAN_SPEED: f32 = 1.0 / 200.0;

#[derive(Copy, Clone, WidgetCommon)]
pub struct RenderView {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    monitor_resolution: (u32, u32),
    image_id: image::Id,
    style: Style,
}

#[derive(Copy, Clone)]
pub struct State {}

#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

#[derive(Copy, Clone, Debug)]
pub enum Event {
    Resized(u32, u32),
    Rotate(f32, f32),
    Pan(f32, f32),
    LightPan(f32, f32),
    Zoom(f32),
}

widget_ids! {
    pub struct Ids {
        image
    }
}

impl RenderView {
    /// Begin building a `RenderView`.
    pub fn new(image_id: image::Id, monitor_resolution: (u32, u32)) -> Self {
        RenderView {
            common: widget::CommonBuilder::default(),
            monitor_resolution,
            image_id,
            style: Style::default(),
        }
    }
}

impl Widget for RenderView {
    type State = Ids;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            state,
            rect,
            id,
            ui,
            ..
        } = args;
        let image_id = self.image_id;

        let (x, y, w, h) = rect.x_y_w_h();
        widget::Image::new(image_id)
            .x_y(x, y)
            .w_h(w, h)
            .source_rectangle(position::rect::Rect::from_corners(
                [0.0, self.monitor_resolution.1 as f64],
                [w, self.monitor_resolution.1 as f64 - h],
            ))
            .parent(id)
            .graphics_for(id)
            .set(state.image, ui);

        for event in ui.widget_input(id).events() {
            match event {
                event::Widget::WindowResized(_dims) => {
                    return Some(Event::Resized(w as u32, h as u32));
                }
                event::Widget::Drag(event::Drag {
                    button: input::MouseButton::Left,
                    delta_xy,
                    ..
                }) => {
                    return Some(Event::Rotate(
                        (delta_xy[0] / w) as f32,
                        (delta_xy[1] / h) as f32,
                    ));
                }
                event::Widget::Drag(event::Drag {
                    button: input::MouseButton::Middle,
                    delta_xy,
                    ..
                }) => {
                    return Some(Event::Pan(
                        delta_xy[0] as f32 * PAN_SPEED,
                        delta_xy[1] as f32 * PAN_SPEED,
                    ));
                }
                event::Widget::Drag(event::Drag {
                    button: input::MouseButton::Right,
                    delta_xy,
                    ..
                }) => {
                    return Some(Event::LightPan(
                        delta_xy[0] as f32 * PAN_SPEED,
                        delta_xy[1] as f32 * PAN_SPEED,
                    ));
                }
                event::Widget::Scroll(event::Scroll { y, .. }) => {
                    return Some(Event::Zoom(y as f32 * SCROLL_SPEED));
                }
                _ => {}
            }
        }

        None
    }
}
