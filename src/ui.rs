use crate::bus;
use gtk::prelude::*;
use gio::prelude::*;
use std::thread;
use std::rc::Rc;

pub fn start_ui_threads(bus: &bus::Bus) -> (thread::JoinHandle<()>, thread::JoinHandle<()>) {
    let (sender, receiver) = bus.subscribe().unwrap();

    let gtk_thread = thread::spawn(move || gtk_main(sender));
    let bus_thread = thread::spawn(move || {
        for event in receiver {
            println!("ui bus thread got {}", event);
        }
    });

    (gtk_thread, bus_thread)
}

fn gtk_main(bus: bus::Sender) {
    let application = gtk::Application::new(Some("com.mechaneia.surfacelab"), Default::default())
        .expect("Failed to initialize GTK application");

    let bus_rc = Rc::new(bus);

    application.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("SurfaceLab");
        window.set_default_size(350, 70);
        let button = gtk::Button::new_with_label("Click me!");

        let bus_rc_1 = bus_rc.clone();
        button.connect_clicked(move |_| {
            bus::emit(bus_rc_1.as_ref(), "Hello world".to_string());
            println!("Clicked!");
        });

        window.add(&button);
        window.show_all();
    });

    application.run(&[]);
}
