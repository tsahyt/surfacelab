use crate::{broker, gpu, lang::*};
use std::sync::{Arc, Mutex};
use std::thread;

pub fn start_render_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    let (_sender, receiver) = broker.subscribe();
    match gpu::render::GPURender::new(gpu) {
        Err(e) => {
            log::error!("Failed to initialize GPU Render: {}", e);
            panic!("Critical Error");
        }
        Ok(mut render) => thread::spawn(move || {
            log::info!("Starting Renderer");

            for event in receiver {
                match &*event {
                    Lang::UserEvent(UserEvent::Quit) => break,
                    _ => {}
                }
            }

            log::info!("Renderer terminating");
        }),
    }
}
