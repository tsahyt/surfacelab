use super::{Backend, GPU};
use crate::gpu::basic_mem::*;
use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use thiserror::Error;

pub struct Matcap<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    path: std::path::PathBuf,

    matcap_image: ManuallyDrop<B::Image>,
    matcap_view: ManuallyDrop<B::ImageView>,
    matcap_memory: ManuallyDrop<B::Memory>,
}

#[derive(Debug, Error)]
pub enum MatcapError {
    #[error("Failed to build GPU image for matcap: {0}")]
    ImageBuilderError(#[from] BasicImageBuilderError),
    #[error("Failed to build staging buffer for matcap: {0}")]
    BufferBuilderError(#[from] BasicBufferBuilderError),
    #[error("Matcap IO failed: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Matcap decoding failed: {0}")]
    DecodeError(#[from] image::error::ImageError),
    #[error("Failed to obtain resources for upload of matcap: {0}")]
    OutOfMemory(#[from] hal::device::OutOfMemory),
}

impl<B> Matcap<B>
where
    B: Backend,
{
    const FORMAT: hal::format::Format = hal::format::Format::Rgba8Srgb;

    pub fn from_file<P: AsRef<Path>>(
        gpu: Arc<Mutex<GPU<B>>>,
        path: P,
    ) -> Result<Self, MatcapError> {
        let mut lock = gpu.lock().unwrap();

        // Read matcap from disk
        let io_timer = Instant::now();
        let image = image::io::Reader::open(path.as_ref())?.decode()?.to_rgba8();
        log::debug!(
            "Read Matcap from disk in {}ms",
            io_timer.elapsed().as_millis()
        );

        // Obtain resources for matcap
        let (matcap_image, matcap_memory, matcap_view) =
            BasicImageBuilder::new(&lock.memory_properties.memory_types)
                .size_2d(image.width(), image.height())
                .usage(hal::image::Usage::SAMPLED | hal::image::Usage::TRANSFER_DST)
                .format(Self::FORMAT)
                .memory_type(hal::memory::Properties::DEVICE_LOCAL)
                .unwrap()
                .build::<B>(&lock.device)?;

        // Build staging buffer
        let (staging_buffer, staging_memory) =
            BasicBufferBuilder::new(&lock.memory_properties.memory_types)
                .bytes((image.width() * image.height() * 4) as u64)
                .usage(hal::buffer::Usage::TRANSFER_SRC)
                .data(image.as_raw())
                .memory_type(hal::memory::Properties::CPU_VISIBLE)
                .unwrap()
                .build::<B>(&lock.device)?;

        // Transfer from staging buffer to device only memory
        let mut command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::TRANSIENT,
            )
        }?;

        let fence = lock.device.create_fence(false).unwrap();

        unsafe {
            let mut command_buffer = command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[hal::memory::Barrier::Image {
                    states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                        ..(
                            hal::image::Access::TRANSFER_WRITE,
                            hal::image::Layout::TransferDstOptimal,
                        ),
                    target: &matcap_image,
                    families: None,
                    range: hal::image::SubresourceRange {
                        aspects: hal::format::Aspects::COLOR,
                        ..Default::default()
                    },
                }],
            );
            command_buffer.copy_buffer_to_image(
                &staging_buffer,
                &matcap_image,
                hal::image::Layout::TransferDstOptimal,
                Some(hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: image.width(),
                    buffer_height: image.height(),
                    image_offset: hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: hal::image::Extent {
                        width: image.width(),
                        height: image.height(),
                        depth: 1,
                    },
                    image_layers: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                }),
            );
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TRANSFER..hal::pso::PipelineStage::FRAGMENT_SHADER,
                hal::memory::Dependencies::empty(),
                &[hal::memory::Barrier::Image {
                    states: (
                        hal::image::Access::TRANSFER_WRITE,
                        hal::image::Layout::TransferDstOptimal,
                    )
                        ..(
                            hal::image::Access::SHADER_READ,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        ),
                    target: &matcap_image,
                    families: None,
                    range: hal::image::SubresourceRange {
                        aspects: hal::format::Aspects::COLOR,
                        ..Default::default()
                    },
                }],
            );

            command_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&command_buffer), Some(&fence));
            lock.device.wait_for_fence(&fence, !0).unwrap();
            command_pool.free(Some(command_buffer));
        }

        // Teardown of temporary resources
        unsafe {
            lock.device.destroy_buffer(staging_buffer);
            lock.device.free_memory(staging_memory);
            lock.device.destroy_fence(fence);
            lock.device.destroy_command_pool(command_pool);
        }

        Ok(Self {
            gpu: gpu.clone(),
            path: path.as_ref().into(),
            matcap_image: ManuallyDrop::new(matcap_image),
            matcap_view: ManuallyDrop::new(matcap_view),
            matcap_memory: ManuallyDrop::new(matcap_memory),
        })
    }

    /// Get a reference to the matcap's image view.
    pub fn matcap_view(&self) -> &B::ImageView {
        &*self.matcap_view
    }

    /// Get a reference to the matcap's path.
    pub fn path(&self) -> &std::path::PathBuf {
        &self.path
    }
}

impl<B> Drop for Matcap<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::debug!("Dropping matcap");

        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.matcap_image));
            lock.device
                .destroy_image_view(ManuallyDrop::take(&mut self.matcap_view));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.matcap_memory));
        }
    }
}
