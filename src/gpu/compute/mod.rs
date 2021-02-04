use crate::lang;

use gfx_hal as hal;
use gfx_hal::prelude::*;
use smallvec::SmallVec;
use std::borrow::Borrow;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};

pub mod allocator;
pub mod thumbnails;

pub use allocator::{Image, ImageError};
pub use thumbnails::ThumbnailIndex;

use super::{
    Backend, DownloadError, InitializationError, PipelineError, Shader, ShaderType, UploadError,
    GPU,
};

/// GPU side compute component
pub struct GPUCompute<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: ManuallyDrop<B::CommandPool>,

    // Uniforms and Sampler
    uniform_buf: ManuallyDrop<B::Buffer>,
    uniform_mem: ManuallyDrop<B::Memory>,
    sampler: ManuallyDrop<B::Sampler>,

    // Image Memory Management
    allocator: Arc<Mutex<allocator::ComputeAllocator<B>>>,

    // Descriptors
    descriptor_pool: ManuallyDrop<B::DescriptorPool>,

    // Thumbnails
    thumbnail_cache: thumbnails::ThumbnailCache<B>,

    // Sync
    fence: ManuallyDrop<B::Fence>,
}

impl<B> GPUCompute<B>
where
    B: Backend,
{
    /// Size of the uniform buffer for compute shaders, in bytes
    const UNIFORM_BUFFER_SIZE: u64 = 2048;

    /// Create a new GPUCompute instance.
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, InitializationError> {
        log::info!("Obtaining GPU Compute Resources");

        let allocator = allocator::ComputeAllocator::new(gpu.clone())?;

        // Thumbnail Data. Produce this first before we lock the GPU for the
        // rest of the constructor, otherwise we get a deadlock.
        let thumbnail_cache = thumbnails::ThumbnailCache::new(gpu.clone());

        let lock = gpu.lock().unwrap();

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::TRANSIENT,
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Command Pool"))?;

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
                .map_err(|_| InitializationError::ResourceAcquisition("Uniform Buffer"))?;
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
                        && (mem_type.properties.contains(
                            hal::memory::Properties::CPU_VISIBLE
                                | hal::memory::Properties::DEVICE_LOCAL,
                        ) || mem_type.properties.contains(
                            hal::memory::Properties::CPU_VISIBLE
                                | hal::memory::Properties::COHERENT,
                        ))
                })
                .unwrap()
                .into();
            let mem = lock
                .device
                .allocate_memory(upload_type, Self::UNIFORM_BUFFER_SIZE)
                .map_err(|_| InitializationError::Allocation("Uniform Buffer"))?;
            lock.device
                .bind_buffer_memory(&mem, 0, &mut buf)
                .map_err(|_| InitializationError::Bind)?;
            (buf, mem)
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
        .map_err(|_| InitializationError::ResourceAcquisition("Descriptor Pool"))?;

        let fence = ManuallyDrop::new(lock.device.create_fence(false).unwrap());

        // Initialize sampler
        let sampler = unsafe {
            lock.device.create_sampler(&hal::image::SamplerDesc::new(
                hal::image::Filter::Linear,
                hal::image::WrapMode::Tile,
            ))
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Sampler"))?;

        Ok(GPUCompute {
            gpu: gpu.clone(),
            command_pool: ManuallyDrop::new(command_pool),

            uniform_buf: ManuallyDrop::new(uniform_buf),
            uniform_mem: ManuallyDrop::new(uniform_mem),
            sampler: ManuallyDrop::new(sampler),

            allocator: Arc::new(Mutex::new(allocator)),

            descriptor_pool: ManuallyDrop::new(descriptor_pool),

            thumbnail_cache,
            fence,
        })
    }

    /// Build a new compute shader given raw SPIR-V. The resulting shader will
    /// destroy itself when dropped. The parent GPU can not be dropped before
    /// all its shaders are dropped!
    pub fn create_shader(&self, spirv: &[u8]) -> Result<Shader<B>, InitializationError> {
        let lock = self.gpu.lock().unwrap();
        let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(spirv))
            .map_err(|_| InitializationError::ShaderSPIRV)?;
        let shader = unsafe { lock.device.create_shader_module(&loaded_spirv) }
            .map_err(|_| InitializationError::ShaderModule)?;
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
        transfer_dst: bool,
    ) -> Result<Image<B>, InitializationError> {
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
        let px_width = match format {
            hal::format::Format::R32Sfloat => 4,
            hal::format::Format::Rgba16Sfloat => 8,
            _ => panic!("Unsupported compute image format!"),
        };

        // Create device image
        let image = unsafe {
            lock.device.create_image(
                hal::image::Kind::D2(size, size, 1, 1),
                1,
                format,
                hal::image::Tiling::Optimal,
                hal::image::Usage::SAMPLED
                    | if !transfer_dst {
                        hal::image::Usage::STORAGE
                    } else {
                        hal::image::Usage::TRANSFER_DST
                    }
                    | hal::image::Usage::TRANSFER_SRC,
                hal::image::ViewCapabilities::empty(),
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Compute Image"))?;

        Ok(Image::new(
            self.allocator.clone(),
            size,
            px_width,
            image,
            format,
        ))
    }

    /// Fill the uniform buffer with the given data. The data *must* fit into
    /// UNIFORM_BUFFER_SIZE.
    pub fn fill_uniforms(&self, uniforms: &[u8]) -> Result<(), PipelineError> {
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
                .map_err(|_| PipelineError::UniformMapping)?;
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
    ) -> Result<ComputePipeline<B>, InitializationError>
    where
        I: IntoIterator,
        I::Item: Borrow<hal::pso::DescriptorSetLayoutBinding>,
    {
        let lock = self.gpu.lock().unwrap();

        // Layouts
        let set_layout = unsafe { lock.device.create_descriptor_set_layout(bindings, &[]) }
            .map_err(|_| InitializationError::ResourceAcquisition("Descriptor Set Layout"))?;
        let pipeline_layout = unsafe { lock.device.create_pipeline_layout(Some(&set_layout), &[]) }
            .map_err(|_| InitializationError::ResourceAcquisition("Pipeline Layout"))?;

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
        .map_err(|_| InitializationError::ResourceAcquisition("Pipeline"))?;

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
    ) -> Result<B::DescriptorSet, InitializationError> {
        unsafe { self.descriptor_pool.allocate_set(layout) }
            .map_err(|_| InitializationError::Allocation("Failed to allocate descriptor set"))
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

    /// Run a single compute pipeline given input and output images, the size of
    /// the output images, and the descriptors to be used.
    pub fn run_pipeline<'a, I>(
        &mut self,
        image_size: u32,
        input_images: I,
        output_images: I,
        pipeline: &ComputePipeline<B>,
        descriptors: &B::DescriptorSet,
    ) where
        I: IntoIterator<Item = &'a Image<B>> + Clone,
    {
        unsafe {
            let lock = self.gpu.lock().unwrap();
            lock.device.reset_fence(&self.fence).unwrap();
        }

        let input_locks: SmallVec<[_; 6]> = input_images
            .clone()
            .into_iter()
            .map(|i| i.get_raw().lock().unwrap())
            .collect();
        let output_locks: SmallVec<[_; 2]> = output_images
            .clone()
            .into_iter()
            .map(|i| i.get_raw().lock().unwrap())
            .collect();

        let pre_barriers = {
            let input_barriers = input_images.into_iter().enumerate().map(|(k, i)| {
                i.barrier_to(
                    &input_locks[k],
                    hal::image::Access::SHADER_READ,
                    hal::image::Layout::ShaderReadOnlyOptimal,
                )
            });
            let output_barriers = output_images.into_iter().enumerate().map(|(k, i)| {
                i.barrier_to(
                    &output_locks[k],
                    hal::image::Access::SHADER_WRITE,
                    hal::image::Layout::General,
                )
            });
            input_barriers.chain(output_barriers)
        };

        let command_buffer = unsafe {
            let mut command_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::COMPUTE_SHADER,
                hal::memory::Dependencies::empty(),
                pre_barriers,
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
    }

    /// Borrow the uniform buffer
    pub fn uniform_buffer(&self) -> &B::Buffer {
        &self.uniform_buf
    }

    /// Borrow the sampler
    pub fn sampler(&self) -> &B::Sampler {
        &self.sampler
    }

    /// Download a raw image from the GPU by copying it into a CPU visible
    /// buffer.
    pub fn download_image(&mut self, image: &Image<B>) -> Result<Vec<u8>, DownloadError> {
        let mut lock = self.gpu.lock().unwrap();
        let bytes = image.get_bytes() as u64;

        // Create, allocate, and bind download buffer. We need this buffer
        // because the image is otherwise not in host readable memory!
        let mut buf = unsafe {
            lock.device
                .create_buffer(bytes, hal::buffer::Usage::TRANSFER_DST)
        }
        .map_err(|_| DownloadError::Creation)?;

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
            .map_err(|_| DownloadError::Allocation)?;

        unsafe {
            lock.device
                .bind_buffer_memory(&mem, 0, &mut buf)
                .map_err(|_| DownloadError::BufferBind)?
        };

        // Reset fence
        unsafe {
            lock.device.reset_fence(&self.fence).unwrap();
        }

        // Lock image and build barrier
        let image_lock = image.get_raw().lock().unwrap();

        let barrier = image.barrier_to(
            &image_lock,
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
                &image_lock,
                hal::image::Layout::TransferSrcOptimal,
                &buf,
                Some(hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: image.get_size(),
                    buffer_height: image.get_size(),
                    image_offset: hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: hal::image::Extent {
                        width: image.get_size(),
                        height: image.get_size(),
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
                .map_err(|_| DownloadError::Map)?;
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
    pub fn upload_image(&mut self, image: &Image<B>, buffer: &[u16]) -> Result<(), UploadError> {
        debug_assert!(image.is_backed());

        let mut lock = self.gpu.lock().unwrap();
        let bytes = image.get_bytes() as u64;

        // Create and allocate staging buffer in host readable memory.
        let mut buf = unsafe {
            lock.device
                .create_buffer(bytes, hal::buffer::Usage::TRANSFER_SRC)
        }
        .map_err(|_| UploadError::Creation)?;

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
            .map_err(|_| UploadError::Allocation)?;

        unsafe {
            lock.device
                .bind_buffer_memory(&mem, 0, &mut buf)
                .map_err(|_| UploadError::BufferBind)?
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

        // Copy buffer to image
        let image_lock = image.get_raw().lock().unwrap();

        unsafe {
            let mut command_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[image.barrier_to(
                    &image_lock,
                    hal::image::Access::TRANSFER_WRITE,
                    hal::image::Layout::TransferDstOptimal,
                )],
            );
            command_buffer.copy_buffer_to_image(
                &buf,
                &image_lock,
                hal::image::Layout::TransferDstOptimal,
                Some(hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: image.get_size(),
                    buffer_height: image.get_size(),
                    image_offset: hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: hal::image::Extent {
                        width: image.get_size(),
                        height: image.get_size(),
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
                    &image_lock,
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
    pub fn copy_image(&mut self, from: &Image<B>, to: &Image<B>) {
        debug_assert!(from.is_backed() && to.is_backed());
        let mut lock = self.gpu.lock().unwrap();

        unsafe { lock.device.reset_fence(&self.fence).unwrap() };

        let from_lock = from.get_raw().lock().unwrap();
        let to_lock = to.get_raw().lock().unwrap();

        unsafe {
            let mut cmd_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            cmd_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[
                    from.barrier_to(
                        &from_lock,
                        hal::image::Access::TRANSFER_READ,
                        hal::image::Layout::TransferSrcOptimal,
                    ),
                    to.barrier_to(
                        &to_lock,
                        hal::image::Access::TRANSFER_WRITE,
                        hal::image::Layout::TransferDstOptimal,
                    ),
                ],
            );
            cmd_buffer.blit_image(
                &from_lock,
                hal::image::Layout::TransferSrcOptimal,
                &to_lock,
                hal::image::Layout::TransferDstOptimal,
                hal::image::Filter::Nearest,
                &[hal::command::ImageBlit {
                    src_subresource: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    src_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                        x: from.get_size() as i32,
                        y: from.get_size() as i32,
                        z: 1,
                    },
                    dst_subresource: hal::image::SubresourceLayers {
                        aspects: hal::format::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    dst_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                        x: to.get_size() as i32,
                        y: to.get_size() as i32,
                        z: 1,
                    },
                }],
            );
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TRANSFER..hal::pso::PipelineStage::COMPUTE_SHADER,
                hal::memory::Dependencies::empty(),
                &[
                    from.barrier_to(
                        &from_lock,
                        hal::image::Access::SHADER_READ,
                        hal::image::Layout::ShaderReadOnlyOptimal,
                    ),
                    to.barrier_to(
                        &to_lock,
                        hal::image::Access::SHADER_READ,
                        hal::image::Layout::ShaderReadOnlyOptimal,
                    ),
                ],
            );
            cmd_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&cmd_buffer), Some(&self.fence));
            lock.device.wait_for_fence(&self.fence, !0).unwrap();
            self.command_pool.free(Some(cmd_buffer));
        }
    }

    /// Create a thumbnail of the given image and return it
    pub fn generate_thumbnail(&mut self, image: &Image<B>, thumbnail: &ThumbnailIndex) {
        let thumbnail_image = self.thumbnail_cache.image(thumbnail);
        let image_lock = image.get_raw().lock().unwrap();

        let mut lock = self.gpu.lock().unwrap();
        unsafe { lock.device.reset_fence(&self.fence).unwrap() };

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
                        &image_lock,
                        hal::image::Access::TRANSFER_READ,
                        hal::image::Layout::TransferSrcOptimal,
                    ),
                ],
            );
            cmd_buffer.blit_image(
                &image_lock,
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
                        x: image.get_size() as i32,
                        y: image.get_size() as i32,
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
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TRANSFER..hal::pso::PipelineStage::COMPUTE_SHADER,
                hal::memory::Dependencies::empty(),
                &[
                    image.barrier_to(
                        &image_lock,
                        hal::image::Access::SHADER_READ,
                        hal::image::Layout::ShaderReadOnlyOptimal,
                    ),
                    hal::memory::Barrier::Image {
                        states: (
                            hal::image::Access::TRANSFER_WRITE,
                            hal::image::Layout::TransferDstOptimal,
                        )
                            ..(
                                hal::image::Access::SHADER_READ,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            ),
                        target: thumbnail_image,
                        families: None,
                        range: super::COLOR_RANGE.clone(),
                    },
                ],
            );
            cmd_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&cmd_buffer), Some(&self.fence));
            lock.device.wait_for_fence(&self.fence, !0).unwrap();
            self.command_pool.free(Some(cmd_buffer));
        }
    }

    /// Get a new thumbnail from the cache
    pub fn new_thumbnail(&mut self, ty: lang::ImageType) -> ThumbnailIndex {
        match ty {
            lang::ImageType::Grayscale => self.thumbnail_cache.next(true),
            lang::ImageType::Rgb => self.thumbnail_cache.next(false),
        }
    }

    /// Return a thumbnail the cache. Thumbnail to thumbnail, dust to dust.
    pub fn return_thumbnail(&mut self, thumbnail: ThumbnailIndex) {
        self.thumbnail_cache.free(thumbnail);
    }

    pub fn view_thumbnail(&self, thumbnail: &ThumbnailIndex) -> &Arc<Mutex<B::ImageView>> {
        self.thumbnail_cache.image_view(thumbnail)
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

/// A compute pipeline that can be used with GPUCompute. Typically describes one
/// compute shader execution for one atomic operator.
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
