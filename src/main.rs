use surfacelab::bus::Bus;

fn main() {
    env_logger::init();

    // initialize the bus
    let mut bus: Bus = Bus::new(1024);

    // start threads
    let (gtk_thread, ui_thread) = surfacelab::ui::start_ui_threads(&bus);
    let nodes_thread = surfacelab::nodes::start_nodes_thread(&bus);
    let compute_thread = surfacelab::compute::start_compute_thread(&bus);
    let render_thread = surfacelab::render::start_render_thread(&bus);

    // finalize bus to drop initial sender and receiver
    bus.finalize();

    // close bus to break event loops
    // drop(bus);

    // wait for threads
    ui_thread.join().unwrap();
    nodes_thread.join().unwrap();
    compute_thread.join().unwrap();
    render_thread.join().unwrap();
    gtk_thread.join().unwrap();
}
