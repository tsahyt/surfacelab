use crate::bus;
use std::thread;

pub fn start_nodes_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        for event in receiver {
            println!("node manager thread got {}", event);
        }
    })
}
