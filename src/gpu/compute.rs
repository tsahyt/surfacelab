use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::{Backend, CommandBuffer, Shader, ShaderType, GPU};

pub struct GPUCompute<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: B::CommandPool,

    // Uniforms
    uniform_buf: B::Buffer,
    uniform_mem: B::Memory,

    // Image Memory Management
    allocs: Cell<AllocId>,
    image_mem: B::Memory,
    image_mem_chunks: RefCell<Vec<Chunk>>,

    // Descriptors
    descriptor_pool: B::DescriptorPool,
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
    const UNIFORM_BUFFER_SIZE: u64 = 1024; // bytes

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

        // Descriptor Pool. We need to set out resource limits here. Since we
        // keep descriptor sets around for each shader after creation, we need a
        // size large enough to accomodate all the nodes.
        let descriptor_pool = unsafe {
            use hal::pso::*;
            lock.device.create_descriptor_pool(
                128,
                &[
                    DescriptorRangeDesc {
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Sampler,
                        count: 1,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::StorageImage,
                        count: 1,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::SampledImage,
                        count: 8,
                    },
                ],
                hal::pso::DescriptorPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "Failed to create descriptor pool")?;

        Ok(GPUCompute {
            gpu: gpu.clone(),
            command_pool: command_pool,

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

            descriptor_pool,
        })
    }

    /// Build a new compute shader given raw SPIR-V. The resulting shader will
    /// destroy itself when dropped. The parent GPU can not be dropped before
    /// all its shaders are dropped!
    pub fn create_shader(&self, spirv: &[u8]) -> Result<Shader<B>, String> {
        let lock = self.gpu.lock().unwrap();
        let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(spirv))
            .map_err(|e| format!("Failed to load SPIR-V: {}", e))?;
        let shader = unsafe { lock.device.create_shader_module(&loaded_spirv) }
            .map_err(|e| format!("Failed to build shader module: {}", e))?;
        Ok(Shader {
            raw: ManuallyDrop::new(shader),
            ty: ShaderType::Compute,
            parent: self.gpu.clone(),
        })
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
    fn find_free_image_memory(&self, bytes: u64) -> Option<(u64, Vec<usize>)> {
        let request = bytes / Self::CHUNK_SIZE;
        let mut free = Vec::with_capacity(request as usize);
        let mut offset = 0;

        for (i, chunk) in self.image_mem_chunks.borrow().iter().enumerate() {
            if chunk.alloc.is_none() {
                free.push(i);
                if free.len() == request as usize {
                    return Some((offset, free));
                }
            } else {
                offset = i as u64 * Self::CHUNK_SIZE;
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

    /// Fill the uniform buffer with the given data. The data *must* fit into
    /// UNIFORM_BUFFER_SIZE.
    pub fn fill_uniforms(&self, uniforms: &[u8]) -> Result<(), String> {
        debug_assert!(uniforms.len() <= Self::UNIFORM_BUFFER_SIZE as usize);

        let lock = self.gpu.lock().unwrap();

        unsafe {
            let mapping = lock
                .device
                .map_memory(&self.uniform_mem, 0..Self::UNIFORM_BUFFER_SIZE)
                .map_err(|e| {
                    format!("Failed to map uniform buffer into CPU address space: {}", e)
                })?;
            std::ptr::copy_nonoverlapping(
                uniforms.as_ptr() as *const u8,
                mapping,
                uniforms.len() as usize,
            );
            lock.device.unmap_memory(&self.uniform_mem);
        }

        Ok(())
    }

    /// Create a new compute pipeline, given a shader and a set of bindings.
    pub fn create_pipeline(
        &self,
        shader: &Shader<B>,
        bindings: &[hal::pso::DescriptorSetLayoutBinding],
    ) -> Result<ComputePipeline<B>, String> {
        let lock = self.gpu.lock().unwrap();

        // Layouts
        let set_layout = unsafe { lock.device.create_descriptor_set_layout(bindings, &[]) }
            .map_err(|_| "Failed to create descriptor set layout")?;
        let pipeline_layout = unsafe { lock.device.create_pipeline_layout(Some(&set_layout), &[]) }
            .map_err(|_| "Failed to create pipeline layout")?;

        let entry_point = hal::pso::EntryPoint {
            entry: "main",
            module: &*shader.raw,
            specialization: hal::pso::Specialization::default(),
        };

        // Pipeline
        let pipeline = unsafe {
            lock.device.create_compute_pipeline(
                &hal::pso::ComputePipelineDesc::new(entry_point, &pipeline_layout),
                None,
            )
        }
        .map_err(|_| "Failed to create pipeline")?;

        Ok(ComputePipeline {
            parent: self,
            raw: ManuallyDrop::new(pipeline),
            set_layout: ManuallyDrop::new(set_layout),
            pipeline_layout: ManuallyDrop::new(pipeline_layout),
        })
    }

    /// Obtain a new descriptor set from the command pool.
    pub fn allocate_descriptor_set(
        &mut self,
        layout: &B::DescriptorSetLayout,
    ) -> Result<B::DescriptorSet, String> {
        unsafe { self.descriptor_pool.allocate_set(layout) }
            .map_err(|e| format!("Failed to allocate descriptor set: {}", e))
    }

    /// Specifying the parameters of a descriptor set write operation
    pub fn write_descriptor_sets<'a, I, J>(&self, write_iter: I)
    where
        I: IntoIterator<Item = hal::pso::DescriptorSetWrite<'a, B, J>>,
        J: IntoIterator,
        J::Item: std::borrow::Borrow<hal::pso::Descriptor<'a, B>>,
    {
        let lock = self.gpu.lock().unwrap();
        unsafe { lock.device.write_descriptor_sets(write_iter) };
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
    offset: u64,
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
    pub fn allocate_memory(&mut self, compute: &GPUCompute<B>) -> Result<(), String> {
        log::trace!("Allocating memory for image");
        debug_assert!(self.alloc.is_none());

        // Handle memory manager
        let bytes = self.size as u64 * self.size as u64 * 4;
        let (offset, chunks) = compute
            .find_free_image_memory(bytes)
            .ok_or("Unable to find free memory for image")?;
        let alloc = compute.allocate_image_memory(&chunks);
        self.alloc = Some(Rc::new(Alloc {
            parent: self.parent,
            id: alloc,
            offset,
        }));

        let lock = compute.gpu.lock().unwrap();
        unsafe {
            lock.device
                .bind_image_memory(&compute.image_mem, offset, &mut self.raw)
        }
        .map_err(|_| "Failed to bind Image to memory")?;

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

    pub fn ensure_alloc(&mut self, compute: &GPUCompute<B>) -> Result<(), String> {
        if self.alloc.is_none() {
            return self.allocate_memory(compute);
        }

        log::trace!("Reusing existing allocation");

        Ok(())
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

pub struct ComputePipeline<B: Backend> {
    parent: *const GPUCompute<B>,
    raw: ManuallyDrop<B::ComputePipeline>,
    set_layout: ManuallyDrop<B::DescriptorSetLayout>,
    pipeline_layout: ManuallyDrop<B::PipelineLayout>,
}

impl<B> ComputePipeline<B>
where
    B: Backend,
{
    /// Get descriptor set layout.
    pub fn set_layout(&self) -> &B::DescriptorSetLayout {
        &*self.set_layout
    }
}

impl<B> Drop for ComputePipeline<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::debug!("Dropping compute pipeline");

        let parent = unsafe { &*self.parent };

        {
            let lock = parent.gpu.lock().unwrap();
            unsafe {
                lock.device
                    .destroy_descriptor_set_layout(ManuallyDrop::take(&mut self.set_layout));
                lock.device
                    .destroy_pipeline_layout(ManuallyDrop::take(&mut self.pipeline_layout));
                lock.device
                    .destroy_compute_pipeline(ManuallyDrop::take(&mut self.raw));
            }
        }
    }
}
