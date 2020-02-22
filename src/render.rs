use crate::{broker, gpu, lang::*};
use std::thread;

pub fn start_render_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: &gpu::GPU<B>,
) -> thread::JoinHandle<()> {
    let (_sender, receiver) = broker.subscribe();

    thread::spawn(move || {
        log::info!("Starting Renderer");

        for event in receiver {
            match &*event {
                Lang::UserEvent(UserEvent::Quit) => break,
                _ => {}
            }
        }

        log::info!("Renderer terminating");
    })
}
