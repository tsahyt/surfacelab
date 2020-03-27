use crate::{broker, lang};
use gio::prelude::*;
use once_cell::unsync::OnceCell;
use std::sync::Arc;
use std::thread;

pub mod application;
pub mod color_ramp;
pub mod node;
pub mod node_area;
pub mod node_socket;
pub mod param_box;
pub mod render_events;
pub mod render_area;

thread_local!(static BROKER: OnceCell<broker::BrokerSender<lang::Lang>> = OnceCell::new());

fn emit(ev: lang::Lang) {
    BROKER.with(|b| {
        if let Err(e) = b.get().expect("Uninitialized broker in UI TLS").send(ev) {
            log::error!("UI lost connection to application bus! {}", e)
        }
    })
}

pub fn start_ui_thread(broker: &mut broker::Broker<lang::Lang>) -> thread::JoinHandle<()> {
    log::info!("Starting UI");

    let (sender, receiver) = broker.subscribe();

    thread::spawn(move || gtk_main(sender, receiver))
}

fn ui_bus(gsender: glib::Sender<Arc<lang::Lang>>, receiver: broker::BrokerReceiver<lang::Lang>) {
    for event in receiver {
        gsender.send(event.clone()).unwrap();
        if let lang::Lang::UserEvent(lang::UserEvent::Quit) = &*event {
            break;
        }
    }
}

fn gtk_main(
    sender: broker::BrokerSender<lang::Lang>,
    receiver: broker::BrokerReceiver<lang::Lang>,
) {
    gtk::init().expect("Failed to initialize gtk");

    BROKER.with(|b| {
        b.set(sender)
            .map_err(|_| "<UI thread bus>")
            .expect("Failed to store UI thread bus")
    });
    let application = application::SurfaceLabApplication::new();

    let (gsender, greceiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
    let ui_thread = thread::spawn(move || ui_bus(gsender, receiver));

    let application_clone = application.clone();
    greceiver.attach(None, move |event: Arc<lang::Lang>| {
        application_clone.process_event(event);
        glib::Continue(true)
    });

    application.run(&[]);

    ui_thread.join().unwrap();
    log::info!("UI Terminating");
}
