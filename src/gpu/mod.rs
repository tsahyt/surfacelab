use gfx_backend_vulkan as back;
use gfx_hal as hal;
use gfx_hal::prelude::*;
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

/// Initialize the GPU, optionally headless. When headless is specified,
/// no graphics capable family is required.
///
/// TODO: Late creation of GPU to check for surface compatibility when not running headless
pub fn initialize_gpu(headless: bool) -> Result<Arc<Mutex<GPU<back::Backend>>>, String> {
    log::info!("Initializing GPU");

    let instance = back::Instance::create("surfacelab", 1)
        .map_err(|e| format!("Failed to create an instance! {:?}", e))?;
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
            "Failed to find a GPU with compute support"
        } else {
            "Failed to find a GPU with compute and graphics support!"
        })?;

    let gpu = GPU::new(instance, adapter, headless)?;
    Ok(Arc::new(Mutex::new(gpu)))
}

impl<B> GPU<B>
where
    B: Backend,
{
    pub fn new(
        instance: B::Instance,
        adapter: hal::adapter::Adapter<B>,
        headless: bool,
    ) -> Result<Self, String> {
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

        Ok(GPU {
            instance,
            device,
            queue_group,
            adapter,
            memory_properties,
        })
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
// TODO: ResourceAlive should include a mutex such that we have more than spot checks

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

unsafe impl Send for BrokerImage {}
unsafe impl Sync for BrokerImage {}

impl BrokerImage {
    pub fn from<B: Backend>(view: &B::Image, alive: Weak<()>) -> Self {
        let ptr = view as *const B::Image as *const ();
        Self { alive, raw: ptr }
    }

    pub fn to<'a, B: Backend>(&'a self) -> Option<&'a B::Image> {
        match self.alive.upgrade() {
            Some(_) => unsafe { Some(&*(self.raw as *const B::Image)) },
            None => None,
        }
    }
}

/// Image View variant for broker transmission. See `BrokerImage` for caveats
#[derive(Debug)]
pub struct BrokerImageView {
    alive: ResourceAlive,
    raw: *const (),
}

unsafe impl Send for BrokerImageView {}
unsafe impl Sync for BrokerImageView {}

impl BrokerImageView {
    pub fn from<B: Backend>(view: &B::ImageView, alive: Weak<()>) -> Self {
        let ptr = view as *const B::ImageView as *const ();
        Self { alive, raw: ptr }
    }

    pub fn to<'a, B: Backend>(&self) -> Option<&'a B::ImageView> {
        match self.alive.upgrade() {
            Some(_) => unsafe { Some(&*(self.raw as *const B::ImageView)) },
            None => None,
        }
    }
}
