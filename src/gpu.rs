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
    memory_properties: hal::adapter::MemoryProperties,
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

pub struct GPUCompute<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: B::CommandPool,
    shaders: HashMap<&'static str, B::ShaderModule>,

    // Uniforms
    uniform_buf: B::Buffer,
    uniform_mem: B::Memory,

    // Image Memory Management
    image_mem: B::Memory,
    image_mem_chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, Copy)]
struct Chunk {
    offset: u64,
    free: bool,
}

impl<B> GPUCompute<B>
where
    B: Backend,
{
    const UNIFORM_BUFFER_SIZE: u64 = 4096; // bytes

    const IMAGE_MEMORY_SIZE: u64 = 1024 * 1024 * 1024; // bytes
    const CHUNK_SIZE: u64 = 256 * 256 * 4; // bytes
    const N_CHUNKS: u64 = Self::IMAGE_MEMORY_SIZE / Self::CHUNK_SIZE;

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
        .map_err(|_| "Cannot create command pool!")?;

        // The uniform buffer is created once and shared across all compute
        // shaders, since only one is ever running at the same time. The size
        // of the buffer is given by UNIFORM_BUFFER_SIZE, and must be large
        // enough to accomodate every possible uniform struct!
        let (uniform_buf, uniform_mem) = unsafe {
            let mut buf = lock
                .device
                .create_buffer(
                    Self::UNIFORM_BUFFER_SIZE,
                    hal::buffer::Usage::TRANSFER_DST | hal::buffer::Usage::UNIFORM,
                )
                .map_err(|_| "Cannot create compute uniform buffer")?;
            let buffer_req = lock.device.get_buffer_requirements(&buf);
            let upload_type = lock
                .memory_properties
                .memory_types
                .iter()
                .enumerate()
                .position(|(id, mem_type)| {
                    // type_mask is a bit field where each bit represents a
                    // memory type. If the bit is set to 1 it means we can use
                    // that type for our buffer. So this code finds the first
                    // memory type that has a `1` (or, is allowed), and is
                    // visible to the CPU.
                    buffer_req.type_mask & (1 << id) != 0
                        && mem_type
                            .properties
                            .contains(hal::memory::Properties::CPU_VISIBLE)
                })
                .unwrap()
                .into();
            let mem = lock
                .device
                .allocate_memory(upload_type, Self::UNIFORM_BUFFER_SIZE)
                .map_err(|_| "Failed to allocate device memory for compute uniform buffer")?;
            lock.device
                .bind_buffer_memory(&mem, 0, &mut buf)
                .map_err(|_| "Failed to bind compute uniform buffer to memory")?;
            (buf, mem)
        };

        // Preallocate a block of memory for compute images in device local
        // memory. This serves as memory for all images used in compute other
        // than for image nodes, which are uploaded separately.
        let image_mem = unsafe {
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
            lock.device
                .allocate_memory(memory_type, Self::IMAGE_MEMORY_SIZE)
                .map_err(|_| "Failed to allocate memory region for compute images")?
        };

        Ok(GPUCompute {
            gpu: gpu.clone(),
            command_pool: command_pool,
            shaders: HashMap::new(),
            uniform_buf,
            uniform_mem,
            image_mem,
            image_mem_chunks: (0..Self::N_CHUNKS)
                .map(|id| Chunk {
                    offset: Self::CHUNK_SIZE * id,
                    free: true,
                })
                .collect(),
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

    /// Find the first set of chunks of contiguous free memory that fits the
    /// requested number of bytes
    fn find_free_image_memory(&self, bytes: u64) -> Option<Vec<usize>> {
        let request = bytes / Self::CHUNK_SIZE;
        let mut free = Vec::with_capacity(request as usize);

        for (i, chunk) in self.image_mem_chunks.iter().enumerate() {
            if chunk.free {
                free.push(i);
                if free.len() == request as usize {
                    return Some(free);
                }
            } else {
                free.clear();
            }
        }

        None
    }

    /// Mark the given set of chunks as used.
    fn use_image_memory(&mut self, chunks: &[usize]) {
        for i in chunks {
            self.image_mem_chunks[*i].free = false;
        }
    }

    /// Mark the given set of chunks as free. Memory freed here should no longer
    /// be used!
    fn free_image_memory(&mut self, chunks: &[usize]) {
        for i in chunks {
            self.image_mem_chunks[*i].free = true;
        }
    }
}

impl<B> Drop for GPUCompute<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        // TODO: call device destructors for gpucompute
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
        // TODO: call device destructors for gpurender
        log::info!("Releasing GPU Render resources")
    }
}
