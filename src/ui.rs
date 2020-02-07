use crate::bus::{Bus, Lang};
use gtk::prelude::*;
use gio::prelude::*;
use std::thread;

pub fn start_ui_threads(bus: &Bus<Lang>) -> (thread::JoinHandle<()>, thread::JoinHandle<()>) {
    let stream_consumer = bus.subscribe().unwrap().clone();

    let gtk_thread = thread::spawn(|| gtk_main(bus));
    let bus_thread = thread::spawn(move || {
        for event in stream_consumer {
            println!("got {}", event);
        }
    });

    (gtk_thread, bus_thread)
}

fn gtk_main(bus: &Bus<Lang>) {
    let application = gtk::Application::new(Some("com.mechaneia.surfacelab"), Default::default())
        .expect("Failed to initialize GTK application");

    application.connect_activate(|app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("SurfaceLab");
        window.set_default_size(350, 70);
        let button = gtk::Button::new_with_label("Click me!");
        button.connect_clicked(|_| {
            println!("Clicked!");
        });

        window.add(&button);
        window.show_all();
    });

    application.run(&[]);
}
