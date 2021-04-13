use gfx_backend_vulkan as back;
use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::any::Any;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, Weak};
use thiserror::Error;

pub use gfx_hal::{spec_const_list, Backend};
pub use hal::buffer::SubRange;
pub use hal::image::{Access, Layout};
pub use hal::pso::{
    BufferDescriptorFormat, BufferDescriptorType, Descriptor, DescriptorSetLayoutBinding,
    DescriptorSetWrite, DescriptorType, ImageDescriptorType, ShaderStageFlags, Specialization,
};
pub use hal::window::Extent2D;
pub use hal::Instance;

pub mod basic_mem;
pub mod compute;
pub mod render;
pub mod ui;

pub const COLOR_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    level_start: 0,
    level_count: Some(1),
    layer_start: 0,
    layer_count: Some(1),
};

pub struct GPU<B: Backend> {
    instance: B::Instance,
    device: B::Device,
    adapter: hal::adapter::Adapter<B>,
    queue_group: hal::queue::QueueGroup<B>,
    memory_properties: hal::adapter::MemoryProperties,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderType {
    Compute,
    Vertex,
    Fragment,
}

pub struct Shader<B: Backend> {
    raw: ManuallyDrop<B::ShaderModule>,
    ty: ShaderType,
    parent: Arc<Mutex<GPU<B>>>,
}

impl<B> Drop for Shader<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::debug!("Dropping {:?} shader module", self.ty);

        let lock = self.parent.lock().unwrap();
        unsafe {
            lock.device
                .destroy_shader_module(ManuallyDrop::take(&mut self.raw));
        }
    }
}

#[derive(Debug, Error)]
pub enum PipelineError {
    /// Failed to map Uniform Buffer into CPU space
    #[error("Failed to map Uniform Buffer into CPU space")]
    UniformMapping,
    /// Errors during downloading of images
    #[error("Error during image download")]
    DownloadError(#[from] DownloadError),
}

#[derive(Debug, Error)]
pub enum DownloadError {
    /// Cannot download non-backed image
    #[error("Tried to download an image that is not currently backed")]
    NotBacked,
    /// Failed to build the download buffer
    #[error("Failed to build the download buffer")]
    Building(#[from] basic_mem::BasicBufferBuilderError),
    /// Failed to map download buffer into CPU space
    #[error("Failed to map download buffer into CPU space")]
    Map,
}

#[derive(Debug, Error)]
pub enum BootError {
    #[error("Missing compute support on device")]
    MissingComputeSupport,
    #[error("Missing compute support on device")]
    MissingGraphicsSupport,
    #[error("Unsupported backend encountered")]
    UnsupportedBackend,
}

/// Initialize the GPU, optionally headless. When headless is specified,
/// no graphics capable family is required.
pub fn initialize_gpu(headless: bool) -> Result<Arc<Mutex<GPU<back::Backend>>>, BootError> {
    log::info!("Initializing GPU");

    let instance =
        back::Instance::create("surfacelab", 1).map_err(|_| BootError::UnsupportedBackend)?;
    let adapter = instance
        .enumerate_adapters()
        .into_iter()
        .find(|adapter| {
            adapter.queue_families.iter().any(|family| {
                family.queue_type().supports_compute()
                    && (headless || family.queue_type().supports_graphics())
            })
        })
        .ok_or(if headless {
            BootError::MissingComputeSupport
        } else {
            BootError::MissingGraphicsSupport
        })?;

    let gpu = GPU::new(instance, adapter, headless);
    Ok(Arc::new(Mutex::new(gpu)))
}

impl<B> GPU<B>
where
    B: Backend,
{
    pub fn new(instance: B::Instance, adapter: hal::adapter::Adapter<B>, headless: bool) -> Self {
        log::debug!("Using adapter {:?}", adapter);

        let memory_properties = adapter.physical_device.memory_properties();
        log::debug!(
            "Supported memory types: {:?}",
            memory_properties.memory_types
        );

        let family = adapter
            .queue_families
            .iter()
            .find(|family| {
                family.queue_type().supports_compute()
                    && (headless || family.queue_type().supports_graphics())
            })
            .unwrap();
        let mut gpu = unsafe {
            adapter
                .physical_device
                .open(&[(family, &[1.0])], hal::Features::empty())
                .unwrap()
        };

        let queue_group = gpu.queue_groups.pop().unwrap();
        let device = gpu.device;

        GPU {
            instance,
            device,
            queue_group,
            adapter,
            memory_properties,
        }
    }
}

#[derive(Debug, Error)]
pub enum ShaderError {
    #[error("Error reading SPIR-V code for shader")]
    SPIRVError,
    #[error("Failed to create shader module for pipeline")]
    ShaderModuleCreation(#[from] hal::device::ShaderError),
}

/// Convenience function for creating shader modules for SPIR-V bytecode.
pub fn load_shader<B: Backend>(
    device: &B::Device,
    spirv: &'static [u8],
) -> Result<B::ShaderModule, ShaderError> {
    let loaded_spirv =
        gfx_auxil::read_spirv(std::io::Cursor::new(spirv)).map_err(|_| ShaderError::SPIRVError)?;
    unsafe { device.create_shader_module(&loaded_spirv) }.map_err(ShaderError::from)
}

impl<B> Drop for GPU<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::info!("Dropping GPU")
    }
}

#[derive(Debug, Error)]
pub enum RenderTargetError {
    #[error("No suitable memory found for render target")]
    MemoryType,
    #[error("Failed to construct render target")]
    Construction(#[from] basic_mem::BasicImageBuilderError),
}

pub struct RenderTarget<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    image: ManuallyDrop<B::Image>,
    view: ManuallyDrop<Arc<Mutex<B::ImageView>>>,
    memory: ManuallyDrop<B::Memory>,
    image_layout: hal::image::Layout,
    samples: hal::image::NumSamples,
    format: hal::format::Format,
    compute_target: bool,
}

impl<B> RenderTarget<B>
where
    B: Backend,
{
    pub fn new(
        gpu: Arc<Mutex<GPU<B>>>,
        format: hal::format::Format,
        samples: hal::image::NumSamples,
        compute_target: bool,
        dimensions: (u32, u32),
    ) -> Result<Self, RenderTargetError> {
        let lock = gpu.lock().unwrap();

        let (image, memory, view) =
            basic_mem::BasicImageBuilder::new(&lock.memory_properties.memory_types)
                .size_2d_msaa(dimensions.0, dimensions.1, samples)
                .format(format)
                .tiling(hal::image::Tiling::Optimal)
                .usage(if compute_target {
                    hal::image::Usage::SAMPLED | hal::image::Usage::STORAGE
                } else {
                    hal::image::Usage::COLOR_ATTACHMENT | hal::image::Usage::SAMPLED
                })
                .memory_type(hal::memory::Properties::DEVICE_LOCAL)
                .ok_or(RenderTargetError::MemoryType)?
                .build::<B>(&lock.device)?;

        Ok(Self {
            gpu: gpu.clone(),
            image: ManuallyDrop::new(image),
            view: ManuallyDrop::new(Arc::new(Mutex::new(view))),
            memory: ManuallyDrop::new(memory),
            image_layout: hal::image::Layout::Undefined,
            samples,
            format,
            compute_target,
        })
    }

    pub fn image_view(&self) -> &Arc<Mutex<B::ImageView>> {
        &self.view
    }

    fn barrier_to(
        &mut self,
        access: hal::image::Access,
        layout: hal::image::Layout,
    ) -> hal::memory::Barrier<B> {
        let barrier = hal::memory::Barrier::Image {
            states: (hal::image::Access::empty(), self.image_layout)..(access, layout),
            target: &*self.image,
            families: None,
            range: COLOR_RANGE.clone(),
        };

        if self.compute_target {
            self.image_layout = layout;
        } else {
            self.image_layout = hal::image::Layout::ShaderReadOnlyOptimal;
        }

        barrier
    }

    pub fn barrier_before(&mut self) -> hal::memory::Barrier<B> {
        if self.compute_target {
            self.barrier_to(
                hal::image::Access::SHADER_WRITE,
                hal::image::Layout::General,
            )
        } else {
            self.barrier_to(
                hal::image::Access::COLOR_ATTACHMENT_WRITE,
                hal::image::Layout::ColorAttachmentOptimal,
            )
        }
    }

    pub fn barrier_after(&mut self) -> hal::memory::Barrier<B> {
        self.barrier_to(
            hal::image::Access::SHADER_READ,
            hal::image::Layout::ShaderReadOnlyOptimal,
        )
    }

    pub fn samples(&self) -> hal::image::NumSamples {
        self.samples
    }

    pub fn format(&self) -> hal::format::Format {
        self.format
    }
}

impl<B> Drop for RenderTarget<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        let view = {
            let mut inner = unsafe { ManuallyDrop::take(&mut self.view) };
            loop {
                match Arc::try_unwrap(inner) {
                    Ok(t) => break t,
                    Err(a) => inner = a,
                }
            }
        };
        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device.destroy_image_view(view.into_inner().unwrap());
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.memory));
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.image));
        }
    }
}

/// An Image type hiding the backend safely in order to send it over the bus.
#[derive(Debug, Clone)]
pub struct BrokerImage {
    inner: Weak<dyn Any + 'static + Send + Sync>,
}

impl BrokerImage {
    pub fn from<B: Backend>(image: &Arc<Mutex<B::Image>>) -> Self {
        let gen_img: Arc<dyn Any + 'static + Send + Sync> = image.clone();
        Self {
            inner: Arc::downgrade(&gen_img),
        }
    }

    pub fn to<B: Backend>(self) -> Option<Weak<Mutex<B::Image>>> {
        self.inner
            .upgrade()
            .and_then(|x| x.downcast::<Mutex<B::Image>>().ok())
            .map(|x| Arc::downgrade(&x))
    }
}

/// An ImageView type hiding the backend safely in order to send it over the bus.
#[derive(Debug, Clone)]
pub struct BrokerImageView {
    inner: Weak<dyn Any + 'static + Send + Sync>,
}

impl BrokerImageView {
    pub fn from<B: Backend>(image: &Arc<Mutex<B::ImageView>>) -> Self {
        let gen_img: Arc<dyn Any + 'static + Send + Sync> = image.clone();
        Self {
            inner: Arc::downgrade(&gen_img),
        }
    }

    pub fn to<B: Backend>(self) -> Option<Weak<Mutex<B::ImageView>>> {
        self.inner
            .upgrade()
            .and_then(|x| x.downcast::<Mutex<B::ImageView>>().ok())
            .map(|x| Arc::downgrade(&x))
    }
}
