use crate::{broker, gpu, lang::*};

use winit::platform::unix::EventLoopExtUnix;

use std::sync::{Arc, Mutex};
use std::thread;

use conrod_core::{
    self, widget, widget_ids, Colorable, Labelable, Positionable, UiCell, Widget,
};

conrod_winit::v021_conversion_fns!();

widget_ids!(
    struct Ids { text, button, counter }
);

struct App {
    clicks: u32,
}

fn gui(ui: &mut UiCell, ids: &Ids, app: &mut App) {
    widget::Text::new("Hello World!")
        .middle_of(ui.window)
        .color(conrod_core::color::WHITE)
        .font_size(32)
        .set(ids.text, ui);
    for _press in widget::Button::new().label("Press").set(ids.button, ui) {
        app.clicks += 1;
    }
    widget::Text::new(&format!("Times Clicked: {}", app.clicks))
        .color(conrod_core::color::GRAY)
        .font_size(16)
        .set(ids.counter, ui);
}

const DIMS: gpu::Extent2D = gpu::Extent2D {
    width: 1920,
    height: 1080,
};

fn ui_loop<B: gpu::Backend>(gpu: Arc<Mutex<gpu::GPU<B>>>) {
    let event_loop: winit::event_loop::EventLoop<()> = winit::event_loop::EventLoop::new_any_thread();

    let window = winit::window::WindowBuilder::new()
        .with_min_inner_size(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
            64.0, 64.0,
        )))
        .with_inner_size(winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(
            DIMS.width,
            DIMS.height,
        )))
        .with_title("quad".to_string())
        .build(&event_loop).unwrap();

    let mut renderer = gpu::ui::Renderer::new(gpu, &window, DIMS, [1024, 1024]);

    // conrod
    let mut app = App { clicks: 0 };
    let mut ui = conrod_core::UiBuilder::new([DIMS.width as f64, DIMS.height as f64]).build();
    let ids = Ids::new(ui.widget_id_generator());
    let image_map = conrod_core::image::Map::new();

    ui.fonts
        .insert_from_file("/home/paul/.local/share/fonts/Recursive/static/Recursive-Medium-CASL=0-CRSV=0-MONO=0-slnt=0.ttf")
        .unwrap();

    // It is important that the closure move captures the Renderer,
    // otherwise it will not be dropped when the event loop exits.
    event_loop.run(move |event, _, control_flow| {
        if let Some(event) = convert_event(&event, &window) {
            ui.handle_event(event);
        }

        *control_flow = winit::event_loop::ControlFlow::Wait;

        match event {
            winit::event::Event::WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::CloseRequested => {
                    *control_flow = winit::event_loop::ControlFlow::Exit
                }
                winit::event::WindowEvent::KeyboardInput {
                    input:
                        winit::event::KeyboardInput {
                            virtual_keycode: Some(winit::event::VirtualKeyCode::Escape),
                            ..
                        },
                    ..
                } => *control_flow = winit::event_loop::ControlFlow::Exit,
                winit::event::WindowEvent::Resized(dims) => {
                    renderer.recreate_swapchain(Some(gpu::Extent2D {
                        width: dims.width,
                        height: dims.height,
                    }));
                }
                _ => {}
            },

            winit::event::Event::MainEventsCleared => {
                // Update widgets if any event has happened
                if ui.global_input().events().next().is_some() {
                    let mut ui = ui.set_widgets();
                    gui(&mut ui, &ids, &mut app);
                    window.request_redraw();
                }
            }

            winit::event::Event::RedrawRequested(..) => {
                let primitives = match ui.draw_if_changed() {
                    None => return,
                    Some(ps) => ps,
                };

                renderer.render(&image_map, primitives);
            }
            _ => {}
        }
    });
}

pub fn start_ui_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    let (_sender, receiver, disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("ui".to_string())
        .spawn(move || {
            ui_loop(gpu)
        })
        .expect("Failed to spawn UI thread!")
}
