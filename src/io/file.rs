use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fs::File;
use std::path::Path;
use thiserror::Error;

/// Struct defining a .surf file.
#[derive(Debug, Serialize, Deserialize)]
pub struct SurfaceFile {
    pub node_data: Vec<u8>,
    pub compute_data: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum SurfaceIOError {
    #[error("Error during file IO")]
    FileIO(#[from] std::io::Error),
    #[error("Error during file serialization")]
    Serialization(#[from] serde_cbor::Error),
}

impl SurfaceFile {
    pub fn save<P: AsRef<Path> + std::fmt::Debug>(&self, path: P) -> Result<(), SurfaceIOError> {
        log::info!("Saving surface to {:?}", path);

        let output_file = File::create(path)?;
        serde_cbor::to_writer(output_file, &self)?;

        Ok(())
    }

    pub fn open<P: AsRef<Path> + std::fmt::Debug>(path: P) -> Result<Self, SurfaceIOError> {
        log::info!("Loading surface from file {:?}", path);
        let input_file = File::open(path)?;
        let surface_file: Self = serde_cbor::from_reader(input_file)?;

        Ok(surface_file)
    }
}

pub struct SurfaceFileBuilder {
    pub node_data: Option<Vec<u8>>,
    pub compute_data: Option<Vec<u8>>,
}

impl SurfaceFileBuilder {
    pub fn new() -> Self {
        Self {
            node_data: None,
            compute_data: None,
        }
    }

    pub fn buildable(&self) -> bool {
        self.node_data.is_some() && self.compute_data.is_some()
    }

    pub fn node_data(&mut self, node_data: &[u8]) -> &mut Self {
        self.node_data = Some(node_data.to_vec());
        self
    }

    pub fn compute_data(&mut self, compute_data: &[u8]) -> &mut Self {
        self.compute_data = Some(compute_data.to_vec());
        self
    }

    pub fn build(self) -> Option<SurfaceFile> {
        Some(SurfaceFile {
            node_data: self.node_data?,
            compute_data: self.compute_data?,
        })
    }
}
