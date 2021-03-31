use crate::{broker, gpu, lang::*};

use conrod_core::{widget::Widget, widget_ids};
use winit::platform::unix::EventLoopExtUnix;

use std::sync::{Arc, Mutex};
use std::thread;

pub mod app_state;
pub mod components;
pub mod i18n;
pub mod util;
pub mod widgets;

conrod_winit::v023_conversion_fns!();

widget_ids! {
    struct Ids {
        application
    }
}

/// Set up and run the main UI loop
fn ui_loop<B: gpu::Backend>(
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    sender: broker::BrokerSender<Lang>,
    receiver: broker::BrokerReceiver<Lang>,
    window_size: (u32, u32),
) {
    // Initialize event loop in thread. This is possible on Linux and Windows,
    // but would be a blocker for macOS.
    let event_loop: winit::event_loop::EventLoop<()> =
        winit::event_loop::EventLoop::new_any_thread();

    let window = winit::window::WindowBuilder::new()
        .with_inner_size(winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(
            window_size.0,
            window_size.1,
        )))
        .with_title("SurfaceLab".to_string())
        .build(&event_loop)
        .unwrap();

    let monitor_size = window
        .available_monitors()
        .map(|m| m.size())
        .next()
        .unwrap();

    let dims = gpu::Extent2D {
        width: window_size.0,
        height: window_size.1,
    };
    let mut renderer =
        gpu::ui::Renderer::new(gpu, &window, dims, [1024, 1024]).expect("Error building renderer");
    let mut ui = conrod_core::UiBuilder::new([dims.width as f64, dims.height as f64]).build();
    let assets = find_folder::Search::KidsThenParents(3, 5)
        .for_folder("assets")
        .unwrap();

    let icon_font = ui
        .fonts
        .insert_from_file(assets.join("MaterialDesignIcons.ttf"))
        .expect("Missing icon font!");
    let text_font = ui
        .fonts
        .insert_from_file(assets.join("Recursive-Regular.ttf"))
        .expect("Missing UI font!");

    // Initialize GUI Application Data
    let mut app_data = components::app::ApplicationData::new(
        sender,
        conrod_core::image::Map::new(),
        (monitor_size.width, monitor_size.height),
    );

    // Initialize top level ids
    let ids = Ids::new(ui.widget_id_generator());

    let mut event_buffer = Vec::with_capacity(8);

    // It is important that the closure move captures the Renderer,
    // otherwise it will not be dropped when the event loop exits.
    event_loop.run(move |event, _, control_flow| {
        if let Some(event) = convert_event(&event, &window) {
            ui.handle_event(event);
        }

        *control_flow = winit::event_loop::ControlFlow::Wait;

        // Buffer all events from the receiver
        while let Ok(broker_event) = receiver.try_recv() {
            event_buffer.push(broker_event);
        }

        match event {
            winit::event::Event::WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::CloseRequested => {
                    *control_flow = winit::event_loop::ControlFlow::Exit
                }
                winit::event::WindowEvent::Resized(dims) => {
                    renderer
                        .recreate_swapchain(Some(gpu::Extent2D {
                            width: dims.width,
                            height: dims.height,
                        }))
                        .expect("Swapchain recreation failed");
                }
                _ => {}
            },

            winit::event::Event::MainEventsCleared => {
                // Update widgets if any event has happened
                if ui.global_input().events().next().is_some() {
                    let mut ui = ui.set_widgets();
                    components::app::Application::new(&mut app_data, &mut renderer)
                        .event_buffer(&event_buffer)
                        .text_font(text_font)
                        .icon_font(icon_font)
                        .panel_color(conrod_core::color::DARK_CHARCOAL)
                        .panel_gap(0.5)
                        .set(ids.application, &mut ui);
                    window.request_redraw();
                    event_buffer.clear();
                }
            }

            winit::event::Event::RedrawRequested(..) => {
                let primitives = match ui.draw_if_changed() {
                    None => return,
                    Some(ps) => ps,
                };

                renderer
                    .render(&app_data.image_map(), primitives)
                    .expect("Rendering failed");
            }

            winit::event::Event::LoopDestroyed => {
                app_data
                    .sender
                    .send(Lang::UserIOEvent(UserIOEvent::Quit))
                    .unwrap();
            }

            _ => {}
        }
    });
}

/// Spawn the UI thread.
///
/// Requires GPU access to render the UI and to have access to thumbnails and
/// the render images
pub fn start_ui_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    window_size: (u32, u32),
) -> thread::JoinHandle<()> {
    let (sender, receiver, _disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("ui".to_string())
        .spawn(move || ui_loop(gpu, sender, receiver, window_size))
        .expect("Failed to spawn UI thread!")
}
