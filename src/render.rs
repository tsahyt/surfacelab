use crate::bus;
use std::thread;

pub fn start_render_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        log::info!("Starting Renderer");

        for event in receiver {
            log::trace!("Renderer processing event {:?}", event);
        }

        log::info!("Renderer terminating");
    })
}
