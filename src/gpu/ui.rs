use super::GPU;

use gfx_hal as hal;
use hal::{
    buffer, command, format as f,
    format::{ChannelType, Swizzle},
    image as i, memory as m, pass,
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
    io::Cursor,
    iter,
    mem::{self, ManuallyDrop},
    ptr,
};

use conrod_core::{self, mesh::*};

const ENTRY_NAME: &str = "main";
const GLYPH_CACHE_FORMAT: hal::format::Format = hal::format::Format::R8Unorm;
const COLOR_RANGE: i::SubresourceRange = i::SubresourceRange {
    aspects: f::Aspects::COLOR,
    levels: 0..1,
    layers: 0..1,
};

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
}

impl<B> ImageDimensions for Image<B>
where
    B: Backend,
{
    fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }
}

pub struct VertexBuffer<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    capacity: usize,
    staging_mem: ManuallyDrop<B::Memory>,
    staging_buf: ManuallyDrop<B::Buffer>,
    device_mem: ManuallyDrop<B::Memory>,
    device_buf: ManuallyDrop<B::Buffer>,
}

#[derive(Debug)]
pub enum VertexBufferError {
    BufferCreation(hal::buffer::CreationError),
    MemoryAllocation(hal::device::AllocationError),
    Bind(hal::device::BindError),
    Map(hal::device::MapError),
    OutOfMemory(hal::device::OutOfMemory),
}

impl<B> VertexBuffer<B>
where
    B: Backend,
{
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, VertexBufferError> {
        Self::with_capacity(gpu, 16384)
    }

    fn build_buffer(
        gpu: Arc<Mutex<GPU<B>>>,
        bytes: u64,
        properties: hal::memory::Properties,
        usage: hal::buffer::Usage,
    ) -> Result<(B::Buffer, B::Memory), VertexBufferError> {
        let lock = gpu.lock().unwrap();

        let mut buf = unsafe { lock.device.create_buffer(bytes, usage) }
            .map_err(VertexBufferError::BufferCreation)?;
        let req = unsafe { lock.device.get_buffer_requirements(&buf) };
        let ty = lock
            .memory_properties
            .memory_types
            .iter()
            .enumerate()
            .position(|(id, mem_type)| {
                req.type_mask & (1 << id) != 0 && mem_type.properties.contains(properties)
            })
            .unwrap();
        let mem = unsafe { lock.device.allocate_memory(ty.into(), req.size) }
            .map_err(VertexBufferError::MemoryAllocation)?;
        unsafe { lock.device.bind_buffer_memory(&mem, 0, &mut buf) }
            .map_err(VertexBufferError::Bind)?;

        Ok((buf, mem))
    }

    pub fn with_capacity(
        gpu: Arc<Mutex<GPU<B>>>,
        capacity: usize,
    ) -> Result<Self, VertexBufferError> {
        let bytes = (std::mem::size_of::<Vertex>() * capacity) as u64;

        let (staging_buf, staging_mem) = Self::build_buffer(
            gpu.clone(),
            bytes,
            hal::memory::Properties::CPU_VISIBLE,
            hal::buffer::Usage::TRANSFER_SRC,
        )?;
        let (device_buf, device_mem) = Self::build_buffer(
            gpu.clone(),
            bytes,
            hal::memory::Properties::DEVICE_LOCAL,
            hal::buffer::Usage::TRANSFER_DST | hal::buffer::Usage::VERTEX,
        )?;

        Ok(Self {
            gpu,
            capacity,
            staging_mem: ManuallyDrop::new(staging_mem),
            staging_buf: ManuallyDrop::new(staging_buf),
            device_buf: ManuallyDrop::new(device_buf),
            device_mem: ManuallyDrop::new(device_mem),
        })
    }

    /// Resizes the buffers. This will destroy old data!
    fn resize(&mut self, new_capacity: usize) -> Result<(), VertexBufferError> {
        let bytes = (std::mem::size_of::<Vertex>() * new_capacity) as u64;

        let (staging_buf, staging_mem) = Self::build_buffer(
            self.gpu.clone(),
            bytes,
            hal::memory::Properties::CPU_VISIBLE,
            hal::buffer::Usage::TRANSFER_SRC,
        )?;
        let (device_buf, device_mem) = Self::build_buffer(
            self.gpu.clone(),
            bytes,
            hal::memory::Properties::DEVICE_LOCAL,
            hal::buffer::Usage::TRANSFER_DST | hal::buffer::Usage::VERTEX,
        )?;

        {
            let lock = self.gpu.lock().unwrap();

            unsafe {
                lock.device
                    .free_memory(ManuallyDrop::take(&mut self.staging_mem));
                lock.device
                    .free_memory(ManuallyDrop::take(&mut self.device_mem));
                lock.device
                    .destroy_buffer(ManuallyDrop::take(&mut self.staging_buf));
                lock.device
                    .destroy_buffer(ManuallyDrop::take(&mut self.device_buf));
            }
        }

        self.staging_buf = ManuallyDrop::new(staging_buf);
        self.staging_mem = ManuallyDrop::new(staging_mem);
        self.device_buf = ManuallyDrop::new(device_buf);
        self.device_mem = ManuallyDrop::new(device_mem);
        self.capacity = new_capacity;

        Ok(())
    }

    pub fn copy_from_slice(
        &mut self,
        vertices: &[Vertex],
        cmd_buffer: &mut B::CommandBuffer,
    ) -> Result<(), VertexBufferError> {
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
                .map_memory(&*self.staging_mem, hal::memory::Segment::ALL)
        }
        .map_err(VertexBufferError::Map)?;
        unsafe {
            std::ptr::copy_nonoverlapping(vertices.as_ptr() as *const u8, mapping, bytes);
            lock.device
                .flush_mapped_memory_ranges(std::iter::once((
                    &*self.staging_mem,
                    hal::memory::Segment::ALL,
                )))
                .map_err(VertexBufferError::OutOfMemory)?;
            lock.device.unmap_memory(&*self.staging_mem);
        };

        unsafe {
            cmd_buffer.copy_buffer(
                &*self.staging_buf,
                &*self.device_buf,
                std::iter::once(hal::command::BufferCopy {
                    src: 0,
                    dst: 0,
                    size: bytes as u64,
                }),
            );
        }

        Ok(())
    }

    pub fn buffer(&self) -> &B::Buffer {
        &*self.device_buf
    }
}

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
                .free_memory(ManuallyDrop::take(&mut self.staging_mem));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.device_mem));
            lock.device
                .destroy_buffer(ManuallyDrop::take(&mut self.staging_buf));
            lock.device
                .destroy_buffer(ManuallyDrop::take(&mut self.device_buf));
        }
    }
}

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
    pub fn new(gpu: Arc<Mutex<GPU<B>>>, size: [u32; 2]) -> Self {
        let lock = gpu.lock().unwrap();

        let [width, height] = size;
        let kind = i::Kind::D2(width as i::Size, height as i::Size, 1, 1);

        let mut image = unsafe {
            lock.device.create_image(
                kind,
                1,
                GLYPH_CACHE_FORMAT,
                i::Tiling::Linear,
                i::Usage::TRANSFER_DST | i::Usage::SAMPLED,
                i::ViewCapabilities::empty(),
            )
        }
        .unwrap();
        let image_req = unsafe { lock.device.get_image_requirements(&image) };

        let device_type = lock
            .memory_properties
            .memory_types
            .iter()
            .enumerate()
            .position(|(id, memory_type)| {
                image_req.type_mask & (1 << id) != 0
                    && memory_type.properties.contains(m::Properties::DEVICE_LOCAL)
            })
            .unwrap()
            .into();
        let memory = unsafe { lock.device.allocate_memory(device_type, image_req.size) }.unwrap();

        unsafe { lock.device.bind_image_memory(&memory, 0, &mut image) }.unwrap();
        let view = unsafe {
            lock.device.create_image_view(
                &image,
                i::ViewKind::D2,
                GLYPH_CACHE_FORMAT,
                Swizzle::NO,
                COLOR_RANGE.clone(),
            )
        }
        .unwrap();

        GlyphCache {
            gpu: gpu.clone(),
            image: ManuallyDrop::new(image),
            memory: ManuallyDrop::new(memory),
            view: ManuallyDrop::new(view),
            cache_size: image_req.size,
            cache_dims: size,
        }
    }

    pub fn upload(&mut self, command_pool: &mut B::CommandPool, cpu_cache: &[u8]) {
        let mut lock = self.gpu.lock().unwrap();

        let mut image_upload_buffer = unsafe {
            lock.device
                .create_buffer(self.cache_size, buffer::Usage::TRANSFER_SRC)
        }
        .unwrap();
        let image_upload_reqs =
            unsafe { lock.device.get_buffer_requirements(&image_upload_buffer) };

        let upload_type = lock
            .memory_properties
            .memory_types
            .iter()
            .enumerate()
            .position(|(id, mem_type)| {
                image_upload_reqs.type_mask & (1 << id) != 0
                    && mem_type.properties.contains(m::Properties::CPU_VISIBLE)
            })
            .unwrap()
            .into();

        // copy image data into staging buffer
        let image_upload_memory = unsafe {
            let memory = lock
                .device
                .allocate_memory(upload_type, image_upload_reqs.size)
                .unwrap();
            lock.device
                .bind_buffer_memory(&memory, 0, &mut image_upload_buffer)
                .unwrap();
            let mapping = lock.device.map_memory(&memory, m::Segment::ALL).unwrap();
            ptr::copy_nonoverlapping(cpu_cache.as_ptr() as *const u8, mapping, cpu_cache.len());
            lock.device
                .flush_mapped_memory_ranges(iter::once((&memory, m::Segment::ALL)))
                .unwrap();
            lock.device.unmap_memory(&memory);
            memory
        };

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
    }

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

pub struct Renderer<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,

    surface: ManuallyDrop<B::Surface>,
    format: hal::format::Format,

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

    sampler: ManuallyDrop<B::Sampler>,
    mesh: conrod_core::mesh::Mesh,
}

// TODO: MSAA for UI rendering
impl<B> Renderer<B>
where
    B: Backend,
{
    pub fn new<W: raw_window_handle::HasRawWindowHandle>(
        gpu: Arc<Mutex<GPU<B>>>,
        window: &W,
        dimensions: window::Extent2D,
        glyph_cache_dims: [u32; 2],
    ) -> Renderer<B> {
        let vertex_buffer = VertexBuffer::new(gpu.clone()).expect("Error creating Vertex Buffer");
        let glyph_cache = GlyphCache::new(gpu.clone(), glyph_cache_dims);

        let lock = gpu.lock().unwrap();

        let mut surface =
            unsafe { lock.instance.create_surface(window) }.expect("Failed to create surface");

        if !surface.supports_queue_family(&lock.adapter.queue_families[lock.queue_group.family.0]) {
            log::error!("Surface does not support queue family!");
        }

        let mut command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                pool::CommandPoolCreateFlags::empty(),
            )
        }
        .expect("Can't create command pool");

        let sampler = unsafe {
            lock.device
                .create_sampler(&i::SamplerDesc::new(i::Filter::Linear, i::WrapMode::Clamp))
        }
        .expect("Can't create sampler");

        // Setup renderpass and pipeline
        let basic_set_layout = unsafe {
            lock.device.create_descriptor_set_layout(
                &[
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
        }
        .expect("Can't create basic descriptor set layout");
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
        }
        .expect("Can't create image descriptor set layout");

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
        }
        .expect("Can't create descriptor pool");
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
        }
        .expect("Can't create descriptor pool");

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

        let caps = surface.capabilities(&lock.adapter.physical_device);
        let formats = surface.supported_formats(&lock.adapter.physical_device);
        let format = formats.map_or(f::Format::Rgba8Srgb, |formats| {
            formats
                .iter()
                .find(|format| format.base_format().1 == ChannelType::Srgb)
                .copied()
                .unwrap_or(formats[0])
        });

        let swap_config = window::SwapchainConfig::from_caps(&caps, format, dimensions);
        let extent = swap_config.extent;
        unsafe {
            surface
                .configure_swapchain(&lock.device, swap_config)
                .expect("Can't configure swapchain");
        };

        let render_pass = {
            let attachment = pass::Attachment {
                format: Some(format),
                samples: 1,
                ops: pass::AttachmentOps::new(
                    pass::AttachmentLoadOp::Clear,
                    pass::AttachmentStoreOp::Store,
                ),
                stencil_ops: pass::AttachmentOps::DONT_CARE,
                layouts: i::Layout::Undefined..i::Layout::Present,
            };

            let subpass = pass::SubpassDesc {
                colors: &[(0, i::Layout::ColorAttachmentOptimal)],
                depth_stencil: None,
                inputs: &[],
                resolves: &[],
                preserves: &[],
            };

            unsafe {
                lock.device
                    .create_render_pass(&[attachment], &[subpass], &[])
            }
            .expect("Can't create render pass")
        };

        let submission_complete_semaphore = lock
            .device
            .create_semaphore()
            .expect("Could not create semaphore");
        let submission_complete_fence = lock
            .device
            .create_fence(true)
            .expect("Could not create fence");
        let command_buffer = unsafe { command_pool.allocate_one(command::Level::Primary) };

        let pipeline_layout = unsafe {
            lock.device
                .create_pipeline_layout(vec![&basic_set_layout, &image_set_layout], &[])
        }
        .expect("Can't create pipeline layout");
        let pipeline = {
            let vs_module = {
                let spirv = pso::read_spirv(Cursor::new(
                    &include_bytes!("../../shaders/ui-vert.spv")[..],
                ))
                .unwrap();
                unsafe { lock.device.create_shader_module(&spirv) }.unwrap()
            };
            let fs_module = {
                let spirv = pso::read_spirv(Cursor::new(
                    &include_bytes!("../../shaders/ui-frag.spv")[..],
                ))
                .unwrap();
                unsafe { lock.device.create_shader_module(&spirv) }.unwrap()
            };

            let pipeline = {
                let (vs_entry, fs_entry) = (
                    pso::EntryPoint {
                        entry: ENTRY_NAME,
                        module: &vs_module,
                        specialization: hal::spec_const_list![0.8f32],
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
                pipeline_desc.blender.targets.push(pso::ColorBlendDesc {
                    mask: pso::ColorMask::ALL,
                    blend: Some(pso::BlendState::ALPHA),
                });
                pipeline_desc.vertex_buffers.push(pso::VertexBufferDesc {
                    binding: 0,
                    stride: mem::size_of::<Vertex>() as u32,
                    rate: VertexInputRate::Vertex,
                });

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

        Renderer {
            gpu: gpu.clone(),
            surface: ManuallyDrop::new(surface),
            format,
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
            sampler: ManuallyDrop::new(sampler),
            mesh,
        }
    }

    pub fn recreate_swapchain(&mut self, new_dimensions: Option<window::Extent2D>) {
        if let Some(ext) = new_dimensions {
            self.dimensions = ext;
        }

        let lock = self.gpu.lock().unwrap();

        let caps = self.surface.capabilities(&lock.adapter.physical_device);
        let swap_config = window::SwapchainConfig::from_caps(&caps, self.format, self.dimensions);
        let extent = swap_config.extent.to_extent();

        unsafe {
            self.surface
                .configure_swapchain(&lock.device, swap_config)
                .expect("Can't create swapchain");
        }

        self.viewport.rect.w = extent.width as _;
        self.viewport.rect.h = extent.height as _;
    }

    pub fn render<P: conrod_core::render::PrimitiveWalker>(
        &mut self,
        image_map: &conrod_core::image::Map<Image<B>>,
        primitives: P,
    ) {
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
        )
        .expect("Error while filling");

        let surface_image = unsafe {
            match self.surface.acquire_image(!0) {
                Ok((image, _)) => image,
                Err(_) => {
                    self.recreate_swapchain(None);
                    return;
                }
            }
        };

        let framebuffer = {
            let lock = self.gpu.lock().unwrap();

            let framebuffer = unsafe {
                lock.device
                    .create_framebuffer(
                        &self.render_pass,
                        iter::once(surface_image.borrow()),
                        i::Extent {
                            width: self.dimensions.width,
                            height: self.dimensions.height,
                            depth: 1,
                        },
                    )
                    .unwrap()
            };

            // Wait for the fence of the previous submission of this frame and reset it
            unsafe {
                lock.device
                    .wait_for_fence(&self.submission_complete_fence, !0)
                    .expect("Failed to wait for fence");
                lock.device
                    .reset_fence(&self.submission_complete_fence)
                    .expect("Failed to reset fence");
                self.command_pool.reset(false);
            }

            framebuffer
        };

        // Rendering
        let cmd_buffer = &mut *self.command_buffer;
        unsafe {
            cmd_buffer.begin_primary(command::CommandBufferFlags::ONE_TIME_SUBMIT);

            cmd_buffer.set_viewports(0, &[self.viewport.clone()]);
            cmd_buffer.set_scissors(0, &[self.viewport.rect]);
            // Fill vertex buffer and copy to device only memory
            self.vertex_buffer
                .copy_from_slice(conv_vertex_buffer(self.mesh.vertices()), cmd_buffer)
                .unwrap();
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
                &[command::ClearValue {
                    color: command::ClearColor {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                }],
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
                            let descriptor = image_map
                                .get(&img_id)
                                .map(|img| &img.descriptor)
                                .unwrap_or(&self.image_desc_default);
                            cmd_buffer.bind_graphics_descriptor_sets(
                                &self.pipeline_layout,
                                0,
                                vec![&self.basic_desc_set, descriptor],
                                &[],
                            );
                            cmd_buffer.draw(range.start as u32..range.end as u32, 0..1);
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
    }

    pub fn fill<'a, P: conrod_core::render::PrimitiveWalker>(
        &'a mut self,
        image_map: &conrod_core::image::Map<Image<B>>,
        viewport: [f32; 4],
        dpi_factor: f64,
        primitives: P,
    ) -> Result<(), conrod_core::text::rt::gpu_cache::CacheWriteErr> {
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
            );
        }
        Ok(())
    }

    /// Create an image for use in the image map for rendering
    pub fn create_image(&mut self, image_view: &B::ImageView, width: u32, height: u32) -> Image<B> {
        let lock = self.gpu.lock().unwrap();
        let desc = unsafe { self.image_desc_pool.allocate_set(&*self.image_set_layout) }.unwrap();

        unsafe {
            lock.device
                .write_descriptor_sets(vec![pso::DescriptorSetWrite {
                    set: &desc,
                    binding: 0,
                    array_offset: 0,
                    descriptors: Some(pso::Descriptor::Image(
                        image_view,
                        i::Layout::ShaderReadOnlyOptimal,
                    )),
                }]);
        }

        Image {
            descriptor: desc,
            width,
            height,
        }
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
