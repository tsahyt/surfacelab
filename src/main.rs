use surfacelab::bus::Bus;

fn main() {
    env_logger::init();

    // initialize the bus
    let mut bus: Bus = Bus::new(1024);

    // start threads
    let ui_thread = surfacelab::ui::start_ui_thread(&bus);
    // FIXME: threads seem to take 100% CPU each
    let nodes_thread = surfacelab::nodes::start_nodes_thread(&bus);
    // let compute_thread = surfacelab::compute::start_compute_thread(&bus);
    // let render_thread = surfacelab::render::start_render_thread(&bus);

    // finalize bus to drop initial sender and receiver
    bus.finalize();

    // close bus to break event loops
    // drop(bus);

    // wait for threads
    ui_thread.join().unwrap();
    nodes_thread.join().unwrap();
    // compute_thread.join().unwrap();
    // render_thread.join().unwrap();

    // FIXME: application doesn't exit when GUI is closed
}
