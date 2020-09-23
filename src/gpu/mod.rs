use gfx_backend_vulkan as back;
use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::any::Any;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, Weak};

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

#[derive(Debug)]
pub enum InitializationError {
    /// Failed to acquire a resource during initialization
    ResourceAcquisition(&'static str),
    /// Failed to allocate memory
    Allocation(&'static str),
    /// Failed to bind memory to a resource
    Bind,
    MissingFeature(&'static str),
    /// Failed to read shader SPIR-V
    ShaderSPIRV,
    /// Failed to build shader module
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

/// Type for cross thread checks of whether a resource is still alive.
/// Implemented as Arc/Weak. Arc is to be held at the resource "home", Weak can
/// be distributed.
pub type ResourceAlive = Weak<()>;

/// Image variant hiding the parameterization over the backend. Deeply
/// unsafe! Must be used with similar backend types on both ends. No care is
/// taken to ensure this.
///
/// This exists solely for transmitting an image over the broker bus
/// without incurring the type parameter all throughout the language.
///
/// Some manual care is required to make sure the images does not drop while
/// the data is in use in both threads. To do so, the `from` method takes a
/// `Weak` that must be alive if and only if the backing image is still alive.
#[derive(Debug)]
pub struct BrokerImage {
    alive: ResourceAlive,
    raw: *const (),
}

// TODO: Do not send Image, instead use views and render directly from compute memory

unsafe impl Send for BrokerImage {}
unsafe impl Sync for BrokerImage {}

impl BrokerImage {
    pub fn from<B: Backend>(view: &B::Image, alive: Weak<()>) -> Self {
        let ptr = view as *const B::Image as *const ();
        Self { alive, raw: ptr }
    }

    pub fn to<B: Backend>(&self) -> Option<&B::Image> {
        match self.alive.upgrade() {
            Some(_) => unsafe { Some(&*(self.raw as *const B::Image)) },
            None => None,
        }
    }
}

/// A type hiding the backend safely in order to send it over the bus.
#[derive(Debug, Clone)]
pub struct BrokerImageView {
    inner: Arc<dyn Any + 'static + Send + Sync>,
}

impl BrokerImageView {
    pub fn from<B: Backend>(view: &Arc<Mutex<B::ImageView>>) -> Self {
        Self {
            inner: view.clone(),
        }
    }

    pub fn to<B: Backend>(self) -> Weak<Mutex<B::ImageView>> {
        self.inner
            .downcast::<Mutex<B::ImageView>>()
            .ok()
            .map(|x| Arc::downgrade(&x))
            .unwrap()
    }
}
