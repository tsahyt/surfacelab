use std::thread;
use surfacelab::{broker, gpu, lang};

fn main() {
    env_logger::init();

    // initialize GPU before proceeding
    match gpu::initialize_gpu(true) {
        Ok(gpu) => {
            // initialize the bus
            let mut broker: broker::Broker<lang::Lang> = broker::Broker::new(1024);

            // start threads
            let ui_thread = surfacelab::ui::start_ui_thread(&mut broker);
            let nodes_thread = surfacelab::nodes::start_nodes_thread(&mut broker);
            // TODO: both threads need mutable access. gfx-hal and threading?
            let compute_thread = surfacelab::compute::start_compute_thread(&mut broker, &gpu);
            let render_thread = surfacelab::render::start_render_thread(&mut broker, &gpu);
            let _broker_runner = thread::spawn(move || broker.run());

            // wait for threads
            ui_thread.join().unwrap();
            nodes_thread.join().unwrap();
            compute_thread.join().unwrap();
            render_thread.join().unwrap();
        }
        Err(err) => log::error!("{}", err),
    }
}
