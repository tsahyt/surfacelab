use super::{Backend, InitializationError, PipelineError, GPU};
use gfx_hal as hal;
use gfx_hal::prelude::*;
use image::hdr;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct EnvironmentMaps<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    irradiance_image: ManuallyDrop<B::Image>,
    irradiance_view: ManuallyDrop<B::ImageView>,
    irradiance_memory: ManuallyDrop<B::Memory>,
}

impl<B> EnvironmentMaps<B>
where
    B: Backend,
{
    /// Initialize GPU side structures for environment map without data, given a
    /// cubemap size.
    fn init(gpu: Arc<Mutex<GPU<B>>>, size: usize) -> Result<Self, String> {
        todo!()
    }

    /// Create environment maps from a path to a HDRi file. Expects .hdr,
    /// equirectangular mapping!
    pub fn from_file<P: AsRef<Path>>(
        gpu: Arc<Mutex<GPU<B>>>,
        cubemap_size: usize,
        path: P,
    ) -> Result<Self, String> {
        use std::fs::File;
        use std::io::BufReader;

        // Initialize
        let env_maps = Self::init(gpu, cubemap_size)?;

        // Read data from file
        let reader =
            BufReader::new(File::open(path).map_err(|_| "Failed to open HDRi file".to_string())?);
        let decoder =
            hdr::HdrDecoder::new(reader).map_err(|_| "Failed to decode HDRi file".to_string())?;
        let metadata = decoder.metadata();
        let raw_hdri = decoder
            .read_image_hdr()
            .map_err(|_| "Failed to read from HDRi file".to_string())?;

        // Prepare compute pipeline

        // Convolve irradiance map

        // Pre-filter environment map

        // Clean up compute pipeline
        todo!()
    }
}

impl<B> Drop for EnvironmentMaps<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::debug!("Dropping environment maps");

        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.irradiance_image));
            lock.device
                .destroy_image_view(ManuallyDrop::take(&mut self.irradiance_view));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.irradiance_memory));
        }
    }
}
