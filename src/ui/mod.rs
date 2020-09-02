use crate::{broker, gpu, lang::*};

use winit::platform::unix::EventLoopExtUnix;

use std::sync::{Arc, Mutex};
use std::thread;

pub mod app;
pub mod graph;
pub mod node;
pub mod param_box;
pub mod renderview;
pub mod util;

conrod_winit::v021_conversion_fns!();

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
        .with_min_inner_size(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
            64.0, 64.0,
        )))
        .with_inner_size(winit::dpi::Size::Physical(winit::dpi::PhysicalSize::new(
            DIMS.width,
            DIMS.height,
        )))
        .with_title("quad".to_string())
        .build(&event_loop)
        .unwrap();

    let monitor_size = window.primary_monitor().size();

    let mut renderer = gpu::ui::Renderer::new(gpu, &window, DIMS, [1024, 1024]);

    let mut app = app::App::new(sender, (monitor_size.width, monitor_size.height));

    let mut ui = conrod_core::UiBuilder::new([DIMS.width as f64, DIMS.height as f64]).build();
    let ids = app::Ids::new(ui.widget_id_generator());
    let mut image_map = conrod_core::image::Map::new();
    let assets = find_folder::Search::KidsThenParents(3, 5)
        .for_folder("assets")
        .unwrap();

    let fonts = app::AppFonts {
        icon_font: ui
            .fonts
            .insert_from_file(assets.join("MaterialDesignIcons.ttf"))
            .unwrap(),
        text_font: ui
            .fonts
            .insert_from_file(assets.join("Recursive-Regular.ttf"))
            .unwrap(),
    };

    // It is important that the closure move captures the Renderer,
    // otherwise it will not be dropped when the event loop exits.
    event_loop.run(move |event, _, control_flow| {
        if let Some(event) = convert_event(&event, &window) {
            ui.handle_event(event);
        }

        *control_flow = winit::event_loop::ControlFlow::Wait;

        if let Ok(broker_event) = receiver.try_recv() {
            match &*broker_event {
                Lang::RenderEvent(RenderEvent::RendererAdded(_id, view)) => {
                    let id = image_map.insert(renderer.create_image(
                        view.to::<B>(),
                        app.monitor_resolution.0,
                        app.monitor_resolution.1,
                    ));
                    app.render_image = Some(id);
                }
                Lang::RenderEvent(RenderEvent::RendererRedrawn(_id)) => {
                    ui.needs_redraw();
                }
                Lang::ComputeEvent(ComputeEvent::ThumbnailCreated(res, thmb)) => {
                    let id = image_map.insert(renderer.create_image(thmb.to::<B>(), 128, 128));
                    app.register_thumbnail(&res.drop_fragment(), id);
                }
                Lang::GraphEvent(ev) => app.handle_graph_event(ev),
                _ => {}
            }
        }

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
                    app::gui(&mut ui, &ids, &fonts, &mut app);
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
    let (sender, receiver, _disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("ui".to_string())
        .spawn(move || ui_loop(gpu, sender, receiver))
        .expect("Failed to spawn UI thread!")
}
