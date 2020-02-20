use crate::{bus, lang::*};
use gio::prelude::*;
use glib::clone;
use gtk::prelude::*;
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

fn ui_bus(gsender: glib::Sender<bus::Lang>, receiver: bus::Receiver) {
    for event in receiver {
        log::trace!("UI processing event {:?}", event);
        gsender.send(event).unwrap();
    }
}

fn gtk_main(sender: bus::Sender, receiver: bus::Receiver) {
    gtk::init().expect("Failed to initialize gtk");
    let application = application::SurfaceLabApplication::new(sender);

    let (gsender, greceiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    let ui_thread = thread::spawn(move || ui_bus(gsender, receiver));

    let application_clone = application.clone();
    greceiver.attach(None, move |event: bus::Lang| {
        application_clone.process_event(event);
        glib::Continue(true)
    });

    application.run(&[]);

    ui_thread.join().unwrap();
    log::info!("UI Terminating");
}
