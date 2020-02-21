use surfacelab::{broker, lang};

fn main() {
    env_logger::init();

    // initialize the bus
    let mut broker: broker::Broker<lang::Lang> = broker::Broker::new(1024);

    // start threads
    let ui_thread = surfacelab::ui::start_ui_thread(&mut broker);
    let nodes_thread = surfacelab::nodes::start_nodes_thread(&mut broker);
    let compute_thread = surfacelab::compute::start_compute_thread(&mut bus);
    let render_thread = surfacelab::render::start_render_thread(&mut bus);

    // wait for threads
    ui_thread.join().unwrap();
    nodes_thread.join().unwrap();
    compute_thread.join().unwrap();
    render_thread.join().unwrap();

    // FIXME: application doesn't exit when GUI is closed
}
