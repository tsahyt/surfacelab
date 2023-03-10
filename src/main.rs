use std::thread;
use surfacelab::{
    broker, gpu,
    lang::{self, config::Configuration},
};

fn main() {
    env_logger::init();

    // initialize GPU before proceeding
    match gpu::initialize_gpu(true) {
        Ok(gpu) => {
            // initialize the bus
            let mut broker: broker::Broker<lang::Lang> = broker::Broker::new(1024);

            // read config file from known location or use default
            let config = Configuration::load_from_file("config.toml")
                .unwrap_or_else(|_| Configuration::default());

            // start threads
            let ui_thread = surfacelab::ui::start_ui_thread(&mut broker, gpu.clone(), &config);
            let compute_thread =
                surfacelab::compute::start_compute_thread(&mut broker, gpu.clone(), &config);
            let io_thread = surfacelab::io::start_io_thread(&mut broker, config);
            let undo_thread = surfacelab::undo::start_undo_thread(&mut broker);
            let nodes_thread = surfacelab::nodes::start_nodes_thread(&mut broker);
            let render_thread = surfacelab::render::start_render_thread(&mut broker, gpu);
            let _broker_runner = thread::spawn(move || broker.run());

            // wait for threads
            ui_thread.join().unwrap();
            io_thread.join().unwrap();
            undo_thread.join().unwrap();
            nodes_thread.join().unwrap();
            compute_thread.join().unwrap();
            render_thread.join().unwrap();
        }
        Err(err) => log::error!("{:?}", err),
    }
}
