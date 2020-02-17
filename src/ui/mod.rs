use crate::{bus, clone, lang};
use gio::prelude::*;
use gtk::prelude::*;
use std::convert::TryFrom;
use std::rc::Rc;
use std::thread;

pub mod node;
pub mod node_area;
pub mod node_socket;
pub mod subclass;
pub mod util;

pub fn start_ui_threads(bus: &bus::Bus) -> (thread::JoinHandle<()>, thread::JoinHandle<()>) {
    log::info!("Starting UI");

    let (sender, receiver) = bus.subscribe().unwrap();

    let gtk_thread = thread::spawn(move || gtk_main(sender));
    let bus_thread = thread::spawn(move || {
        for event in receiver {
            log::trace!("UI processing event {:?}", event);
        }

        log::info!("UI Terminating");
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
        window.set_default_size(1024, 768);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 16);

        // Node Area
        let node_area = node_area::NodeArea::new();

        // Buttons
        let button_box = {
            let button_box = gtk::ButtonBox::new(gtk::Orientation::Horizontal);
            button_box.set_layout(gtk::ButtonBoxStyle::Expand);
            let new_image_node_button = gtk::Button::new_with_label("New Image Node");
            button_box.add(&new_image_node_button);

            new_image_node_button.connect_clicked(clone!(node_area => move |_| {
                let new_node = node::Node::new();
                node_area.add(&new_node);
                new_node.show();
            }));

            // let new_noise_node_button = gtk::Button::new_with_label("New Noise Node");
            // button_box.add(&new_noise_node_button);
            // let new_output_node_button = gtk::Button::new_with_label("New Output Node");
            // button_box.add(&new_output_node_button);
            button_box
        };

        // test node
        // let node = node::Node::new();
        // vbox.add(&node);

        vbox.add(&button_box);
        vbox.pack_end(&node_area, true, true, 0);

        window.add(&vbox);
        window.show_all();
    });

    application.run(&[]);
}
