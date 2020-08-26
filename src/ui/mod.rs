use crate::{broker, gpu, lang::*};

use winit::platform::unix::EventLoopExtUnix;

use std::sync::{Arc, Mutex};
use std::thread;

pub mod app;

conrod_winit::v021_conversion_fns!();

const DIMS: gpu::Extent2D = gpu::Extent2D {
    width: 1920,
    height: 1080,
};

fn ui_loop<B: gpu::Backend>(gpu: Arc<Mutex<gpu::GPU<B>>>) {
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

    let mut renderer = gpu::ui::Renderer::new(gpu, &window, DIMS, [1024, 1024]);

    // Demo Graph.
    let mut graph = petgraph::Graph::new();
    let a = graph.add_node("A");
    let b = graph.add_node("B");
    let c = graph.add_node("C");
    let d = graph.add_node("D");
    let e = graph.add_node("E");
    graph.extend_with_edges(&[
        (a, c, (1, 0)),
        (a, d, (0, 1)),
        (b, d, (0, 0)),
        (c, d, (0, 2)),
        (d, e, (0, 0)),
    ]);

    // Construct a starting layout for the nodes.
    let mut layout_map = std::collections::HashMap::new();
    layout_map.insert(b, [-100.0, 100.0]);
    layout_map.insert(a, [-300.0, 0.0]);
    layout_map.insert(c, [-100.0, -100.0]);
    layout_map.insert(d, [100.0, 0.0]);
    layout_map.insert(e, [300.0, 0.0]);
    let layout = conrod_core::widget::graph::Layout::from(layout_map);

    let mut app = app::App {
        graph,
        graph_layout: layout
    };

    let mut ui = conrod_core::UiBuilder::new([DIMS.width as f64, DIMS.height as f64]).build();
    let ids = app::Ids::new(ui.widget_id_generator());
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
                    app::gui(&mut ui, &ids, &mut app);
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
    let (_sender, _receiver, _disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("ui".to_string())
        .spawn(move || ui_loop(gpu))
        .expect("Failed to spawn UI thread!")
}
