use crate::{bus, lang};
use gio::prelude::*;
use gtk::prelude::*;
use std::convert::TryFrom;
use std::rc::Rc;
use std::thread;

macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}

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
        window.set_default_size(350, 70);

        let button_box = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let new_o_button = gtk::Button::new_with_label("New Output");
        new_o_button.connect_clicked(clone!(bus_rc => move |_| {
            bus::emit(
                bus_rc.as_ref(),
                lang::Lang::UserNodeEvent(lang::UserNodeEvent::NewNode(lang::Operator::Output {
                    output_type: lang::OutputType::default(),
                })),
            );
        }));
        button_box.add(&new_o_button);

        let new_i_button = gtk::Button::new_with_label("New Input");
        new_i_button.connect_clicked(clone!(bus_rc => move |_| {
            bus::emit(
                bus_rc.as_ref(),
                lang::Lang::UserNodeEvent(lang::UserNodeEvent::NewNode(lang::Operator::Image {
                    path: std::path::PathBuf::from("/tmp/foo.png"),
                })),
            );
        }));
        button_box.add(&new_i_button);

        let remove_button = gtk::Button::new_with_label("Remove Output");
        remove_button.connect_clicked(clone!(bus_rc => move |_| {
            bus::emit(
                bus_rc.as_ref(),
                lang::Lang::UserNodeEvent(
                    lang::UserNodeEvent::RemoveNode(
                        uriparse::uri::URI::try_from("node:output.1").unwrap())),
            );
        }));
        button_box.add(&remove_button);

        let connect_button = gtk::Button::new_with_label("Connect");
        connect_button.connect_clicked(clone!(bus_rc => move |_| {
            bus::emit(
                bus_rc.as_ref(),
                lang::Lang::UserNodeEvent(
                    lang::UserNodeEvent::ConnectSockets(
                        uriparse::uri::URI::try_from("node:image.1#image").unwrap(),
                        uriparse::uri::URI::try_from("node:output.1#value").unwrap()
                    )),
            );
        }));
        button_box.add(&connect_button);

        window.add(&button_box);
        window.show_all();
    });

    application.run(&[]);
}
