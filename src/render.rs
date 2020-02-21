use crate::{broker, lang};
use std::thread;

pub fn start_render_thread(broker: &mut broker::Broker<lang::Lang>) -> thread::JoinHandle<()> {
    let (_sender, receiver) = broker.subscribe();

    thread::spawn(move || {
        log::info!("Starting Renderer");

        for event in receiver {
            log::trace!("Renderer processing event {:?}", event);
        }

        log::info!("Renderer terminating");
    })
}
