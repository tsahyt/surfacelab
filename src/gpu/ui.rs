use super::{basic_mem::*, load_shader, RenderTarget, GPU};

use gfx_hal as hal;
use hal::{
    buffer, command, format as f, image as i, memory as m, pass,
    pass::Subpass,
    pool,
    prelude::*,
    pso,
    pso::{PipelineStage, ShaderStageFlags, VertexInputRate},
    queue::Submission,
    window, Backend,
};
use std::sync::{Arc, Mutex};

use std::{
    borrow::Borrow,
    collections::HashMap,
    iter,
    mem::{self, ManuallyDrop},
    sync::Weak,
};

use thiserror::Error;

use conrod_core::{self, mesh::*};

/// Format of the render target
const TARGET_FORMAT: f::Format = f::Format::Bgra8Srgb;

/// Number of MSAA samples to use for UI drawing. This does not affect blitted textures!
const MSAA_SAMPLES: i::NumSamples = 4;

/// Entry point for UI shaders
const ENTRY_NAME: &str = "main";

/// Format for the glyph cache, used for text and icons
const GLYPH_CACHE_FORMAT: hal::format::Format = hal::format::Format::R8Unorm;

const COLOR_RANGE: i::SubresourceRange = i::SubresourceRange {
    aspects: f::Aspects::COLOR,
    levels: 0..1,
    layers: 0..1,
};

/// UI Vertex definition, representationally matching what comes out of conrod
/// such that we can simply reinterpret the memory region instead of spending
/// time on conversion.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Vertex {
    /// The normalised position of the vertex within vector space.
    ///
    /// [-1.0, 1.0] is the leftmost, bottom position of the display.
    /// [1.0, -1.0] is the rightmost, top position of the display.
    pub position: [f32; 2],
    /// The coordinates of the texture used by this `Vertex`.
    ///
    /// [0.0, 0.0] is the leftmost, top position of the texture.
    /// [1.0, 1.0] is the rightmost, bottom position of the texture.
    pub tex_coords: [f32; 2],
    /// Linear sRGB with an alpha channel.
    pub rgba: [f32; 4],
    /// The mode with which the `Vertex` will be drawn within the fragment shader.
    ///
    /// `0` for rendering text.
    /// `1` for rendering an image.
    /// `2` for rendering non-textured 2D geometry.
    ///
    /// If any other value is given, the fragment shader will not output any color.
    pub mode: u32,
}

/// A loaded gfx-hal texture and it's width/height, as well as format. Any
/// validation as well as ensuring the image is backed by memory is to be done
/// by the user!
///
/// Also note that cleanup has to be performed by the user before the image is
/// dropped. In particular images should be handled through the API provided by
/// the UI renderer.
#[derive(Debug)]
pub struct Image<B: Backend> {
    descriptor: B::DescriptorSet,
    /// The width of the image.
    width: u32,
    /// The height of the image.
    height: u32,
    /// A weak reference to the view, such that we can check whether it is alive
    /// and lock it for rendering, to ensure it cannot be freed while we're using it.
    view: Weak<Mutex<B::ImageView>>,
}

impl<B> ImageDimensions for Image<B>
where
    B: Backend,
{
    fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }
}

/// Dynamically resizing vertex buffer for UI.
pub struct VertexBuffer<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    capacity: usize,
    memory: ManuallyDrop<B::Memory>,
    buffer: ManuallyDrop<B::Buffer>,
}

#[derive(Debug, Error)]
pub enum VertexBufferError {
    #[error("Error during buffer building")]
    BufferCreation(#[from] BasicBufferBuilderError),
    #[error("Error during memory mapping")]
    Map(#[from] hal::device::MapError),
    #[error("Out of memory during write")]
    OutOfMemory(#[from] hal::device::OutOfMemory),
}

impl<B> VertexBuffer<B>
where
    B: Backend,
{
    /// Create a new vertex buffer with default size (16384 vertices).
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, VertexBufferError> {
        Self::with_capacity(gpu, 16384)
    }

    /// Build a new vertex buffer with a given capacity.
    pub fn with_capacity(
        gpu: Arc<Mutex<GPU<B>>>,
        capacity: usize,
    ) -> Result<Self, VertexBufferError> {
        let bytes = (std::mem::size_of::<Vertex>() * capacity) as u64;

        let lock = gpu.lock().unwrap();

        let mut buffer_builder = BasicBufferBuilder::new(&lock.memory_properties.memory_types);
        buffer_builder
            .bytes(bytes)
            .usage(hal::buffer::Usage::VERTEX);

        // Pick memory type for buffer builder for AMD/Nvidia
        if let None = buffer_builder.memory_type(
            hal::memory::Properties::CPU_VISIBLE | hal::memory::Properties::DEVICE_LOCAL,
        ) {
            buffer_builder
                .memory_type(
                    hal::memory::Properties::CPU_VISIBLE | hal::memory::Properties::COHERENT,
                )
                .expect("Failed to find appropriate memory type for UI vertex buffer");
        }

        let (buffer, memory) = buffer_builder.build::<B>(&lock.device)?;

        drop(lock);

        Ok(Self {
            gpu,
            capacity,
            memory: ManuallyDrop::new(memory),
            buffer: ManuallyDrop::new(buffer),
        })
    }

    /// Resizes the buffers. This will destroy old data!
    fn resize(&mut self, new_capacity: usize) -> Result<(), VertexBufferError> {
        let bytes = (std::mem::size_of::<Vertex>() * new_capacity) as u64;

        let lock = self.gpu.lock().unwrap();

        let mut buffer_builder = BasicBufferBuilder::new(&lock.memory_properties.memory_types);
        buffer_builder
            .bytes(bytes)
            .usage(hal::buffer::Usage::VERTEX);

        // Pick memory type for buffer builder for AMD/Nvidia
        if let None = buffer_builder.memory_type(
            hal::memory::Properties::CPU_VISIBLE | hal::memory::Properties::DEVICE_LOCAL,
        ) {
            buffer_builder
                .memory_type(
                    hal::memory::Properties::CPU_VISIBLE | hal::memory::Properties::COHERENT,
                )
                .expect("Failed to find appropriate memory type for UI vertex buffer");
        }

        let (buffer, memory) = buffer_builder.build::<B>(&lock.device)?;

        {
            unsafe {
                lock.device
                    .destroy_buffer(ManuallyDrop::take(&mut self.buffer));
                lock.device
                    .free_memory(ManuallyDrop::take(&mut self.memory));
            }
        }

        self.buffer = ManuallyDrop::new(buffer);
        self.memory = ManuallyDrop::new(memory);
        self.capacity = new_capacity;

        Ok(())
    }

    /// Copy the data from the given slice into this vertex buffer.
    pub fn copy_from_slice(&mut self, vertices: &[Vertex]) -> Result<(), VertexBufferError> {
        if vertices.len() > self.capacity {
            self.resize(vertices.len().max(self.capacity * 2))?;
        }

        if vertices.is_empty() {
            return Ok(());
        }

        let lock = self.gpu.lock().unwrap();
        let bytes = std::mem::size_of::<Vertex>() * vertices.len();

        let mapping = unsafe {
            lock.device
                .map_memory(&*self.memory, hal::memory::Segment::ALL)
        }?;
        unsafe {
            std::ptr::copy_nonoverlapping(vertices.as_ptr() as *const u8, mapping, bytes);
            lock.device.flush_mapped_memory_ranges(std::iter::once((
                &*self.memory,
                hal::memory::Segment::ALL,
            )))?;
            lock.device.unmap_memory(&*self.memory);
        };

        Ok(())
    }

    /// Obtain a reference to the underlying buffer for mapping in a pipeline.
    pub fn buffer(&self) -> &B::Buffer {
        &*self.buffer
    }
}

/// Conversion function for vertex buffer data. This is zero copy.
fn conv_vertex_buffer(buffer: &[conrod_core::mesh::Vertex]) -> &[Vertex] {
    unsafe { &*(buffer as *const [conrod_core::mesh::Vertex] as *const [Vertex]) }
}

impl<B> Drop for VertexBuffer<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.memory));
            lock.device
                .destroy_buffer(ManuallyDrop::take(&mut self.buffer));
        }
    }
}

#[derive(Debug, Error)]
pub enum GlyphCacheError {
    #[error("Failed to build glyph cache image")]
    ImageBuilderError(#[from] BasicImageBuilderError),
    #[error("Failed to build buffer for upload")]
    UploadBufferError(#[from] BasicBufferBuilderError),
}

/// The glyph cache for the UI renderer. Holds font and icon data. Is prepared
/// by conrod through rusttype.
///
/// Designed to not be updated particularly often, since staging memory is
/// recreated as needed. Resides on device. The size is fixed at creation time.
pub struct GlyphCache<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    image: ManuallyDrop<B::Image>,
    view: ManuallyDrop<B::ImageView>,
    memory: ManuallyDrop<B::Memory>,
    cache_size: u64,
    cache_dims: [u32; 2],
}

impl<B> GlyphCache<B>
where
    B: Backend,
{
    /// Create a new glyph cache with the given dimensions.
    pub fn new(gpu: Arc<Mutex<GPU<B>>>, size: [u32; 2]) -> Result<Self, GlyphCacheError> {
        let lock = gpu.lock().unwrap();

        let [width, height] = size;

        let (image, memory, view) = BasicImageBuilder::new(&lock.memory_properties.memory_types)
            .size_2d(width, height)
            .format(GLYPH_CACHE_FORMAT)
            .usage(i::Usage::TRANSFER_DST | i::Usage::SAMPLED)
            .memory_type(m::Properties::DEVICE_LOCAL)
            .unwrap()
            .build::<B>(&lock.device)?;

        let cache_size = unsafe { lock.device.get_image_requirements(&image) }.size;

        Ok(GlyphCache {
            gpu: gpu.clone(),
            image: ManuallyDrop::new(image),
            memory: ManuallyDrop::new(memory),
            view: ManuallyDrop::new(view),
            cache_size,
            cache_dims: size,
        })
    }

    /// Upload new glyph cache data. Expected to be laid out by rusttype.
    ///
    /// The command pool must reside on the same device as the cache.
    pub fn upload(
        &mut self,
        command_pool: &mut B::CommandPool,
        cpu_cache: &[u8],
    ) -> Result<(), GlyphCacheError> {
        let mut lock = self.gpu.lock().unwrap();

        let (image_upload_buffer, image_upload_memory) =
            BasicBufferBuilder::new(&lock.memory_properties.memory_types)
                .bytes(self.cache_size)
                .data(cpu_cache)
                .usage(buffer::Usage::TRANSFER_SRC)
                .memory_type(m::Properties::CPU_VISIBLE)
                .unwrap()
                .build::<B>(&lock.device)?;

        // copy buffer to texture
        let copy_fence = lock
            .device
            .create_fence(false)
            .expect("Could not create fence");
        unsafe {
            let mut cmd_buffer = command_pool.allocate_one(command::Level::Primary);
            cmd_buffer.begin_primary(command::CommandBufferFlags::ONE_TIME_SUBMIT);

            let image_barrier = m::Barrier::Image {
                states: (i::Access::empty(), i::Layout::Undefined)
                    ..(i::Access::TRANSFER_WRITE, i::Layout::TransferDstOptimal),
                target: &*self.image,
                families: None,
                range: COLOR_RANGE.clone(),
            };

            cmd_buffer.pipeline_barrier(
                PipelineStage::TOP_OF_PIPE..PipelineStage::TRANSFER,
                m::Dependencies::empty(),
                &[image_barrier],
            );

            cmd_buffer.copy_buffer_to_image(
                &image_upload_buffer,
                &*self.image,
                i::Layout::TransferDstOptimal,
                &[command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: self.cache_dims[0],
                    buffer_height: self.cache_dims[1],
                    image_layers: i::SubresourceLayers {
                        aspects: f::Aspects::COLOR,
                        level: 0,
                        layers: 0..1,
                    },
                    image_offset: i::Offset { x: 0, y: 0, z: 0 },
                    image_extent: i::Extent {
                        width: self.cache_dims[0],
                        height: self.cache_dims[1],
                        depth: 1,
                    },
                }],
            );

            let image_barrier = m::Barrier::Image {
                states: (i::Access::TRANSFER_WRITE, i::Layout::TransferDstOptimal)
                    ..(i::Access::SHADER_READ, i::Layout::ShaderReadOnlyOptimal),
                target: &*self.image,
                families: None,
                range: COLOR_RANGE.clone(),
            };
            cmd_buffer.pipeline_barrier(
                PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
                m::Dependencies::empty(),
                &[image_barrier],
            );

            cmd_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&cmd_buffer), Some(&copy_fence));

            lock.device
                .wait_for_fence(&copy_fence, !0)
                .expect("Can't wait for fence");
        }

        unsafe {
            lock.device.free_memory(image_upload_memory);
            lock.device.destroy_fence(copy_fence);
        }

        Ok(())
    }

    /// Obtain image view of the glyph cache for use in a pipeline
    pub fn view(&self) -> &B::ImageView {
        &*self.view
    }
}

impl<B> Drop for GlyphCache<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.image));
            lock.device
                .destroy_image_view(ManuallyDrop::take(&mut self.view));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.memory));
        }
    }
}

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("Error in vertex buffer operation")]
    VertexBuffer(#[from] VertexBufferError),
    #[error("Error in glyph cache operation")]
    GlyphCache(#[from] GlyphCacheError),
    #[error("Failed window initialization")]
    WindowInit(#[from] hal::window::InitError),
    #[error("Failed window initialization")]
    WindowCreation(#[from] hal::window::CreationError),
    #[error("Surface does not support queue family")]
    SurfaceQueueMismatch,
    #[error("Out of memory condition encountered")]
    OutOfMemory(#[from] hal::device::OutOfMemory),
    #[error("Driver allocation error encountered")]
    AllocationError(#[from] hal::device::AllocationError),
    #[error("Shader initialization failed")]
    ShaderInit,
    #[error("Render target (re)initialization failed")]
    TargetInit,
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("Error writing glyph cache")]
    CacheWriteError(#[from] conrod_core::text::rt::gpu_cache::CacheWriteErr),
    #[error("Error uploading glyph cache")]
    GlyphCache(#[from] GlyphCacheError),
    #[error("Synchronization primitives timed out")]
    SyncTimeout,
}

/// UI Renderer
pub struct Renderer<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,

    surface: ManuallyDrop<B::Surface>,

    dimensions: window::Extent2D,
    viewport: pso::Viewport,
    render_pass: ManuallyDrop<B::RenderPass>,
    pipeline: ManuallyDrop<B::GraphicsPipeline>,
    pipeline_layout: ManuallyDrop<B::PipelineLayout>,

    // Basic Descriptors
    basic_desc_pool: ManuallyDrop<B::DescriptorPool>,
    basic_desc_set: B::DescriptorSet,
    basic_set_layout: ManuallyDrop<B::DescriptorSetLayout>,

    // Per Image Descriptors
    image_desc_pool: ManuallyDrop<B::DescriptorPool>,
    image_set_layout: ManuallyDrop<B::DescriptorSetLayout>,
    image_desc_default: B::DescriptorSet,

    submission_complete_semaphore: ManuallyDrop<B::Semaphore>,
    submission_complete_fence: ManuallyDrop<B::Fence>,
    command_pool: ManuallyDrop<B::CommandPool>,
    command_buffer: ManuallyDrop<B::CommandBuffer>,

    vertex_buffer: VertexBuffer<B>,
    glyph_cache: GlyphCache<B>,

    render_target: RenderTarget<B>,

    sampler: ManuallyDrop<B::Sampler>,
    mesh: conrod_core::mesh::Mesh,
}

impl<B> Renderer<B>
where
    B: Backend,
{
    pub fn new<W: raw_window_handle::HasRawWindowHandle>(
        gpu: Arc<Mutex<GPU<B>>>,
        window: &W,
        dimensions: window::Extent2D,
        glyph_cache_dims: [u32; 2],
    ) -> Result<Renderer<B>, RendererError> {
        let vertex_buffer = VertexBuffer::new(gpu.clone())?;
        let glyph_cache = GlyphCache::new(gpu.clone(), glyph_cache_dims)?;
        let render_target = RenderTarget::new(
            gpu.clone(),
            TARGET_FORMAT,
            MSAA_SAMPLES,
            false,
            (dimensions.width, dimensions.height),
        )
        .map_err(|_| RendererError::TargetInit)?;

        // Lock after other structures have been created using the GPU already
        // to prevent deadlock.
        let lock = gpu.lock().unwrap();

        let mut surface = unsafe { lock.instance.create_surface(window) }?;

        if !surface.supports_queue_family(&lock.adapter.queue_families[lock.queue_group.family.0]) {
            return Err(RendererError::SurfaceQueueMismatch);
        }

        let mut command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                pool::CommandPoolCreateFlags::empty(),
            )
        }?;

        let sampler = unsafe {
            lock.device
                .create_sampler(&i::SamplerDesc::new(i::Filter::Linear, i::WrapMode::Clamp))
        }?;

        // Setup renderpass and pipeline
        let basic_set_layout = unsafe {
            lock.device.create_descriptor_set_layout(
                &[
                    // Glyph Cache Image
                    pso::DescriptorSetLayoutBinding {
                        binding: 0,
                        ty: pso::DescriptorType::Image {
                            ty: pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    // Sampler for glyphs and images
                    pso::DescriptorSetLayoutBinding {
                        binding: 1,
                        ty: pso::DescriptorType::Sampler,
                        count: 1,
                        stage_flags: ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                ],
                &[],
            )
        }?;

        // Descriptor set layout for images
        let image_set_layout = unsafe {
            lock.device.create_descriptor_set_layout(
                &[pso::DescriptorSetLayoutBinding {
                    binding: 0,
                    ty: pso::DescriptorType::Image {
                        ty: pso::ImageDescriptorType::Sampled {
                            with_sampler: false,
                        },
                    },
                    count: 1,
                    stage_flags: ShaderStageFlags::FRAGMENT,
                    immutable_samplers: false,
                }],
                &[],
            )
        }?;

        // Descriptors
        let mut basic_desc_pool = unsafe {
            lock.device.create_descriptor_pool(
                1, // sets
                &[
                    pso::DescriptorRangeDesc {
                        ty: pso::DescriptorType::Image {
                            ty: pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                    },
                    pso::DescriptorRangeDesc {
                        ty: pso::DescriptorType::Sampler,
                        count: 1,
                    },
                ],
                pso::DescriptorPoolCreateFlags::empty(),
            )
        }?;

        let mut image_desc_pool = unsafe {
            lock.device.create_descriptor_pool(
                4096, // sets
                &[pso::DescriptorRangeDesc {
                    ty: pso::DescriptorType::Image {
                        ty: pso::ImageDescriptorType::Sampled {
                            with_sampler: false,
                        },
                    },
                    count: 4096,
                }],
                pso::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET,
            )
        }?;

        let basic_desc_set = unsafe { basic_desc_pool.allocate_set(&basic_set_layout) }.unwrap();

        // Write basic descriptor set. These do not change ever.
        unsafe {
            lock.device.write_descriptor_sets(vec![
                pso::DescriptorSetWrite {
                    set: &basic_desc_set,
                    binding: 0,
                    array_offset: 0,
                    descriptors: Some(pso::Descriptor::Image(
                        glyph_cache.view(),
                        i::Layout::ShaderReadOnlyOptimal,
                    )),
                },
                pso::DescriptorSetWrite {
                    set: &basic_desc_set,
                    binding: 1,
                    array_offset: 0,
                    descriptors: Some(pso::Descriptor::Sampler(&sampler)),
                },
            ]);
        }

        let image_desc_default =
            unsafe { image_desc_pool.allocate_set(&image_set_layout) }.unwrap();

        // Default descriptor set for images. If there are no images, we still
        // need to provide *something*, so we just provide the glyph cache.
        unsafe {
            lock.device
                .write_descriptor_sets(vec![pso::DescriptorSetWrite {
                    set: &image_desc_default,
                    binding: 0,
                    array_offset: 0,
                    descriptors: Some(pso::Descriptor::Image(
                        glyph_cache.view(),
                        i::Layout::ShaderReadOnlyOptimal,
                    )),
                }]);
        }

        // Swapchain configuration
        let caps = surface.capabilities(&lock.adapter.physical_device);

        let swap_config = window::SwapchainConfig::from_caps(&caps, TARGET_FORMAT, dimensions);
        let extent = swap_config.extent;
        unsafe { surface.configure_swapchain(&lock.device, swap_config)? };

        // Define render pass
        let render_pass = {
            let color_attachment = pass::Attachment {
                format: Some(TARGET_FORMAT),
                samples: render_target.samples(),
                ops: pass::AttachmentOps::new(
                    pass::AttachmentLoadOp::DontCare,
                    pass::AttachmentStoreOp::Store,
                ),
                stencil_ops: pass::AttachmentOps::DONT_CARE,
                layouts: i::Layout::Undefined..i::Layout::ColorAttachmentOptimal,
            };

            let present_attachment = pass::Attachment {
                format: Some(TARGET_FORMAT),
                samples: 1,
                ops: pass::AttachmentOps::new(
                    pass::AttachmentLoadOp::DontCare,
                    pass::AttachmentStoreOp::Store,
                ),
                stencil_ops: pass::AttachmentOps::DONT_CARE,
                layouts: i::Layout::Undefined..i::Layout::Present,
            };

            let subpass = pass::SubpassDesc {
                colors: &[(0, i::Layout::ColorAttachmentOptimal)],
                depth_stencil: None,
                inputs: &[],
                resolves: &[(1, i::Layout::Present)],
                preserves: &[],
            };

            unsafe {
                lock.device.create_render_pass(
                    &[color_attachment, present_attachment],
                    &[subpass],
                    &[],
                )
            }?
        };

        // Sync primitives
        let submission_complete_semaphore = lock.device.create_semaphore()?;
        let submission_complete_fence = lock.device.create_fence(true)?;

        let command_buffer = unsafe { command_pool.allocate_one(command::Level::Primary) };

        let pipeline_layout = unsafe {
            lock.device
                .create_pipeline_layout(vec![&basic_set_layout, &image_set_layout], &[])
        }?;
        let pipeline = {
            let vs_module = {
                load_shader::<B>(
                    &lock.device,
                    &include_bytes!("../../shaders/ui-vert.spv")[..],
                )
                .map_err(|_| RendererError::ShaderInit)?
            };
            let fs_module = {
                load_shader::<B>(
                    &lock.device,
                    &include_bytes!("../../shaders/ui-frag.spv")[..],
                )
                .map_err(|_| RendererError::ShaderInit)?
            };

            let pipeline = {
                let (vs_entry, fs_entry) = (
                    pso::EntryPoint {
                        entry: ENTRY_NAME,
                        module: &vs_module,
                        specialization: pso::Specialization::default(),
                    },
                    pso::EntryPoint {
                        entry: ENTRY_NAME,
                        module: &fs_module,
                        specialization: pso::Specialization::default(),
                    },
                );

                let shader_entries = pso::GraphicsShaderSet {
                    vertex: vs_entry,
                    hull: None,
                    domain: None,
                    geometry: None,
                    fragment: Some(fs_entry),
                };

                let subpass = Subpass {
                    index: 0,
                    main_pass: &render_pass,
                };

                let mut pipeline_desc = pso::GraphicsPipelineDesc::new(
                    shader_entries,
                    pso::Primitive::TriangleList,
                    pso::Rasterizer::FILL,
                    &pipeline_layout,
                    subpass,
                );
                pipeline_desc.multisampling = Some(pso::Multisampling {
                    rasterization_samples: render_target.samples(),
                    sample_shading: None,
                    sample_mask: !0,
                    alpha_coverage: false,
                    alpha_to_one: false,
                });
                pipeline_desc.blender.targets.push(pso::ColorBlendDesc {
                    mask: pso::ColorMask::ALL,
                    blend: Some(pso::BlendState::ALPHA),
                });
                pipeline_desc.vertex_buffers.push(pso::VertexBufferDesc {
                    binding: 0,
                    stride: mem::size_of::<Vertex>() as u32,
                    rate: VertexInputRate::Vertex,
                });

                // Vertex Attributes
                pipeline_desc.attributes.push(pso::AttributeDesc {
                    location: 0,
                    binding: 0,
                    element: pso::Element {
                        format: f::Format::Rg32Sfloat,
                        offset: 0,
                    },
                });
                pipeline_desc.attributes.push(pso::AttributeDesc {
                    location: 1,
                    binding: 0,
                    element: pso::Element {
                        format: f::Format::Rg32Sfloat,
                        offset: 8,
                    },
                });
                pipeline_desc.attributes.push(pso::AttributeDesc {
                    location: 2,
                    binding: 0,
                    element: pso::Element {
                        format: f::Format::Rgba32Sfloat,
                        offset: 16,
                    },
                });
                pipeline_desc.attributes.push(pso::AttributeDesc {
                    location: 3,
                    binding: 0,
                    element: pso::Element {
                        format: f::Format::R32Uint,
                        offset: 32,
                    },
                });

                unsafe { lock.device.create_graphics_pipeline(&pipeline_desc, None) }
            };

            unsafe {
                lock.device.destroy_shader_module(vs_module);
            }
            unsafe {
                lock.device.destroy_shader_module(fs_module);
            }

            pipeline.unwrap()
        };

        // Rendering setup
        let viewport = pso::Viewport {
            rect: pso::Rect {
                x: 0,
                y: 0,
                w: extent.width as _,
                h: extent.height as _,
            },
            depth: 0.0..1.0,
        };

        let mesh = Mesh::with_glyph_cache_dimensions(glyph_cache_dims);

        Ok(Renderer {
            gpu: gpu.clone(),
            surface: ManuallyDrop::new(surface),
            dimensions,
            viewport,
            render_pass: ManuallyDrop::new(render_pass),
            pipeline: ManuallyDrop::new(pipeline),
            pipeline_layout: ManuallyDrop::new(pipeline_layout),
            basic_desc_pool: ManuallyDrop::new(basic_desc_pool),
            basic_set_layout: ManuallyDrop::new(basic_set_layout),
            basic_desc_set,
            image_desc_pool: ManuallyDrop::new(image_desc_pool),
            image_set_layout: ManuallyDrop::new(image_set_layout),
            image_desc_default,
            submission_complete_semaphore: ManuallyDrop::new(submission_complete_semaphore),
            submission_complete_fence: ManuallyDrop::new(submission_complete_fence),
            command_pool: ManuallyDrop::new(command_pool),
            command_buffer: ManuallyDrop::new(command_buffer),
            vertex_buffer,
            glyph_cache,
            render_target,
            sampler: ManuallyDrop::new(sampler),
            mesh,
        })
    }

    /// Recreate the swapchain with new dimensions.
    ///
    /// Note that the swapchain will *always* be recreated, with the same
    /// dimensions if no new dimensions are given.
    pub fn recreate_swapchain(
        &mut self,
        new_dimensions: Option<window::Extent2D>,
    ) -> Result<(), RendererError> {
        if let Some(ext) = new_dimensions {
            self.dimensions = ext;
        }

        self.render_target = RenderTarget::new(
            self.gpu.clone(),
            self.render_target.format(),
            self.render_target.samples(),
            false,
            (self.dimensions.width, self.dimensions.height),
        )
        .map_err(|_| RendererError::TargetInit)?;

        let lock = self.gpu.lock().unwrap();

        let caps = self.surface.capabilities(&lock.adapter.physical_device);
        let swap_config = window::SwapchainConfig::from_caps(&caps, TARGET_FORMAT, self.dimensions);
        let extent = swap_config.extent.to_extent();

        unsafe {
            self.surface
                .configure_swapchain(&lock.device, swap_config)?;
        }

        self.viewport.rect.w = extent.width as _;
        self.viewport.rect.h = extent.height as _;

        Ok(())
    }

    /// Render the UI given an image map and a collection of conrod primitives
    /// to render.
    pub fn render<P: conrod_core::render::PrimitiveWalker>(
        &mut self,
        image_map: &conrod_core::image::Map<Image<B>>,
        primitives: P,
    ) -> Result<(), RenderError> {
        self.fill(
            image_map,
            [
                self.viewport.rect.x as f32,
                self.viewport.rect.y as f32,
                self.viewport.rect.w as f32,
                self.viewport.rect.h as f32,
            ],
            1.0,
            primitives,
        )?;

        let surface_image = unsafe {
            match self.surface.acquire_image(!0) {
                Ok((image, _)) => image,
                Err(_) => {
                    self.recreate_swapchain(None);
                    return Ok(());
                }
            }
        };

        let framebuffer = {
            let lock = self.gpu.lock().unwrap();
            let target_lock = self.render_target.image_view().lock().unwrap();

            // Create framebuffer for frame. This is in accordance with the
            // recommendations from the gfx-rs team
            let framebuffer = unsafe {
                lock.device
                    .create_framebuffer(
                        &self.render_pass,
                        vec![&target_lock as &B::ImageView, surface_image.borrow()],
                        i::Extent {
                            width: self.dimensions.width,
                            height: self.dimensions.height,
                            depth: 1,
                        },
                    )
                    .unwrap()
            };

            // Wait for the fence of the previous submission and reset it
            unsafe {
                lock.device
                    .wait_for_fence(&self.submission_complete_fence, !0)
                    .map_err(|_| RenderError::SyncTimeout)?;
                lock.device
                    .reset_fence(&self.submission_complete_fence)
                    .map_err(|_| RenderError::SyncTimeout)?;
                self.command_pool.reset(false);
            }

            framebuffer
        };

        // Rendering
        let mut image_locks = HashMap::new();
        let cmd_buffer = &mut *self.command_buffer;

        // Fill vertex buffer
        self.vertex_buffer
            .copy_from_slice(conv_vertex_buffer(self.mesh.vertices()))
            .unwrap();

        unsafe {
            cmd_buffer.begin_primary(command::CommandBufferFlags::ONE_TIME_SUBMIT);

            cmd_buffer.set_viewports(0, &[self.viewport.clone()]);
            cmd_buffer.set_scissors(0, &[self.viewport.rect]);

            cmd_buffer.bind_graphics_pipeline(&self.pipeline);
            cmd_buffer.bind_vertex_buffers(
                0,
                iter::once((self.vertex_buffer.buffer(), buffer::SubRange::WHOLE)),
            );
            cmd_buffer.bind_graphics_descriptor_sets(
                &self.pipeline_layout,
                0,
                vec![&self.basic_desc_set, &self.image_desc_default],
                &[],
            );

            cmd_buffer.begin_render_pass(
                &self.render_pass,
                &framebuffer,
                self.viewport.rect,
                &[],
                command::SubpassContents::Inline,
            );
        }

        for cmd in self.mesh.commands() {
            match cmd {
                Command::Scizzor(scizzor) => unsafe {
                    cmd_buffer.set_scissors(
                        0,
                        &[hal::pso::Rect {
                            x: scizzor.top_left[0] as i16,
                            y: scizzor.top_left[1] as i16,
                            w: scizzor.dimensions[0] as i16,
                            h: scizzor.dimensions[1] as i16,
                        }],
                    );
                },
                Command::Draw(draw) => match draw {
                    Draw::Plain(range) => unsafe {
                        if !ExactSizeIterator::is_empty(&range) {
                            cmd_buffer.draw(range.start as u32..range.end as u32, 0..1);
                        }
                    },
                    Draw::Image(img_id, range) => unsafe {
                        if !ExactSizeIterator::is_empty(&range) {
                            if let Some(image) = image_map.get(&img_id) {
                                // It is enough to hold a strong reference to prevent a drop
                                image_locks
                                    .entry(img_id)
                                    .or_insert_with(|| image.view.upgrade());
                                cmd_buffer.bind_graphics_descriptor_sets(
                                    &self.pipeline_layout,
                                    0,
                                    vec![&self.basic_desc_set, &image.descriptor],
                                    &[],
                                );
                                cmd_buffer.draw(range.start as u32..range.end as u32, 0..1);
                            }
                        }
                    },
                },
            }
        }
        unsafe {
            cmd_buffer.end_render_pass();
            cmd_buffer.finish();

            let submission = Submission {
                command_buffers: iter::once(&*cmd_buffer),
                wait_semaphores: None,
                signal_semaphores: iter::once(&*self.submission_complete_semaphore),
            };

            let result = {
                let mut lock = self.gpu.lock().unwrap();
                lock.queue_group.queues[0]
                    .submit(submission, Some(&*self.submission_complete_fence));

                // present frame
                let result = lock.queue_group.queues[0].present_surface(
                    &mut self.surface,
                    surface_image,
                    Some(&self.submission_complete_semaphore),
                );

                lock.device.destroy_framebuffer(framebuffer);

                result
            };

            if result.is_err() {
                self.recreate_swapchain(None);
            }
        }

        Ok(())
    }

    /// Fill the internal mesh from the primitives
    pub fn fill<'a, P: conrod_core::render::PrimitiveWalker>(
        &'a mut self,
        image_map: &conrod_core::image::Map<Image<B>>,
        viewport: [f32; 4],
        dpi_factor: f64,
        primitives: P,
    ) -> Result<(), RenderError> {
        let [vp_l, vp_t, vp_r, vp_b] = viewport;
        let lt = [vp_l as conrod_core::Scalar, vp_t as conrod_core::Scalar];
        let rb = [vp_r as conrod_core::Scalar, vp_b as conrod_core::Scalar];
        let viewport = conrod_core::Rect::from_corners(lt, rb);
        let fill = self
            .mesh
            .fill(viewport, dpi_factor, image_map, primitives)?;
        if fill.glyph_cache_requires_upload {
            self.glyph_cache.upload(
                &mut *self.command_pool,
                self.mesh.glyph_cache_pixel_buffer(),
            )?;
        }
        Ok(())
    }

    /// Create an image for use in the image map for rendering
    pub fn create_image(
        &mut self,
        view: Weak<Mutex<B::ImageView>>,
        width: u32,
        height: u32,
    ) -> Option<Image<B>> {
        let strong_ref = view.upgrade()?;
        let image_view = strong_ref.lock().unwrap();

        let lock = self.gpu.lock().unwrap();

        let desc = unsafe { self.image_desc_pool.allocate_set(&*self.image_set_layout) }.unwrap();

        unsafe {
            lock.device
                .write_descriptor_sets(vec![pso::DescriptorSetWrite {
                    set: &desc,
                    binding: 0,
                    array_offset: 0,
                    descriptors: Some(pso::Descriptor::Image(
                        &image_view as &B::ImageView,
                        i::Layout::ShaderReadOnlyOptimal,
                    )),
                }]);
        }

        Some(Image {
            descriptor: desc,
            width,
            height,
            view,
        })
    }

    /// Destroy an image, freeing up the descriptor set resources. This does
    /// *NOT* destroy the image view, image, or memory that was backing this
    /// image!
    pub fn destroy_image(&mut self, image: Image<B>) {
        unsafe {
            self.image_desc_pool.free_sets(iter::once(image.descriptor));
        }
    }
}

impl<B> Drop for Renderer<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        let lock = self.gpu.lock().unwrap();
        let device = &lock.device;

        device.wait_idle().unwrap();
        unsafe {
            device.destroy_descriptor_pool(ManuallyDrop::take(&mut self.basic_desc_pool));
            device.destroy_descriptor_set_layout(ManuallyDrop::take(&mut self.basic_set_layout));
            device.destroy_descriptor_pool(ManuallyDrop::take(&mut self.image_desc_pool));
            device.destroy_descriptor_set_layout(ManuallyDrop::take(&mut self.image_set_layout));
            device.destroy_sampler(ManuallyDrop::take(&mut self.sampler));
            device.destroy_command_pool(ManuallyDrop::take(&mut self.command_pool));
            device.destroy_semaphore(ManuallyDrop::take(&mut self.submission_complete_semaphore));
            device.destroy_fence(ManuallyDrop::take(&mut self.submission_complete_fence));
            device.destroy_render_pass(ManuallyDrop::take(&mut self.render_pass));
            self.surface.unconfigure_swapchain(&device);
            device.destroy_graphics_pipeline(ManuallyDrop::take(&mut self.pipeline));
            device.destroy_pipeline_layout(ManuallyDrop::take(&mut self.pipeline_layout));
            let surface = ManuallyDrop::take(&mut self.surface);
            lock.instance.destroy_surface(surface);
        }
    }
}
