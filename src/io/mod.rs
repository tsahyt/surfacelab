use crate::{broker, lang::config::Configuration, lang::*};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    thread,
};

pub mod file;

pub fn start_io_thread(
    broker: &mut broker::Broker<Lang>,
    config: Configuration,
) -> thread::JoinHandle<()> {
    let (sender, receiver, disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("io".to_string())
        .spawn(move || {
            log::info!("Starting IO manager");

            let mut io_manager = IOManager::new(config);
            let _scheduler = scheduler_setup(sender.clone());

            for event in receiver {
                match io_manager.process_event(&event) {
                    Some(res) => {
                        for r in res {
                            sender.send(r).unwrap();
                        }
                    }
                    None => break,
                }
            }

            io_manager.save_config();

            log::info!("IO manager terminating");
            disconnector.disconnect();
        })
        .expect("Failed to start IO manager thread!")
}

fn scheduler_setup(sender: broker::BrokerSender<Lang>) -> clokwerk::ScheduleHandle {
    use clokwerk::*;
    use enclose::*;

    let sender_arc = std::sync::Arc::new(sender);
    let mut scheduler = Scheduler::new();

    scheduler
        .every(2.minutes())
        .run(enclose!((sender_arc => sender) move ||
                      sender.send(Lang::ScheduleEvent(ScheduleEvent::Autosave)).unwrap()));

    scheduler.watch_thread(std::time::Duration::from_secs(1))
}

pub struct IOManager {
    file_builder: Option<file::SurfaceFileBuilder>,
    save_path: Option<PathBuf>,
    config: Configuration,
}

impl Default for IOManager {
    fn default() -> Self {
        Self::new(Configuration::default())
    }
}

impl IOManager {
    pub fn new(config: Configuration) -> Self {
        Self {
            file_builder: None,
            save_path: None,
            config,
        }
    }

    pub fn save_config(&self) {
        if let Err(e) = self.config.save_to_file("config.toml") {
            log::error!("Error during config file write: {}", e);
        }
    }

    pub fn process_event(&mut self, event: &Lang) -> Option<Vec<Lang>> {
        let mut response = Vec::new();

        match event {
            Lang::UserIOEvent(UserIOEvent::OpenSurface(path)) => {
                response.append(&mut self.open_surface(path))
            }
            Lang::UserIOEvent(UserIOEvent::SaveSurface(path)) => self.save_surface(path),
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::GraphEvent(GraphEvent::Serialized(data)) => self.write_graph_data(data),
            Lang::ComputeEvent(ComputeEvent::Serialized(data)) => self.write_compute_data(data),
            Lang::RenderEvent(RenderEvent::Serialized(data)) => self.write_render_settings(data),
            Lang::ScheduleEvent(ScheduleEvent::Autosave) => {
                log::debug!("Autosave requested by schedule");
                response.push(Lang::UserIOEvent(UserIOEvent::SaveSurface(
                    "autosave.surf".into(),
                )));
            }
            _ => {}
        }

        Some(response)
    }

    fn open_surface<P: AsRef<Path> + Debug>(&self, path: P) -> Vec<Lang> {
        let mut response = Vec::new();

        match file::SurfaceFile::open(path) {
            Ok(file::SurfaceFile {
                node_data,
                compute_data,
                render_settings,
            }) => {
                response.push(Lang::IOEvent(IOEvent::NodeDataLoaded(node_data)));
                response.push(Lang::IOEvent(IOEvent::ComputeDataLoaded(compute_data)));
                response.push(Lang::IOEvent(IOEvent::RenderSettingsLoaded(
                    render_settings,
                )));
            }
            Err(e) => log::error!("{}", e),
        }

        response
    }

    fn save_surface<P: AsRef<Path>>(&mut self, path: P) {
        self.save_path = Some(PathBuf::from(path.as_ref()));
        self.file_builder = Some(file::SurfaceFileBuilder::new());
    }

    fn write_graph_data(&mut self, data: &[u8]) {
        if let Some(fb) = &mut self.file_builder {
            fb.node_data(data);
        }

        self.attempt_write().unwrap();
    }

    fn write_compute_data(&mut self, data: &[u8]) {
        if let Some(fb) = &mut self.file_builder {
            fb.compute_data(data);
        }

        self.attempt_write().unwrap();
    }

    fn write_render_settings(&mut self, data: &[u8]) {
        if let Some(fb) = &mut self.file_builder {
            fb.render_settings(data);
        }

        self.attempt_write().unwrap();
    }

    fn attempt_write(&mut self) -> Result<(), file::SurfaceIOError> {
        if let Some(path) = self.save_path.as_ref() {
            if self
                .file_builder
                .as_ref()
                .map(|fb| fb.buildable())
                .unwrap_or(false)
            {
                let fb = self.file_builder.take().unwrap();
                let file = fb.build().unwrap();
                file.save(path)?;
            }
        }

        Ok(())
    }
}
