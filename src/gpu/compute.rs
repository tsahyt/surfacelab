use crate::lang;

use gfx_hal as hal;
use gfx_hal::prelude::*;
use smallvec::{smallvec, SmallVec};
use std::borrow::Borrow;
use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use super::{Backend, Shader, ShaderType, GPU};

pub struct GPUCompute<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: ManuallyDrop<B::CommandPool>,

    // Uniforms and Sampler
    uniform_buf: ManuallyDrop<B::Buffer>,
    uniform_mem: ManuallyDrop<B::Memory>,
    sampler: ManuallyDrop<B::Sampler>,

    // Image Memory Management
    allocs: Cell<AllocId>,
    image_mem: ManuallyDrop<B::Memory>,
    image_mem_chunks: RefCell<Vec<Chunk>>, // TODO: compute image handling without refcell

    // Descriptors
    descriptor_pool: ManuallyDrop<B::DescriptorPool>,

    // Thumbnails
    thumbnail_cache: ThumbnailCache<B>,

    // Sync
    fence: ManuallyDrop<B::Fence>,
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
    const UNIFORM_BUFFER_SIZE: u64 = 2048; // bytes

    const IMAGE_MEMORY_SIZE: u64 = 1024 * 1024 * 128; // bytes
    const CHUNK_SIZE: u64 = 256 * 256 * 4; // bytes
    const N_CHUNKS: u64 = Self::IMAGE_MEMORY_SIZE / Self::CHUNK_SIZE;

    /// Create a new GPUCompute instance.
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, String> {
        log::info!("Obtaining GPU Compute Resources");

        // Thumbnail Data. Produce this first before we lock the GPU for the
        // rest of the constructor, otherwise we get a deadlock.
        let thumbnail_cache = ThumbnailCache::new(gpu.clone());

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
            let ops = crate::lang::AtomicOperator::all_default().len();

            lock.device.create_descriptor_pool(
                ops,
                &[
                    DescriptorRangeDesc {
                        ty: DescriptorType::Buffer {
                            ty: BufferDescriptorType::Uniform,
                            format: BufferDescriptorFormat::Structured {
                                dynamic_offset: false,
                            },
                        },
                        count: ops,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Sampler,
                        count: ops,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Image {
                            ty: ImageDescriptorType::Storage { read_only: false },
                        },
                        count: ops,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Image {
                            ty: ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 8 * ops,
                    },
                ],
                DescriptorPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "Failed to create descriptor pool")?;

        let fence = ManuallyDrop::new(lock.device.create_fence(false).unwrap());

        // Initialize sampler
        let sampler = unsafe {
            lock.device.create_sampler(&hal::image::SamplerDesc::new(
                hal::image::Filter::Linear,
                hal::image::WrapMode::Tile,
            ))
        }
        .map_err(|_| "Failed to create sampler")?;

        Ok(GPUCompute {
            gpu: gpu.clone(),
            command_pool: ManuallyDrop::new(command_pool),

            uniform_buf: ManuallyDrop::new(uniform_buf),
            uniform_mem: ManuallyDrop::new(uniform_mem),
            sampler: ManuallyDrop::new(sampler),

            allocs: Cell::new(0),
            image_mem: ManuallyDrop::new(image_mem),
            image_mem_chunks: RefCell::new(
                (0..Self::N_CHUNKS)
                    .map(|id| Chunk {
                        offset: Self::CHUNK_SIZE * id,
                        alloc: None,
                    })
                    .collect(),
            ),

            descriptor_pool: ManuallyDrop::new(descriptor_pool),

            thumbnail_cache,
            fence,
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

    pub fn create_compute_image(
        &self,
        size: u32,
        ty: lang::ImageType,
        upload: bool,
    ) -> Result<Image<B>, String> {
        let lock = self.gpu.lock().unwrap();

        // Determine formats and sizes
        let format = match ty {
            lang::ImageType::Grayscale => hal::format::Format::R32Sfloat,

            // We use Rgba16 internally on the GPU even though it wastes an
            // entire 16 bit wide channel. The reason here is that the Vulkan
            // spec does not require Rgb16 support. Many GPUs do support it but
            // some may not, and thus requiring it would impose an arbitrary
            // restriction. It might be possible to make this conditional on the
            // specific GPU.
            lang::ImageType::Rgb => hal::format::Format::Rgba16Sfloat,
        };
        let px_width = ty.gpu_bytes_per_pixel();

        // Create device image
        let image = unsafe {
            lock.device.create_image(
                hal::image::Kind::D2(size, size, 1, 1),
                1,
                format,
                hal::image::Tiling::Optimal,
                hal::image::Usage::SAMPLED
                    | if !upload {
                        hal::image::Usage::STORAGE
                    } else {
                        hal::image::Usage::empty()
                    }
                    | hal::image::Usage::TRANSFER_SRC
                    | if upload {
                        hal::image::Usage::TRANSFER_DST
                    } else {
                        hal::image::Usage::empty()
                    },
                hal::image::ViewCapabilities::empty(),
            )
        }
        .map_err(|_| "Failed to create compute image")?;

        Ok(Image {
            parent: self,
            size,
            px_width,
            raw: ManuallyDrop::new(image),
            layout: Cell::new(hal::image::Layout::Undefined),
            access: Cell::new(hal::image::Access::empty()),
            view: ManuallyDrop::new(None),
            alloc: None,
            format,
            alive: Arc::new(()),
        })
    }

    /// Find the first set of chunks of contiguous free memory that fits the
    /// requested number of bytes
    fn find_free_image_memory(&self, bytes: u64) -> Option<(u64, Vec<usize>)> {
        let request = bytes.max(Self::CHUNK_SIZE) / Self::CHUNK_SIZE;
        let mut free = Vec::with_capacity(request as usize);
        let mut offset = 0;

        for (i, chunk) in self.image_mem_chunks.borrow().iter().enumerate() {
            if chunk.alloc.is_none() {
                free.push(i);
                if free.len() == request as usize {
                    return Some((offset, free));
                }
            } else {
                offset = (i + 1) as u64 * Self::CHUNK_SIZE;
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
                .map_memory(
                    &self.uniform_mem,
                    hal::memory::Segment {
                        offset: 0,
                        size: Some(Self::UNIFORM_BUFFER_SIZE),
                    },
                )
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
    pub fn create_pipeline<I>(
        &self,
        shader: &Shader<B>,
        bindings: I,
    ) -> Result<ComputePipeline<B>, String>
    where
        I: IntoIterator,
        I::Item: Borrow<hal::pso::DescriptorSetLayoutBinding>,
    {
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
            raw: pipeline,
            set_layout,
            pipeline_layout,
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

    pub fn run_pipeline(
        &mut self,
        image_size: u32,
        input_images: Vec<&Image<B>>,
        output_images: Vec<&Image<B>>,
        pipeline: &ComputePipeline<B>,
        descriptors: &B::DescriptorSet,
    ) {
        let start_time = Instant::now();
        unsafe {
            let lock = self.gpu.lock().unwrap();
            lock.device.reset_fence(&self.fence).unwrap();
        }

        let pre_barriers: Vec<_> = {
            let input_barriers = input_images.iter().map(|i| {
                i.barrier_to(
                    hal::image::Access::SHADER_READ,
                    hal::image::Layout::ShaderReadOnlyOptimal,
                )
            });
            let output_barriers = output_images.iter().map(|i| {
                i.barrier_to(
                    hal::image::Access::SHADER_WRITE,
                    hal::image::Layout::General,
                )
            });
            input_barriers.chain(output_barriers).collect()
        };

        let command_buffer = unsafe {
            let mut command_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::COMPUTE_SHADER,
                hal::memory::Dependencies::empty(),
                &pre_barriers,
            );
            command_buffer.bind_compute_pipeline(&pipeline.raw);
            command_buffer.bind_compute_descriptor_sets(
                &pipeline.pipeline_layout,
                0,
                Some(descriptors),
                &[],
            );
            command_buffer.dispatch([image_size / 8, image_size / 8, 1]);
            command_buffer.finish();
            command_buffer
        };

        unsafe {
            let mut lock = self.gpu.lock().unwrap();
            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&command_buffer), Some(&self.fence));
            lock.device.wait_for_fence(&self.fence, !0).unwrap();
            self.command_pool.free(Some(command_buffer));
        }

        log::debug!(
            "Pipeline executed in {}µs",
            start_time.elapsed().as_micros()
        );
    }

    pub fn uniform_buffer(&self) -> &B::Buffer {
        &self.uniform_buf
    }

    pub fn sampler(&self) -> &B::Sampler {
        &self.sampler
    }

    /// Download a raw image from the GPU by copying it into a temporary CPU
    /// visible buffer.
    pub fn download_image(&mut self, image: &Image<B>) -> Result<Vec<u8>, String> {
        let mut lock = self.gpu.lock().unwrap();
        let bytes = (image.size * image.size * image.px_width as u32) as u64;

        // Create, allocate, and bind download buffer. We need this buffer
        // because the image is otherwise not in host readable memory!
        let mut buf = unsafe {
            lock.device
                .create_buffer(bytes, hal::buffer::Usage::TRANSFER_DST)
        }
        .map_err(|_| "Cannot create download buffer")?;

        let buf_req = unsafe { lock.device.get_buffer_requirements(&buf) };
        let mem_type = lock
            .memory_properties
            .memory_types
            .iter()
            .enumerate()
            .position(|(id, mem_type)| {
                buf_req.type_mask & (1 << id) != 0
                    && mem_type
                        .properties
                        .contains(hal::memory::Properties::CPU_VISIBLE)
            })
            .unwrap()
            .into();
        let mem = unsafe { lock.device.allocate_memory(mem_type, bytes) }
            .map_err(|_| "Failed to allocate device memory for download buffer")?;

        unsafe {
            lock.device
                .bind_buffer_memory(&mem, 0, &mut buf)
                .map_err(|_| "Failed to bind download buffer to memory")?
        };

        // Reset fence
        unsafe {
            lock.device.reset_fence(&self.fence).unwrap();
        }

        // Build barrier
        let barrier = image.barrier_to(
            hal::image::Access::TRANSFER_READ,
            hal::image::Layout::TransferSrcOptimal,
        );

        // Copy image to buffer
        unsafe {
            let mut command_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[barrier],
            );
            command_buffer.copy_image_to_buffer(
                &image.raw,
                hal::image::Layout::TransferSrcOptimal,
                &buf,
                Some(hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: image.size,
                    buffer_height: image.size,
                    image_offset: hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: hal::image::Extent {
                        width: image.size,
                        height: image.size,
                        depth: 1,
                    },
                    image_layers: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                }),
            );
            command_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&command_buffer), Some(&self.fence));
            lock.device.wait_for_fence(&self.fence, !0).unwrap();
            self.command_pool.free(Some(command_buffer));
        };

        // Download
        let res = unsafe {
            let mapping = lock
                .device
                .map_memory(
                    &mem,
                    hal::memory::Segment {
                        offset: 0,
                        size: Some(bytes),
                    },
                )
                .map_err(|e| {
                    format!(
                        "Failed to map download buffer into CPU address space: {}",
                        e
                    )
                })?;
            let slice = std::slice::from_raw_parts::<u8>(mapping as *const u8, bytes as usize);
            let owned = slice.to_owned();
            lock.device.unmap_memory(&mem);
            owned
        };

        // Clean Up
        unsafe {
            lock.device.free_memory(mem);
            lock.device.destroy_buffer(buf);
        }

        Ok(res)
    }

    /// Upload image. This assumes the image to be allocated!
    pub fn upload_image(&mut self, image: &Image<B>, buffer: &[u16]) -> Result<(), String> {
        debug_assert!(image.alloc.is_some());
        let mut lock = self.gpu.lock().unwrap();
        let bytes = (image.size * image.size * image.px_width as u32) as u64;

        // Create and allocate staging buffer in host readable memory.
        let mut buf = unsafe {
            lock.device
                .create_buffer(bytes, hal::buffer::Usage::TRANSFER_SRC)
        }
        .map_err(|_| "Cannot create upload buffer")?;

        let buf_req = unsafe { lock.device.get_buffer_requirements(&buf) };
        let mem_type = lock
            .memory_properties
            .memory_types
            .iter()
            .enumerate()
            .position(|(id, mem_type)| {
                buf_req.type_mask & (1 << id) != 0
                    && mem_type
                        .properties
                        .contains(hal::memory::Properties::CPU_VISIBLE)
            })
            .unwrap()
            .into();
        let mem = unsafe { lock.device.allocate_memory(mem_type, bytes) }
            .map_err(|_| "Failed to allocate device memory for upload buffer")?;

        unsafe {
            lock.device
                .bind_buffer_memory(&mem, 0, &mut buf)
                .map_err(|_| "Failed to bind download buffer to memory")?
        };

        // Upload image to staging buffer
        unsafe {
            let mapping = lock
                .device
                .map_memory(
                    &mem,
                    hal::memory::Segment {
                        offset: 0,
                        size: Some(bytes),
                    },
                )
                .unwrap();
            let u8s: &[u8] =
                std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len() * 2);
            std::ptr::copy_nonoverlapping(u8s.as_ptr(), mapping, bytes as usize);
            lock.device.unmap_memory(&mem);
        }

        // Reset fence
        unsafe {
            lock.device.reset_fence(&self.fence).unwrap();
        }

        // Build barrier

        // Copy buffer to image
        unsafe {
            let mut command_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[image.barrier_to(
                    hal::image::Access::TRANSFER_WRITE,
                    hal::image::Layout::TransferDstOptimal,
                )],
            );
            command_buffer.copy_buffer_to_image(
                &buf,
                &image.raw,
                hal::image::Layout::TransferDstOptimal,
                Some(hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: image.size,
                    buffer_height: image.size,
                    image_offset: hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: hal::image::Extent {
                        width: image.size,
                        height: image.size,
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
                hal::pso::PipelineStage::TRANSFER..hal::pso::PipelineStage::COMPUTE_SHADER,
                hal::memory::Dependencies::empty(),
                &[image.barrier_to(
                    hal::image::Access::SHADER_READ,
                    hal::image::Layout::ShaderReadOnlyOptimal,
                )],
            );
            command_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&command_buffer), Some(&self.fence));
            lock.device.wait_for_fence(&self.fence, !0).unwrap();
            self.command_pool.free(Some(command_buffer));
        }

        // Cleanup
        unsafe {
            lock.device.free_memory(mem);
            lock.device.destroy_buffer(buf);
        }

        Ok(())
    }

    /// Copy data between images. This assumes that both images are already allocated!
    pub fn copy_image(&mut self, from: &Image<B>, to: &Image<B>) -> Result<(), String> {
        let mut lock = self.gpu.lock().unwrap();

        unsafe { lock.device.reset_fence(&self.fence).unwrap() };

        unsafe {
            let mut cmd_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[
                    hal::memory::Barrier::Image {
                        states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                            ..(
                                hal::image::Access::TRANSFER_WRITE,
                                hal::image::Layout::TransferDstOptimal,
                            ),
                        target: &*to.raw,
                        families: None,
                        range: super::COLOR_RANGE.clone(),
                    },
                    from.barrier_to(
                        hal::image::Access::TRANSFER_READ,
                        hal::image::Layout::TransferSrcOptimal,
                    ),
                ],
            );
            cmd_buffer.blit_image(
                &*from.raw,
                hal::image::Layout::TransferSrcOptimal,
                &*to.raw,
                hal::image::Layout::TransferDstOptimal,
                hal::image::Filter::Nearest,
                &[hal::command::ImageBlit {
                    src_subresource: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    src_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                        x: from.size as i32,
                        y: from.size as i32,
                        z: 1,
                    },
                    dst_subresource: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    dst_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                        x: to.size as i32,
                        y: to.size as i32,
                        z: 1,
                    },
                }],
            );
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TRANSFER..hal::pso::PipelineStage::COMPUTE_SHADER,
                hal::memory::Dependencies::empty(),
                &[from.barrier_to(
                    hal::image::Access::SHADER_READ,
                    hal::image::Layout::ShaderReadOnlyOptimal,
                )],
            );
            cmd_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&cmd_buffer), Some(&self.fence));
            lock.device.wait_for_fence(&self.fence, !0).unwrap();
            self.command_pool.free(Some(cmd_buffer));
        }

        Ok(())
    }

    /// Create a thumbnail of the given image and return it
    pub fn generate_thumbnail(
        &mut self,
        image: &Image<B>,
        thumbnail: &ThumbnailIndex,
    ) -> Result<(), String> {
        let mut lock = self.gpu.lock().unwrap();
        unsafe { lock.device.reset_fence(&self.fence).unwrap() };

        let thumbnail_image = self.thumbnail_cache.image(thumbnail);

        // Blit image to thumbnail size
        unsafe {
            let mut cmd_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            cmd_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[
                    hal::memory::Barrier::Image {
                        states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                            ..(
                                hal::image::Access::TRANSFER_WRITE,
                                hal::image::Layout::TransferDstOptimal,
                            ),
                        target: thumbnail_image,
                        families: None,
                        range: super::COLOR_RANGE.clone(),
                    },
                    image.barrier_to(
                        hal::image::Access::TRANSFER_READ,
                        hal::image::Layout::TransferSrcOptimal,
                    ),
                ],
            );
            cmd_buffer.blit_image(
                &*image.raw,
                hal::image::Layout::TransferSrcOptimal,
                thumbnail_image,
                hal::image::Layout::TransferDstOptimal,
                hal::image::Filter::Nearest,
                &[hal::command::ImageBlit {
                    src_subresource: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    src_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                        x: image.size as i32,
                        y: image.size as i32,
                        z: 1,
                    },
                    dst_subresource: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    dst_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                        x: self.thumbnail_cache.thumbnail_size() as _,
                        y: self.thumbnail_cache.thumbnail_size() as _,
                        z: 1,
                    },
                }],
            );
            // FIXME: thumbnail format transition doesn't work
            // cmd_buffer.pipeline_barrier(
            //     hal::pso::PipelineStage::TRANSFER..hal::pso::PipelineStage::COMPUTE_SHADER,
            //     hal::memory::Dependencies::empty(),
            //     &[image.barrier_to(
            //         hal::image::Access::SHADER_READ,
            //         hal::image::Layout::ShaderReadOnlyOptimal,
            //     )],
            // );
            cmd_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&cmd_buffer), Some(&self.fence));
            lock.device.wait_for_fence(&self.fence, !0).unwrap();
            self.command_pool.free(Some(cmd_buffer));
        }

        Ok(())
    }

    /// Get a new thumbnail from the cache
    pub fn new_thumbnail(&mut self, grayscale: bool) -> ThumbnailIndex {
        self.thumbnail_cache.next(grayscale)
    }

    /// Return a thumbnail the cache. Thumbnail to thumbnail, dust to dust.
    pub fn return_thumbnail(&mut self, thumbnail: ThumbnailIndex) {
        self.thumbnail_cache.free(thumbnail);
    }

    pub fn view_thumbnail(&self, thumbnail: &ThumbnailIndex) -> &B::ImageView {
        self.thumbnail_cache.image_view(thumbnail)
    }

    pub fn alive_thumbnail(&self, thumbnail: &ThumbnailIndex) -> Weak<()> {
        self.thumbnail_cache.alive(thumbnail)
    }
}

impl<B> Drop for GPUCompute<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::info!("Releasing GPU Compute resources");

        let lock = self.gpu.lock().unwrap();
        unsafe {
            lock.device
                .destroy_fence(ManuallyDrop::take(&mut self.fence));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.uniform_mem));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.image_mem));
            lock.device
                .destroy_buffer(ManuallyDrop::take(&mut self.uniform_buf));
            lock.device
                .destroy_sampler(ManuallyDrop::take(&mut self.sampler));
            lock.device
                .destroy_command_pool(ManuallyDrop::take(&mut self.command_pool));
            lock.device
                .destroy_descriptor_pool(ManuallyDrop::take(&mut self.descriptor_pool));
        }
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
    px_width: u8,
    raw: ManuallyDrop<B::Image>,
    layout: Cell<hal::image::Layout>,
    access: Cell<hal::image::Access>,
    view: ManuallyDrop<Option<B::ImageView>>,
    alloc: Option<Alloc<B>>,
    format: hal::format::Format,
    alive: Arc<()>,
}

impl<B> Image<B>
where
    B: Backend,
{
    fn bind_memory(&mut self, offset: u64, compute: &GPUCompute<B>) -> Result<(), String> {
        let lock = compute.gpu.lock().unwrap();

        unsafe {
            lock.device
                .bind_image_memory(&compute.image_mem, offset, &mut self.raw)
        }
        .map_err(|_| "Failed to bind Image to memory")?;

        // Create view once the image is bound
        let view = unsafe {
            lock.device.create_image_view(
                &self.raw,
                hal::image::ViewKind::D2,
                self.format,
                hal::format::Swizzle::NO,
                super::COLOR_RANGE.clone(),
            )
        }
        .map_err(|_| "Failed to create image view")?;
        unsafe {
            if let Some(view) = ManuallyDrop::take(&mut self.view) {
                lock.device.destroy_image_view(view);
            }
        }
        self.view = ManuallyDrop::new(Some(view));

        Ok(())
    }

    pub fn barrier_to(
        &self,
        access: hal::image::Access,
        layout: hal::image::Layout,
    ) -> hal::memory::Barrier<B> {
        let old_access = self.access.get();
        let old_layout = self.layout.get();
        self.access.set(access);
        self.layout.set(layout);
        hal::memory::Barrier::Image {
            states: (old_access, old_layout)..(access, layout),
            target: &*self.raw,
            families: None,
            range: super::COLOR_RANGE.clone(),
        }
    }

    /// Allocate fresh memory to the image from the underlying memory pool in compute.
    pub fn allocate_memory(&mut self, compute: &GPUCompute<B>) -> Result<(), String> {
        log::trace!("Allocating memory for {}x{} image", self.size, self.size);
        debug_assert!(self.alloc.is_none());

        // Handle memory manager
        let bytes = self.size as u64 * self.size as u64 * self.px_width as u64;
        let (offset, chunks) = compute
            .find_free_image_memory(bytes)
            .ok_or("Unable to find free memory for image")?;
        let alloc = compute.allocate_image_memory(&chunks);
        self.alloc = Some(Alloc {
            parent: self.parent,
            id: alloc,
            offset,
        });

        // Bind
        self.bind_memory(offset, compute)?;

        Ok(())
    }

    /// Release the Image's hold on the backing memory.
    pub fn free_memory(&mut self, compute: &GPUCompute<B>) {
        log::trace!("Releasing image allocation");
        debug_assert!(self.alloc.is_some());

        unsafe {
            if let Some(view) = ManuallyDrop::take(&mut self.view) {
                let lock = compute.gpu.lock().unwrap();
                lock.device.destroy_image_view(view);
            }
        }

        self.alloc = None;
    }

    /// Determine whether an Image is backed by Device memory
    pub fn is_backed(&self) -> bool {
        self.alloc.is_some()
    }

    /// Ensures that the image is backed. If no memory is currently allocated to
    /// it, new memory will be allocated. May fail if out of memory!
    pub fn ensure_alloc(&mut self, compute: &GPUCompute<B>) -> Result<(), String> {
        if self.alloc.is_none() {
            return self.allocate_memory(compute);
        }

        log::trace!("Reusing existing allocation");

        Ok(())
    }

    /// Get a view to the image
    pub fn get_view(&self) -> Option<&B::ImageView> {
        match &*self.view {
            Some(view) => Some(view),
            None => None,
        }
    }

    /// Get the current layout of the image
    pub fn get_layout(&self) -> hal::image::Layout {
        self.layout.get()
    }

    pub fn get_access(&self) -> hal::image::Access {
        self.access.get()
    }

    pub fn get_size(&self) -> u32 {
        self.size
    }

    /// Get the raw image
    pub fn get_raw(&self) -> &B::Image {
        &*self.raw
    }

    /// Get the live status of the image
    pub fn alive(&self) -> super::ResourceAlive {
        Arc::downgrade(&self.alive)
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
                if let Some(view) = ManuallyDrop::take(&mut self.view) {
                    lock.device.destroy_image_view(view);
                }
                lock.device.destroy_image(ManuallyDrop::take(&mut self.raw));
            }
        }
    }
}

// NOTE: The resources claimed by a compute pipeline are never cleaned up.
// gfx-hal has functions to do so, but it of course requires a handle back to
// the parent GPU object, which is difficult to do in Rust with our design. A
// previous solution caused a Poison Error on cleanup. It seems like the current
// approach actually works and there are no errors from the validation layers.
pub struct ComputePipeline<B: Backend> {
    raw: B::ComputePipeline,
    set_layout: B::DescriptorSetLayout,
    pipeline_layout: B::PipelineLayout,
}

impl<B> ComputePipeline<B>
where
    B: Backend,
{
    /// Get descriptor set layout.
    pub fn set_layout(&self) -> &B::DescriptorSetLayout {
        &self.set_layout
    }
}

/// An index into the thumbnail cache
pub struct ThumbnailIndex(usize);

pub struct ThumbnailCache<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    memory: SmallVec<[B::Memory; 4]>,
    images: Vec<Option<B::Image>>,
    views: Vec<Option<B::ImageView>>,
    alive: Vec<Option<Arc<()>>>,
}

impl<B> Drop for ThumbnailCache<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        let n = self.memory.len() * Self::THUMBNAIL_CHUNK_LENGTH;
        for i in 0..n {
            self.free(ThumbnailIndex(i));
        }

        let lock = self.gpu.lock().unwrap();
        for chunk in self.memory.drain(0..self.memory.len()) {
            unsafe { lock.device.free_memory(chunk) };
        }
    }
}

impl<B> ThumbnailCache<B>
where
    B: Backend,
{
    /// Pixel per side
    pub const THUMBNAIL_SIZE: usize = 128;

    /// Size of a single thumbnail in bytes
    const THUMBNAIL_BYTES: usize = Self::THUMBNAIL_SIZE * Self::THUMBNAIL_SIZE * 4;

    /// Format of thumbnails
    const THUMBNAIL_FORMAT: hal::format::Format = hal::format::Format::Rgba8Unorm;

    /// Size of a single allocation, in number of thumbnails. 512 is roughly 32M in memory
    const THUMBNAIL_CHUNK_LENGTH: usize = 512;

    /// Swizzle setting for grayscale images
    const GRAYSCALE_SWIZZLE: hal::format::Swizzle = hal::format::Swizzle(
        hal::format::Component::R,
        hal::format::Component::R,
        hal::format::Component::R,
        hal::format::Component::A,
    );

    /// Create a new thumbnail cache
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Self {
        let chunk = {
            let lock = gpu.lock().unwrap();

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

            let chunk = unsafe {
                lock.device.allocate_memory(
                    memory_type,
                    Self::THUMBNAIL_CHUNK_LENGTH as u64 * Self::THUMBNAIL_BYTES as u64,
                )
            }
            .expect("Error allocating thumbnail memory");

            chunk
        };

        let memory = smallvec![chunk];

        Self {
            gpu,
            memory,
            images: (0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None).collect(),
            views: (0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None).collect(),
            alive: (0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None).collect(),
        }
    }

    /// Obtain the next free thumbnail index from the cache. This will set up
    /// all required internal data structures.
    pub fn next(&mut self, grayscale: bool) -> ThumbnailIndex {
        if let Some(i) = self
            .images
            .iter()
            .enumerate()
            .filter(|(_, x)| x.is_none())
            .map(|(i, _)| i)
            .next()
        {
            self.new_thumbnail_at(i, grayscale);
            ThumbnailIndex(i)
        } else {
            self.grow();
            self.next(grayscale)
        }
    }

    fn new_thumbnail_at(&mut self, i: usize, grayscale: bool) {
        let lock = self.gpu.lock().unwrap();

        let mut image = unsafe {
            lock.device.create_image(
                hal::image::Kind::D2(Self::THUMBNAIL_SIZE as _, Self::THUMBNAIL_SIZE as _, 1, 1),
                1,
                Self::THUMBNAIL_FORMAT,
                hal::image::Tiling::Linear,
                hal::image::Usage::TRANSFER_DST | hal::image::Usage::SAMPLED,
                hal::image::ViewCapabilities::empty(),
            )
        }
        .expect("Error creating thumbnail image");

        let mem = &self.memory[i / Self::THUMBNAIL_CHUNK_LENGTH];
        let offset = (i % Self::THUMBNAIL_CHUNK_LENGTH) * Self::THUMBNAIL_BYTES;

        unsafe {
            lock.device
                .bind_image_memory(mem, offset as u64, &mut image)
        }
        .expect("Error binding thumbnail memory");

        let view = unsafe {
            lock.device.create_image_view(
                &image,
                hal::image::ViewKind::D2,
                Self::THUMBNAIL_FORMAT,
                if grayscale {
                    Self::GRAYSCALE_SWIZZLE
                } else {
                    hal::format::Swizzle::NO
                },
                super::COLOR_RANGE.clone(),
            )
        }
        .expect("Error creating thumbnail image view");

        self.images[i] = Some(image);
        self.views[i] = Some(view);
        self.alive[i] = Some(Arc::new(()));
    }

    fn grow(&mut self) {
        let new_chunk = {
            let lock = self.gpu.lock().unwrap();

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

            let chunk = unsafe {
                lock.device.allocate_memory(
                    memory_type,
                    Self::THUMBNAIL_CHUNK_LENGTH as u64 * Self::THUMBNAIL_BYTES as u64,
                )
            }
            .expect("Error allocating thumbnail memory");

            chunk
        };

        self.memory.push(new_chunk);

        self.images
            .extend((0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None));
        self.views
            .extend((0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None));
        self.alive
            .extend((0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None));
    }

    /// Get the underlying Image from a thumbnail
    pub fn image(&self, index: &ThumbnailIndex) -> &B::Image {
        self.images[index.0].as_ref().unwrap()
    }

    /// Get the underlying image view from a thumbnail
    pub fn image_view(&self, index: &ThumbnailIndex) -> &B::ImageView {
        self.views[index.0].as_ref().unwrap()
    }

    pub fn alive(&self, index: &ThumbnailIndex) -> super::ResourceAlive {
        Arc::downgrade(self.alive[index.0].as_ref().unwrap())
    }

    /// Free a thumbnail by its index. Note that this takes ownership.
    pub fn free(&mut self, index: ThumbnailIndex) {
        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .destroy_image(self.images[index.0].take().unwrap());
            lock.device
                .destroy_image_view(self.views[index.0].take().unwrap());
        }

        self.images[index.0] = None;
        self.views[index.0] = None;
        self.alive[index.0] = None;
    }

    /// The size of a single thumbnail, measured in pixels per side.
    pub fn thumbnail_size(&self) -> usize {
        Self::THUMBNAIL_SIZE
    }
}
