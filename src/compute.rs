use crate::{broker, lang::*};
use std::thread;

pub fn start_compute_thread(broker: &mut broker::Broker<Lang>) -> thread::JoinHandle<()> {
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
