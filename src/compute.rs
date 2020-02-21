use crate::{broker, lang};
use std::thread;

pub fn start_compute_thread(broker: &mut broker::Broker<lang::Lang>) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (_sender, receiver) = broker.subscribe();

    thread::spawn(move || {
        for event in receiver {
            log::trace!("Compute processing event {:?}", event);
        }

        log::info!("GPU Compute Handler terminating");
    })
}
