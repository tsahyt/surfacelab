use crate::{broker, lang::*};
use std::thread;

pub fn start_io_thread(broker: &mut broker::Broker<Lang>) -> thread::JoinHandle<()> {
    let (sender, receiver, disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("io".to_string())
        .spawn(move || {
            log::info!("Starting IO manager");

            let mut io_manager = IOManager::new();

            for event in receiver {}

            log::info!("IO manager terminating");
            disconnector.disconnect();
        })
        .expect("Failed to start IO manager thread!")
}

pub struct IOManager {}

impl IOManager {
    pub fn new() -> Self {
        Self {}
    }
}
