use gfx_backend_vulkan as back;
use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::any::Any;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, Weak};
use thiserror::Error;

pub use gfx_hal::Backend;
pub use hal::buffer::SubRange;
pub use hal::image::{Access, Layout};
pub use hal::pso::{
    BufferDescriptorFormat, BufferDescriptorType, Descriptor, DescriptorSetLayoutBinding,
    DescriptorSetWrite, DescriptorType, ImageDescriptorType, ShaderStageFlags,
};
pub use hal::window::Extent2D;
pub use hal::Instance;

pub mod compute;
pub mod render;
pub mod ui;

pub const COLOR_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    levels: 0..1,
    layers: 0..1,
};

// TODO: more finegrained concurrency model for GPU
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
pub enum InitializationError {
    #[error("Failed to acquire a resource during initialization")]
    ResourceAcquisition(&'static str),
    #[error("Failed to allocate memory")]
    Allocation(&'static str),
    #[error("Failed to bind memory to a resource")]
    Bind,
    #[error("Missing feature")]
    MissingFeature(&'static str),
    #[error("Failed to read SPIR-V for shader")]
    ShaderSPIRV,
    #[error("Failed to build shader module")]
    ShaderModule,
}

#[derive(Debug)]
pub enum PipelineError {
    /// Failed to map Uniform Buffer into CPU space
    UniformMapping,
    /// Errors during downloading of images
    DownloadError(DownloadError),
    /// Errors during uploading of images
    UploadError(UploadError),
}

#[derive(Debug)]
pub enum DownloadError {
    /// Failed to create download buffer
    Creation,
    /// Failed to allocate memory for download buffer
    Allocation,
    /// Failed to bind memory for download buffer
    BufferBind,
    /// Failed to map download buffer into CPU space
    Map,
}

impl From<DownloadError> for PipelineError {
    fn from(e: DownloadError) -> Self {
        Self::DownloadError(e)
    }
}

#[derive(Debug)]
pub enum UploadError {
    /// Failed to create upload buffer
    Creation,
    /// Failed to allocate memory for upload buffer
    Allocation,
    /// Failed to bind memory for upload buffer
    BufferBind,
}

impl From<UploadError> for PipelineError {
    fn from(e: UploadError) -> Self {
        Self::UploadError(e)
    }
}

/// Initialize the GPU, optionally headless. When headless is specified,
/// no graphics capable family is required.
///
/// TODO: Late creation of GPU to check for surface compatibility when not running headless
pub fn initialize_gpu(
    headless: bool,
) -> Result<Arc<Mutex<GPU<back::Backend>>>, InitializationError> {
    log::info!("Initializing GPU");

    let instance = back::Instance::create("surfacelab", 1)
        .map_err(|_| InitializationError::ResourceAcquisition("Instance"))?;
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
            InitializationError::MissingFeature("Compute")
        } else {
            InitializationError::MissingFeature("Graphics")
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

impl<B> Drop for GPU<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::info!("Dropping GPU")
    }
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
    ) -> Result<Self, InitializationError> {
        let lock = gpu.lock().unwrap();

        // Create Image
        let mut image = unsafe {
            lock.device.create_image(
                hal::image::Kind::D2(dimensions.0, dimensions.1, 1, samples),
                1,
                format,
                hal::image::Tiling::Optimal,
                if compute_target {
                    hal::image::Usage::SAMPLED | hal::image::Usage::STORAGE
                } else {
                    hal::image::Usage::COLOR_ATTACHMENT | hal::image::Usage::SAMPLED
                },
                hal::image::ViewCapabilities::empty(),
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Target Image"))?;

        // Allocate and bind memory for image
        let requirements = unsafe { lock.device.get_image_requirements(&image) };
        let memory_type = lock
            .memory_properties
            .memory_types
            .iter()
            .position(|mem_type| {
                mem_type
                    .properties
                    .contains(hal::memory::Properties::DEVICE_LOCAL)
            })
            .unwrap()
            .into();
        let memory = unsafe { lock.device.allocate_memory(memory_type, requirements.size) }
            .map_err(|_| InitializationError::Allocation("Render Target Image"))?;
        unsafe { lock.device.bind_image_memory(&memory, 0, &mut image) }
            .map_err(|_| InitializationError::Bind)?;

        let view = unsafe {
            lock.device.create_image_view(
                &image,
                hal::image::ViewKind::D2,
                format,
                hal::format::Swizzle::NO,
                COLOR_RANGE.clone(),
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Target Image View"))?;

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
