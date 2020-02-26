use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::{Backend, CommandBuffer, Shader, ShaderType, GPU};

pub struct GPUCompute<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: B::CommandPool,
    shaders: HashMap<&'static str, B::ShaderModule>,

    // Uniforms
    uniform_buf: B::Buffer,
    uniform_mem: B::Memory,

    // Image Memory Management
    allocs: Cell<AllocId>,
    image_mem: B::Memory,
    image_mem_chunks: RefCell<Vec<Chunk>>,
}

type AllocId = u16;

#[derive(Debug, Clone)]
struct Chunk {
    offset: u64,
    alloc: Option<AllocId>,
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

            allocs: Cell::new(0),
            image_mem,
            image_mem_chunks: RefCell::new(
                (0..Self::N_CHUNKS)
                    .map(|id| Chunk {
                        offset: Self::CHUNK_SIZE * id,
                        alloc: None,
                    })
                    .collect(),
            ),
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

    pub fn create_compute_image<'a>(&'a self, size: u32) -> Result<Image<B>, String> {
        let image = {
            let lock = self.gpu.lock().unwrap();
            unsafe {
                lock.device.create_image(
                    hal::image::Kind::D2(size, size, 1, 1),
                    1,
                    hal::format::Format::R32Sfloat,
                    hal::image::Tiling::Optimal,
                    hal::image::Usage::SAMPLED | hal::image::Usage::STORAGE,
                    hal::image::ViewCapabilities::empty(),
                )
            }
            .map_err(|_| "Failed to create image")?
        };

        Ok(Image {
            parent: self,
            size,
            raw: ManuallyDrop::new(image),
            alloc: None,
        })
    }

    /// Find the first set of chunks of contiguous free memory that fits the
    /// requested number of bytes
    fn find_free_image_memory(&self, bytes: u64) -> Option<Vec<usize>> {
        let request = bytes / Self::CHUNK_SIZE;
        let mut free = Vec::with_capacity(request as usize);

        for (i, chunk) in self.image_mem_chunks.borrow().iter().enumerate() {
            if chunk.alloc.is_none() {
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

    /// Mark the given set of chunks as used. Assumes that the chunks were
    /// previously free!
    fn allocate_image_memory(&self, chunks: &[usize]) -> AllocId {
        let alloc = self.allocs.get();
        for i in chunks {
            self.image_mem_chunks.borrow_mut()[*i].alloc = Some(alloc);
        }
        self.allocs.set(alloc + 1);
        alloc
    }

    /// Mark the given set of chunks as free. Memory freed here should no longer
    /// be used!
    fn free_image_memory(&self, alloc: AllocId) {
        for mut chunk in self
            .image_mem_chunks
            .borrow_mut()
            .iter_mut()
            .filter(|c| c.alloc == Some(alloc))
        {
            chunk.alloc = None;
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

#[derive(Debug, Clone)]
pub struct Alloc<B: Backend> {
    parent: *const GPUCompute<B>,
    id: AllocId,
}

impl<B> Drop for Alloc<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::trace!("Release image memory for allocation {}", self.id);
        let parent = unsafe { &*self.parent };
        parent.free_image_memory(self.id);
    }
}

pub struct Image<B: Backend> {
    parent: *const GPUCompute<B>,
    size: u32,
    raw: ManuallyDrop<B::Image>,
    alloc: Option<Rc<Alloc<B>>>,
}

impl<B> Image<B>
where
    B: Backend,
{
    /// Allocate fresh memory to the image from the underlying memory pool in compute.
    pub fn allocate_memory(&mut self, compute: &mut GPUCompute<B>) -> Result<(), String> {
        log::trace!("Allocating memory for image");
        debug_assert!(self.alloc.is_none());

        let bytes = self.size as u64 * self.size as u64 * 4;
        let chunks = compute
            .find_free_image_memory(bytes)
            .ok_or("Unable to find free memory for image")?;
        let alloc = compute.allocate_image_memory(&chunks);
        self.alloc = Some(Rc::new(Alloc {
            parent: self.parent,
            id: alloc,
        }));

        Ok(())
    }

    /// Release the Image's hold on the backing memory. Note that this does
    /// *not* necessarily free the underlying memory block, since there may be
    /// other references to it!
    pub fn free_memory(&mut self) {
        log::trace!("Releasing image allocation");
        debug_assert!(self.alloc.is_some());
        self.alloc = None;
    }

    /// Determine whether an Image is backed by Device memory
    pub fn is_backed(&self) -> bool {
        self.alloc.is_some()
    }

    /// Use the memory region from another image. This will increase the
    /// reference count on the underlying memory region.
    pub fn use_memory_from(&mut self, alloc: Rc<Alloc<B>>) {
        log::trace!("Transferring allocation");
        self.alloc = Some(alloc);
    }

    /// Returns a clone of the underlying allocation
    pub fn get_alloc(&self) -> Option<Rc<Alloc<B>>> {
        self.alloc.clone()
    }
}

impl<B> Drop for Image<B>
where
    B: Backend,
{
    /// Drop the raw resource. Any allocated memory will only be dropped when
    /// the last reference to it drops.
    fn drop(&mut self) {
        let parent = unsafe { &*self.parent };

        {
            let lock = parent.gpu.lock().unwrap();
            unsafe {
                lock.device.destroy_image(ManuallyDrop::take(&mut self.raw));
            }
        }
    }
}
