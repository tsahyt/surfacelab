use crate::bus;
use std::thread;

pub fn start_compute_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        for event in receiver {
            println!("compute thread got {}", event);
        }
    })
}
