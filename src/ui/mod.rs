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
            let new_image_node_button = gtk::Button::new_with_label("New Node");
            button_box.add(&new_image_node_button);

            new_image_node_button.connect_clicked(clone!(node_area => move |_| {
                let new_node = node::Node::new();
                new_node.add_socket(uriparse::URI::try_from("node:foo#socket_in").unwrap(), node_socket::NodeSocketIO::Sink);
                new_node.add_socket(uriparse::URI::try_from("node:foo#socket_out").unwrap(), node_socket::NodeSocketIO::Source);
                node_area.add(&new_node);
                new_node.show_all();
            }));

            button_box
        };

        {
            let socket_box = gtk::Fixed::new();

            let node_socket1 = node_socket::NodeSocket::new();
            node_socket1.set_io(node_socket::NodeSocketIO::Source);
            node_socket1.set_socket_uri(uriparse::URI::try_from("node:foo#socket1").unwrap());
            let node_socket2 = node_socket::NodeSocket::new();
            node_socket2.set_io(node_socket::NodeSocketIO::Sink);
            node_socket2.set_socket_uri(uriparse::URI::try_from("node:bar#socket2").unwrap());
            socket_box.put(&node_socket1, 512, 0);
            socket_box.put(&node_socket2, 780, 0);

            let node_1 = node::Node::new();
            node_1.add_socket(uriparse::URI::try_from("node:foo#socket_in").unwrap(), node_socket::NodeSocketIO::Sink);
            node_1.add_socket(uriparse::URI::try_from("node:foo#socket_out").unwrap(), node_socket::NodeSocketIO::Source);
            socket_box.put(&node_1, 0, 0);
            let node_2 = node::Node::new();
            node_2.add_socket(uriparse::URI::try_from("node:foo#socket_in").unwrap(), node_socket::NodeSocketIO::Sink);
            node_2.add_socket(uriparse::URI::try_from("node:foo#socket_out").unwrap(), node_socket::NodeSocketIO::Source);
            socket_box.put(&node_2, 256, 0);
            vbox.add(&socket_box);
        }

        vbox.add(&button_box);
        vbox.pack_end(&node_area, true, true, 0);

        window.add(&vbox);
        window.show_all();
    });

    application.run(&[]);
}
