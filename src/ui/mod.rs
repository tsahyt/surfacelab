use crate::{bus, lang::*};
use gio::prelude::*;
use gtk::prelude::*;
use glib::clone;
use std::rc::Rc;
use std::thread;

pub mod application;
pub mod node;
pub mod node_area;
pub mod node_socket;
pub mod subclass;

pub fn start_ui_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    log::info!("Starting UI");

    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || gtk_main(sender, receiver))
}

fn ui_bus(receiver: bus::Receiver) {
    for event in receiver {
        log::trace!("UI processing event {:?}", event);

        match event {
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, op) => {}
            },
            Lang::UserNodeEvent(..) => {}
        }
    }

    log::info!("UI Terminating");
}

fn gtk_main(sender: bus::Sender, receiver: bus::Receiver) {
    let bus_rc = Rc::new(sender);

    gtk::init().expect("Failed to initialize gtk");
    let application = application::SurfaceLabApplication::new();
    let bus_thread = thread::spawn(move || ui_bus(receiver));
    application.run(&[]);
    bus_thread.join().unwrap();
}
