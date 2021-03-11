use crate::{broker, lang::*};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    thread,
};

pub mod file;

pub fn start_io_thread(broker: &mut broker::Broker<Lang>) -> thread::JoinHandle<()> {
    let (sender, receiver, disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("io".to_string())
        .spawn(move || {
            log::info!("Starting IO manager");

            let mut io_manager = IOManager::new();

            for event in receiver {
                let res = io_manager.process_event(&event);
                for r in res {
                    sender.send(r).unwrap();
                }
            }

            log::info!("IO manager terminating");
            disconnector.disconnect();
        })
        .expect("Failed to start IO manager thread!")
}

pub struct IOManager {
    file_builder: Option<file::SurfaceFileBuilder>,
    save_path: Option<PathBuf>,
}

impl IOManager {
    pub fn new() -> Self {
        Self {
            file_builder: None,
            save_path: None,
        }
    }

    pub fn process_event(&mut self, event: &Lang) -> Vec<Lang> {
        let mut response = Vec::new();

        match event {
            Lang::UserIOEvent(UserIOEvent::OpenSurface(path)) => {
                response.append(&mut self.open_surface(path))
            }
            Lang::UserIOEvent(UserIOEvent::SaveSurface(path)) => self.save_surface(path),
            Lang::GraphEvent(GraphEvent::Serialized(data)) => self.write_graph_data(data),
            Lang::ComputeEvent(ComputeEvent::Serialized(data)) => self.write_compute_data(data),
            _ => {}
        }

        response
    }

    fn open_surface<P: AsRef<Path> + Debug>(&self, path: P) -> Vec<Lang> {
        let mut response = Vec::new();

        match file::SurfaceFile::open(path) {
            Ok(file::SurfaceFile {
                node_data,
                compute_data,
            }) => {
                response.push(Lang::IOEvent(IOEvent::NodeDataLoaded(node_data)));
                response.push(Lang::IOEvent(IOEvent::ComputeDataLoaded(compute_data)));
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
