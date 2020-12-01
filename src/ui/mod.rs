use crate::{broker, gpu, lang::*};

use winit::platform::unix::EventLoopExtUnix;

use std::sync::{Arc, Mutex};
use std::thread;

pub mod app;
pub mod app_state;
pub mod color_picker;
pub mod color_ramp;
pub mod export_row;
pub mod exposed_param_row;
pub mod graph;
pub mod i18n;
pub mod layer_row;
pub mod modal;
pub mod node;
pub mod param_box;
pub mod renderview;
pub mod tabs;
pub mod util;

conrod_winit::v023_conversion_fns!();

const DIMS: gpu::Extent2D = gpu::Extent2D {
    width: 1920,
    height: 1080,
};

fn ui_loop<B: gpu::Backend>(
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    sender: broker::BrokerSender<Lang>,
    receiver: broker::BrokerReceiver<Lang>,
) {
    let event_loop: winit::event_loop::EventLoop<()> =
        winit::event_loop::EventLoop::new_any_thread();

    let window = winit::window::WindowBuilder::new()
        .with_inner_size(winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(
            DIMS.width,
            DIMS.height,
        )))
        .with_title("SurfaceLab".to_string())
        .build(&event_loop)
        .unwrap();

    let monitor_size = window
        .available_monitors()
        .map(|m| m.size())
        .next()
        .unwrap();

    let mut renderer = gpu::ui::Renderer::new(gpu, &window, DIMS, [1024, 1024]);
    let mut ui = conrod_core::UiBuilder::new([DIMS.width as f64, DIMS.height as f64]).build();
    let assets = find_folder::Search::KidsThenParents(3, 5)
        .for_folder("assets")
        .unwrap();

    let fonts = app_state::AppFonts {
        icon_font: ui
            .fonts
            .insert_from_file(assets.join("MaterialDesignIcons.ttf"))
            .unwrap(),
        text_font: ui
            .fonts
            .insert_from_file(assets.join("Recursive-Regular.ttf"))
            .unwrap(),
    };

    let mut gui = app::Gui::new(
        app::Ids::new(ui.widget_id_generator()),
        fonts,
        sender,
        (monitor_size.width, monitor_size.height),
        conrod_core::image::Map::new(),
    );

    // It is important that the closure move captures the Renderer,
    // otherwise it will not be dropped when the event loop exits.
    event_loop.run(move |event, _, control_flow| {
        if let Some(event) = convert_event(&event, &window) {
            ui.handle_event(event);
        }

        *control_flow = winit::event_loop::ControlFlow::Wait;

        if let Ok(broker_event) = receiver.try_recv() {
            gui.handle_event(&mut ui, &mut renderer, &*broker_event);
        }

        match event {
            winit::event::Event::WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::CloseRequested => {
                    *control_flow = winit::event_loop::ControlFlow::Exit
                }
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
                    gui.update_gui(&mut ui);
                    window.request_redraw();
                }
            }

            winit::event::Event::RedrawRequested(..) => {
                let primitives = match ui.draw_if_changed() {
                    None => return,
                    Some(ps) => ps,
                };

                renderer.render(&gui.image_map(), primitives);
            }
            _ => {}
        }
    });
}

pub fn start_ui_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    let (sender, receiver, _disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("ui".to_string())
        .spawn(move || ui_loop(gpu, sender, receiver))
        .expect("Failed to spawn UI thread!")
}
