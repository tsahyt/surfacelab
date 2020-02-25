use gfx_backend_vulkan as back;
use gfx_hal as hal;
use gfx_hal::prelude::*;
pub use gfx_hal::Backend;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

pub struct GPU<B: Backend> {
    instance: B::Instance,
    device: B::Device,
    queue_group: hal::queue::QueueGroup<B>,
    // command_pool: Arc<Mutex<B::CommandPool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderType {
    Compute,
    Vertex,
    Fragment,
}

pub struct Shader(ShaderType, &'static str);

pub struct CommandBuffer<'a, B: Backend> {
    inner: ManuallyDrop<B::CommandBuffer>,
    pool: &'a mut B::CommandPool,
}

impl<B> Drop for CommandBuffer<'_, B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::debug!("Dropping Command Buffer");
        unsafe {
            self.pool.free(Some(ManuallyDrop::take(&mut self.inner)));
            ManuallyDrop::drop(&mut self.inner);
        }
    }
}

/// Initialize the GPU, optionally headless. When headless is specified,
/// no graphics capable family is required.
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

pub struct GPUCompute<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: B::CommandPool,
    shaders: HashMap<&'static str, B::ShaderModule>,
    uniforms: B::Buffer,
}

impl<B> GPUCompute<B>
where
    B: Backend,
{
    /// Create a new GPUCompute instance.
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, String> {
        log::info!("Obtaining GPU Compute Resources");
        let lock = gpu.lock().unwrap();

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "Can't create command pool!")?;

        Ok(GPUCompute {
            gpu: gpu.clone(),
            command_pool: command_pool,
            shaders: HashMap::new(),
            uniforms: unimplemented!(),
        })
    }

    pub fn register_shader(&mut self, spirv: &[u8], name: &'static str) -> Result<Shader, String> {
        let lock = self.gpu.lock().unwrap();
        let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(spirv))
            .map_err(|e| format!("Failed to load SPIR-V: {}", e))?;
        let shader = unsafe { lock.device.create_shader_module(&loaded_spirv) }
            .map_err(|e| format!("Failed to build shader module: {}", e))?;
        self.shaders.insert(name, shader);
        Ok(Shader(ShaderType::Compute, name))
    }

    pub fn primary_command_buffer(&mut self) -> CommandBuffer<B> {
        let inner = { unsafe { self.command_pool.allocate_one(hal::command::Level::Primary) } };

        CommandBuffer {
            inner: ManuallyDrop::new(inner),
            pool: &mut self.command_pool,
        }
    }
}

impl<B> Drop for GPUCompute<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::info!("Releasing GPU Compute resources")
    }
}

pub struct GPURender<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: B::CommandPool,
    vertex_shaders: HashMap<&'static str, B::ShaderModule>,
    fragment_shaders: HashMap<&'static str, B::ShaderModule>,
}

impl<B> GPURender<B>
where
    B: Backend,
{
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, String> {
        log::info!("Obtaining GPU Render Resources");
        let lock = gpu.lock().unwrap();

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "Can't create command pool!")?;

        Ok(GPURender {
            gpu: gpu.clone(),
            command_pool: command_pool,
            vertex_shaders: HashMap::new(),
            fragment_shaders: HashMap::new(),
        })
    }
}

impl<B> Drop for GPURender<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::info!("Releasing GPU Render resources")
    }
}
