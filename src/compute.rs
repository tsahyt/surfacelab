use crate::bus;
use std::thread;

pub fn start_compute_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        for event in receiver {
            log::trace!("Compute processing event {:?}", event);
        }

        log::info!("GPU Compute Handler terminating");
    })
}
