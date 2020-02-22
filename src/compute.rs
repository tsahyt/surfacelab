use crate::{broker, gpu, lang::*};
use std::thread;

pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: &gpu::GPU<B>,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (_sender, receiver) = broker.subscribe();

    thread::spawn(move || {
        for event in receiver {
            match &*event {
                Lang::UserEvent(UserEvent::Quit) => break,
                _ => {}
            }
        }

        log::info!("GPU Compute Handler terminating");
    })
}
