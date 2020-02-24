use crate::{broker, gpu, lang::*};
use std::sync::{Arc, Mutex};
use std::thread;

pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (_sender, receiver) = broker.subscribe();
    match gpu::create_compute(gpu) {
        Err(e) => {
            log::error!("Failed to initialize GPU Compute: {}", e);
            panic!("Critical Error");
        }
        Ok(mut compute) => thread::spawn(move || {
            let cb = compute.primary_command_buffer();
            for event in receiver {
                match &*event {
                    Lang::UserEvent(UserEvent::Quit) => break,
                    _ => {}
                }
            }

            log::info!("GPU Compute Handler terminating");
        }),
    }
}
