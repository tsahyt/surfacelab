use surfacelab::bus::{Bus, Lang};

fn main() {
    // initialize the bus
    let mut bus: Bus<Lang> = Bus::new(1024);

    // start threads
    let (gtk_thread, ui_thread) = surfacelab::ui::start_ui_threads(&bus);

    bus.finalize();
    bus.emit("hello world".to_string());

    // close bus to break event loops
    drop(bus);

    // wait for threads
    ui_thread.join().unwrap();
    gtk_thread.join().unwrap();
}
