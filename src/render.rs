use crate::bus;
use std::thread;

pub fn start_render_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        for event in receiver {
            log::debug!("Renderer processing event {:?}", event);
        }
    })
}
