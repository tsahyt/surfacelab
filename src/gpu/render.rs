use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::borrow::Borrow;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use zerocopy::AsBytes;

static MAIN_VERTEX_SHADER: &[u8] = include_bytes!("../../shaders/quad.spv");
static MAIN_FRAGMENT_SHADER_2D: &[u8] = include_bytes!("../../shaders/renderer2d.spv");
static MAIN_FRAGMENT_SHADER_3D: &[u8] = include_bytes!("../../shaders/renderer3d.spv");

use super::{Backend, GPU};

pub struct GPURender<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: ManuallyDrop<B::CommandPool>,

    // Surface Data and Geometry
    surface: ManuallyDrop<B::Surface>,
    viewport: hal::pso::Viewport,
    format: hal::format::Format,
    dimensions: hal::window::Extent2D,

    // Rendering Data
    descriptor_pool: ManuallyDrop<B::DescriptorPool>,
    main_render_pass: ManuallyDrop<B::RenderPass>,
    main_pipeline: ManuallyDrop<B::GraphicsPipeline>,
    main_pipeline_layout: ManuallyDrop<B::PipelineLayout>,
    main_descriptor_set: B::DescriptorSet,
    main_descriptor_set_layout: ManuallyDrop<B::DescriptorSetLayout>,
    uniform_buffer: ManuallyDrop<B::Buffer>,
    uniform_memory: ManuallyDrop<B::Memory>,
    sampler: ManuallyDrop<B::Sampler>,
    image_slots: ImageSlots<B>,

    // Synchronization
    complete_fence: ManuallyDrop<B::Fence>,
    complete_semaphore: ManuallyDrop<B::Semaphore>,
    transfer_fence: ManuallyDrop<B::Fence>,
}

struct ImageSlots<B: Backend> {
    albedo: ImageSlot<B>,
    roughness: ImageSlot<B>,
    normal: ImageSlot<B>,
    displacement: ImageSlot<B>,
    metallic: ImageSlot<B>,
}

/// Uniform struct to pass to the shader to make decisions on what to render and
/// where to use defaults.
#[derive(AsBytes)]
#[repr(C)]
struct SlotOccupancy {
    albedo: u32,
    roughness: u32,
    normal: u32,
    displacement: u32,
    metallic: u32,
}

/// The renderer holds a fixed number of image slots, as opposed to the compute
/// component which can hold a dynamic amount of images. Each Image Slot also
/// contains MIP levels to reduce artifacts during rendering.
///
/// Since these image slots are separate from the compute images, compute images
/// have to be copied for display first.
///
/// For 2D renderers, the MIP levels can be set to 0, because the artifacts do
/// not occur when just blitting the image.
///
/// Contrary to images in the compute component, which are allocated in a common
/// memory pool, every image slot is given its own memory allocation. This works
/// out because there are comparatively few renderers with few image slots
/// compared to potentially large numbers of compute images.
pub struct ImageSlot<B: Backend> {
    image: ManuallyDrop<B::Image>,
    view: ManuallyDrop<B::ImageView>,
    memory: ManuallyDrop<B::Memory>,
    mip_levels: u8,
    occupied: bool,
}

impl<B> ImageSlot<B>
where
    B: Backend,
{
    pub const FORMAT: hal::format::Format = hal::format::Format::Rgba16Sfloat;

    pub fn new(
        device: &B::Device,
        memory_properties: &hal::adapter::MemoryProperties,
    ) -> Result<Self, String> {
        let mip_levels = 8;

        // Create Image
        let mut image = unsafe {
            device.create_image(
                hal::image::Kind::D2(1024, 1024, 1, 1),
                mip_levels,
                Self::FORMAT,
                hal::image::Tiling::Optimal,
                hal::image::Usage::SAMPLED | hal::image::Usage::TRANSFER_DST,
                hal::image::ViewCapabilities::empty(),
            )
        }
        .map_err(|_| "Failed to create render image")?;

        // Allocate and bind memory for image
        let requirements = unsafe { device.get_image_requirements(&image) };
        let memory_type = memory_properties
            .memory_types
            .iter()
            .position(|mem_type| {
                mem_type
                    .properties
                    .contains(hal::memory::Properties::DEVICE_LOCAL)
            })
            .unwrap()
            .into();
        let image_memory = unsafe { device.allocate_memory(memory_type, requirements.size) }
            .map_err(|_| "Failed to allocate memory for render image")?;
        unsafe { device.bind_image_memory(&image_memory, 0, &mut image) }.unwrap();

        let image_view = unsafe {
            device.create_image_view(
                &image,
                hal::image::ViewKind::D2,
                Self::FORMAT,
                hal::format::Swizzle::NO,
                super::COLOR_RANGE.clone(),
            )
        }
        .map_err(|_| "Failed to create render image view")?;

        Ok(ImageSlot {
            image: ManuallyDrop::new(image),
            view: ManuallyDrop::new(image_view),
            memory: ManuallyDrop::new(image_memory),
            mip_levels: 8,
            occupied: false,
        })
    }
}

pub fn create_surface<B: Backend, H: raw_window_handle::HasRawWindowHandle>(
    gpu: &Arc<Mutex<GPU<B>>>,
    handle: &H,
) -> B::Surface {
    let lock = gpu.lock().unwrap();
    unsafe {
        lock.instance
            .create_surface(handle)
            .expect("Unable to create surface from handle")
    }
}

impl<B> GPURender<B>
where
    B: Backend,
{
    const UNIFORM_BUFFER_SIZE: u64 = 256;
    pub fn new(
        gpu: &Arc<Mutex<GPU<B>>>,
        mut surface: B::Surface,
        width: u32,
        height: u32,
        ty: crate::lang::RendererType,
    ) -> Result<Self, String> {
        log::info!("Obtaining GPU Render Resources");
        let lock = gpu.lock().unwrap();

        // Check whether the surface supports the selected queue family
        if !surface.supports_queue_family(&lock.adapter.queue_families[lock.queue_group.family.0]) {
            return Err("Surface does not support selected queue family!".into());
        }

        // Getting capabilities and deciding on format
        let caps = surface.capabilities(&lock.adapter.physical_device);
        let formats = surface.supported_formats(&lock.adapter.physical_device);

        log::debug!("Surface capabilities: {:?}", caps);
        log::debug!("Surface preferred formats: {:?}", formats);

        let format = formats.map_or(hal::format::Format::Rgba8Srgb, |formats| {
            formats
                .iter()
                .find(|format| format.base_format().1 == hal::format::ChannelType::Srgb)
                .copied()
                .unwrap_or(formats[0])
        });

        log::debug!("Using surface format {:?}", format);

        // Create initial swapchain configuration
        let swap_config = hal::window::SwapchainConfig::from_caps(
            &caps,
            format,
            hal::window::Extent2D { width, height },
        );

        unsafe {
            surface
                .configure_swapchain(&lock.device, swap_config)
                .expect("Can't configure swapchain");
        };

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "Can't create command pool!")?;

        let mut descriptor_pool = unsafe {
            use hal::pso::*;
            lock.device.create_descriptor_pool(
                2,
                &[
                    DescriptorRangeDesc {
                        ty: DescriptorType::UniformBuffer,
                        count: 8,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Sampler,
                        count: 2,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::SampledImage,
                        count: 16,
                    },
                ],
                DescriptorPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "Failed to create render descriptor pool")?;

        // Main Rendering Data
        let main_set_layout = unsafe {
            lock.device.create_descriptor_set_layout(
                &[
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 0,
                        ty: hal::pso::DescriptorType::Sampler,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 1,
                        ty: hal::pso::DescriptorType::UniformBuffer,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 2,
                        ty: hal::pso::DescriptorType::SampledImage,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 3,
                        ty: hal::pso::DescriptorType::SampledImage,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 4,
                        ty: hal::pso::DescriptorType::SampledImage,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 5,
                        ty: hal::pso::DescriptorType::SampledImage,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                ],
                &[],
            )
        }
        .expect("Can't create main descriptor set layout");

        let main_descriptor_set = unsafe { descriptor_pool.allocate_set(&main_set_layout) }
            .map_err(|_| "Failed to allocate render descriptor set")?;

        let (main_render_pass, main_pipeline, main_pipeline_layout) = Self::new_pipeline(
            &lock.device,
            format,
            &main_set_layout,
            MAIN_VERTEX_SHADER,
            match ty {
                crate::lang::RendererType::Renderer2D => MAIN_FRAGMENT_SHADER_2D,
                crate::lang::RendererType::Renderer3D => MAIN_FRAGMENT_SHADER_3D,
            },
        )?;

        // Rendering setup
        let viewport = hal::pso::Viewport {
            rect: hal::pso::Rect {
                x: 0,
                y: 0,
                w: width as _,
                h: height as _,
            },
            depth: 0.0..1.0,
        };

        // Shared Sampler
        let sampler = unsafe {
            lock.device.create_sampler(&hal::image::SamplerDesc::new(
                hal::image::Filter::Linear,
                hal::image::WrapMode::Tile,
            ))
        }
        .map_err(|_| "Failed to create render sampler")?;

        // Uniforms
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

        // Synchronization primitives
        let fence = lock.device.create_fence(true).unwrap();
        let tfence = lock.device.create_fence(false).unwrap();
        let semaphore = lock.device.create_semaphore().unwrap();

        // Image slots
        let image_slots = ImageSlots {
            albedo: ImageSlot::new(&lock.device, &lock.memory_properties)?,
            roughness: ImageSlot::new(&lock.device, &lock.memory_properties)?,
            normal: ImageSlot::new(&lock.device, &lock.memory_properties)?,
            displacement: ImageSlot::new(&lock.device, &lock.memory_properties)?,
            metallic: ImageSlot::new(&lock.device, &lock.memory_properties)?,
        };

        Ok(GPURender {
            gpu: gpu.clone(),
            command_pool: ManuallyDrop::new(command_pool),

            surface: ManuallyDrop::new(surface),
            viewport,
            format,
            dimensions: hal::window::Extent2D { width, height },

            descriptor_pool: ManuallyDrop::new(descriptor_pool),
            main_render_pass: ManuallyDrop::new(main_render_pass),
            main_pipeline: ManuallyDrop::new(main_pipeline),
            main_pipeline_layout: ManuallyDrop::new(main_pipeline_layout),
            main_descriptor_set: main_descriptor_set,
            main_descriptor_set_layout: ManuallyDrop::new(main_set_layout),
            image_slots,
            uniform_buffer: ManuallyDrop::new(uniform_buf),
            uniform_memory: ManuallyDrop::new(uniform_mem),
            sampler: ManuallyDrop::new(sampler),

            complete_fence: ManuallyDrop::new(fence),
            complete_semaphore: ManuallyDrop::new(semaphore),
            transfer_fence: ManuallyDrop::new(tfence),
        })
    }

    #[allow(clippy::type_complexity)]
    fn new_pipeline(
        device: &B::Device,
        format: hal::format::Format,
        set_layout: &B::DescriptorSetLayout,
        vertex_shader: &[u8],
        fragment_shader: &[u8],
    ) -> Result<(B::RenderPass, B::GraphicsPipeline, B::PipelineLayout), String> {
        // Create Render Pass
        let render_pass = {
            let attachment = hal::pass::Attachment {
                format: Some(format),
                samples: 1,
                ops: hal::pass::AttachmentOps::new(
                    hal::pass::AttachmentLoadOp::Clear,
                    hal::pass::AttachmentStoreOp::Store,
                ),
                stencil_ops: hal::pass::AttachmentOps::DONT_CARE,
                layouts: hal::image::Layout::Undefined..hal::image::Layout::Present,
            };

            let subpass = hal::pass::SubpassDesc {
                colors: &[(0, hal::image::Layout::ColorAttachmentOptimal)],
                depth_stencil: None,
                inputs: &[],
                resolves: &[],
                preserves: &[],
            };

            unsafe { device.create_render_pass(&[attachment], &[subpass], &[]) }
                .expect("Can't create render pass")
        };

        // Pipeline
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(std::iter::once(set_layout), &[])
                .expect("Can't create pipeline layout")
        };

        let pipeline = {
            let vs_module = {
                let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(vertex_shader))
                    .map_err(|e| format!("Failed to load vertex shader SPIR-V: {}", e))?;
                unsafe { device.create_shader_module(&loaded_spirv) }
                    .map_err(|e| format!("Failed to build vertex shader module: {}", e))?
            };
            let fs_module = {
                let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(fragment_shader))
                    .map_err(|e| format!("Failed to load fragment shader SPIR-V: {}", e))?;
                unsafe { device.create_shader_module(&loaded_spirv) }
                    .map_err(|e| format!("Failed to build fragment shader module: {}", e))?
            };

            let pipeline = {
                let shader_entries = hal::pso::GraphicsShaderSet {
                    vertex: hal::pso::EntryPoint {
                        entry: "main",
                        module: &vs_module,
                        specialization: hal::pso::Specialization::default(),
                    },
                    hull: None,
                    domain: None,
                    geometry: None,
                    fragment: Some(hal::pso::EntryPoint {
                        entry: "main",
                        module: &fs_module,
                        specialization: hal::pso::Specialization::default(),
                    }),
                };

                let subpass = hal::pass::Subpass {
                    index: 0,
                    main_pass: &render_pass,
                };

                let mut pipeline_desc = hal::pso::GraphicsPipelineDesc::new(
                    shader_entries,
                    hal::pso::Primitive::TriangleList,
                    hal::pso::Rasterizer::FILL,
                    &pipeline_layout,
                    subpass,
                );
                pipeline_desc
                    .blender
                    .targets
                    .push(hal::pso::ColorBlendDesc {
                        mask: hal::pso::ColorMask::ALL,
                        blend: Some(hal::pso::BlendState::ALPHA),
                    });

                unsafe { device.create_graphics_pipeline(&pipeline_desc, None) }
            };

            unsafe {
                device.destroy_shader_module(vs_module);
            }
            unsafe {
                device.destroy_shader_module(fs_module);
            }

            pipeline.unwrap()
        };

        Ok((render_pass, pipeline, pipeline_layout))
    }

    pub fn set_dimensions(&mut self, width: u32, height: u32) {
        self.dimensions.width = width;
        self.dimensions.height = height;
    }

    pub fn recreate_swapchain(&mut self) {
        let lock = self.gpu.lock().unwrap();

        let caps = self.surface.capabilities(&lock.adapter.physical_device);
        let swap_config =
            hal::window::SwapchainConfig::from_caps(&caps, self.format, self.dimensions);
        let extent = swap_config.extent.to_extent();

        unsafe {
            self.surface
                .configure_swapchain(&lock.device, swap_config)
                .expect("Failed to recreate swapchain");
        }

        self.viewport.rect.w = extent.width as _;
        self.viewport.rect.h = extent.height as _;
    }

    fn synchronize_at_fence(&self) {
        let lock = self.gpu.lock().unwrap();
        unsafe {
            lock.device
                .wait_for_fence(&self.complete_fence, 10_000_000_000)
                .expect("Failed to wait for render fence after 10s");
            lock.device
                .reset_fence(&self.complete_fence)
                .expect("Failed to reset render fence");
        }
    }

    fn build_uniforms(&self) -> SlotOccupancy {
        fn from_bool(x: bool) -> u32 {
            if x {
                1
            } else {
                0
            }
        }

        SlotOccupancy {
            albedo: from_bool(self.image_slots.albedo.occupied),
            roughness: from_bool(self.image_slots.roughness.occupied),
            normal: from_bool(self.image_slots.normal.occupied),
            displacement: from_bool(self.image_slots.displacement.occupied),
            metallic: from_bool(self.image_slots.metallic.occupied),
        }
    }

    fn fill_uniforms(&self, device: &B::Device, uniforms: &[u8]) -> Result<(), String> {
        debug_assert!(uniforms.len() <= Self::UNIFORM_BUFFER_SIZE as usize);

        unsafe {
            let mapping = device
                .map_memory(&*self.uniform_memory, 0..Self::UNIFORM_BUFFER_SIZE)
                .map_err(|e| {
                    format!("Failed to map uniform buffer into CPU address space: {}", e)
                })?;
            std::ptr::copy_nonoverlapping(
                uniforms.as_ptr() as *const u8,
                mapping,
                uniforms.len() as usize,
            );
            device.unmap_memory(&*self.uniform_memory);
        }

        Ok(())
    }

    pub fn render(&mut self) {
        let surface_image = unsafe {
            match self.surface.acquire_image(!0) {
                Ok((image, _)) => image,
                Err(_) => {
                    self.recreate_swapchain();
                    return;
                }
            }
        };

        // Wait on previous fence to make sure the last frame has been rendered.
        self.synchronize_at_fence();

        let result = {
            let mut lock = self.gpu.lock().unwrap();

            let uniforms = self.build_uniforms();
            self.fill_uniforms(&lock.device, uniforms.as_bytes())
                .expect("Error filling uniforms during render");

            let framebuffer = unsafe {
                lock.device
                    .create_framebuffer(
                        &self.main_render_pass,
                        std::iter::once(surface_image.borrow()),
                        hal::image::Extent {
                            width: self.dimensions.width,
                            height: self.dimensions.height,
                            depth: 1,
                        },
                    )
                    .unwrap()
            };

            unsafe {
                use hal::pso::*;
                lock.device.write_descriptor_sets(vec![
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 0,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Sampler(&*self.sampler)),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 1,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Buffer(&*self.uniform_buffer, None..None)),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 2,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.displacement.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 3,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.albedo.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 4,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.normal.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 5,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.roughness.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                ])
            }

            let cmd_buffer = unsafe {
                let mut cmd_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
                cmd_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
                cmd_buffer.set_viewports(0, &[self.viewport.clone()]);
                cmd_buffer.set_scissors(0, &[self.viewport.rect]);

                cmd_buffer.pipeline_barrier(
                    hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::FRAGMENT_SHADER,
                    hal::memory::Dependencies::empty(),
                    &[
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.displacement.image,
                            families: None,
                            range: super::COLOR_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.albedo.image,
                            families: None,
                            range: super::COLOR_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.normal.image,
                            families: None,
                            range: super::COLOR_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.roughness.image,
                            families: None,
                            range: super::COLOR_RANGE.clone(),
                        },
                    ],
                );

                cmd_buffer.bind_graphics_descriptor_sets(
                    &self.main_pipeline_layout,
                    0,
                    std::iter::once(&self.main_descriptor_set),
                    &[],
                );
                cmd_buffer.bind_graphics_pipeline(&self.main_pipeline);
                cmd_buffer.begin_render_pass(
                    &self.main_render_pass,
                    &framebuffer,
                    self.viewport.rect,
                    &[hal::command::ClearValue {
                        color: hal::command::ClearColor {
                            float32: [0.8, 0.8, 0.8, 1.0],
                        },
                    }],
                    hal::command::SubpassContents::Inline,
                );
                cmd_buffer.draw(0..6, 0..1);
                cmd_buffer.end_render_pass();
                cmd_buffer.finish();

                cmd_buffer
            };

            // Submit for render
            unsafe {
                lock.queue_group.queues[0].submit(
                    hal::queue::Submission {
                        command_buffers: std::iter::once(&cmd_buffer),
                        wait_semaphores: None,
                        signal_semaphores: std::iter::once(&*self.complete_semaphore),
                    },
                    Some(&self.complete_fence),
                )
            };

            // Present frame
            let result = unsafe {
                lock.queue_group.queues[0].present_surface(
                    &mut self.surface,
                    surface_image,
                    Some(&self.complete_semaphore),
                )
            };

            unsafe { lock.device.destroy_framebuffer(framebuffer) };

            result
        };

        if result.is_err() {
            self.recreate_swapchain();
        }
    }

    /// Transfer an external (usually compute) image to an image slot.
    ///
    /// This is done by *blitting* the source image, since there is a potential
    /// format conversion going on in the process. The image slot has the same
    /// format as the target surface to render on, whereas the source image can
    /// be potentially any format.
    ///
    /// Blitting is performed once per MIP level of the image slot, such that
    /// the MIP hierarchy is created.
    pub fn transfer_image(
        &mut self,
        source: &B::Image,
        source_layout: hal::image::Layout,
        source_access: hal::image::Access,
        image_use: crate::lang::OutputType,
    ) -> Result<(), String> {
        let image_slot = match image_use {
            crate::lang::OutputType::Displacement => &mut self.image_slots.displacement,
            crate::lang::OutputType::Albedo => &mut self.image_slots.albedo,
            crate::lang::OutputType::Roughness => &mut self.image_slots.roughness,
            crate::lang::OutputType::Normal => &mut self.image_slots.normal,
            crate::lang::OutputType::Metallic => &mut self.image_slots.metallic,
            _ => return Ok(()),
        };

        image_slot.occupied = true;

        let blits: Vec<_> = (0..image_slot.mip_levels)
            .map(|level| hal::command::ImageBlit {
                src_subresource: hal::image::SubresourceLayers {
                    aspects: hal::format::Aspects::COLOR,
                    level: 0,
                    layers: 0..1,
                },
                src_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                    x: 1024,
                    y: 1024,
                    z: 1,
                },
                dst_subresource: hal::image::SubresourceLayers {
                    aspects: hal::format::Aspects::COLOR,
                    level,
                    layers: 0..1,
                },
                dst_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                    x: 1024 >> level,
                    y: 1024 >> level,
                    z: 1,
                },
            })
            .collect();

        let cmd_buffer = unsafe {
            let mut cmd_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
            cmd_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[
                    hal::memory::Barrier::Image {
                        states: (hal::image::Access::empty(), source_layout)
                            ..(
                                hal::image::Access::TRANSFER_READ,
                                hal::image::Layout::TransferSrcOptimal,
                            ),
                        target: source,
                        families: None,
                        range: super::COLOR_RANGE.clone(),
                    },
                    hal::memory::Barrier::Image {
                        states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                            ..(
                                hal::image::Access::TRANSFER_WRITE,
                                hal::image::Layout::TransferDstOptimal,
                            ),
                        target: &*image_slot.image,
                        families: None,
                        range: hal::image::SubresourceRange {
                            aspects: hal::format::Aspects::COLOR,
                            levels: 0..image_slot.mip_levels,
                            layers: 0..1,
                        },
                    },
                ],
            );
            cmd_buffer.blit_image(
                source,
                hal::image::Layout::TransferSrcOptimal,
                &*image_slot.image,
                hal::image::Layout::TransferDstOptimal,
                hal::image::Filter::Nearest,
                &blits,
            );
            cmd_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TRANSFER..hal::pso::PipelineStage::FRAGMENT_SHADER,
                hal::memory::Dependencies::empty(),
                &[
                    hal::memory::Barrier::Image {
                        states: (
                            hal::image::Access::TRANSFER_READ,
                            hal::image::Layout::TransferSrcOptimal,
                        )..(source_access, source_layout),
                        target: source,
                        families: None,
                        range: super::COLOR_RANGE.clone(),
                    },
                    hal::memory::Barrier::Image {
                        states: (
                            hal::image::Access::TRANSFER_WRITE,
                            hal::image::Layout::TransferDstOptimal,
                        )
                            ..(
                                hal::image::Access::SHADER_READ,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            ),
                        target: &*image_slot.image,
                        families: None,
                        range: super::COLOR_RANGE.clone(),
                    },
                ],
            );
            cmd_buffer.finish();
            cmd_buffer
        };

        let mut lock = self.gpu.lock().unwrap();
        unsafe {
            lock.device.reset_fence(&*self.transfer_fence).unwrap();
            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&cmd_buffer), Some(&self.transfer_fence));
            lock.device
                .wait_for_fence(&*self.transfer_fence, 5_000_000_000)
                .unwrap();
        }

        unsafe {
            self.command_pool.free(Some(cmd_buffer));
        }

        Ok(())
    }
}

impl<B> Drop for GPURender<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        // Finish all rendering before destruction of resources
        self.synchronize_at_fence();

        log::info!("Releasing GPU Render resources");

        let lock = self.gpu.lock().unwrap();

        fn free_slot<B: Backend>(device: &B::Device, slot: &mut ImageSlot<B>) {
            unsafe {
                device.free_memory(ManuallyDrop::take(&mut slot.memory));
                device.destroy_image_view(ManuallyDrop::take(&mut slot.view));
                device.destroy_image(ManuallyDrop::take(&mut slot.image));
            }
        }

        // Destroy Image Slots
        free_slot(&lock.device, &mut self.image_slots.albedo);
        free_slot(&lock.device, &mut self.image_slots.roughness);
        free_slot(&lock.device, &mut self.image_slots.normal);
        free_slot(&lock.device, &mut self.image_slots.displacement);
        free_slot(&lock.device, &mut self.image_slots.metallic);

        unsafe {
            lock.device
                .destroy_buffer(ManuallyDrop::take(&mut self.uniform_buffer));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.uniform_memory));
            lock.device
                .destroy_command_pool(ManuallyDrop::take(&mut self.command_pool));
            self.surface.unconfigure_swapchain(&lock.device);
            lock.instance
                .destroy_surface(ManuallyDrop::take(&mut self.surface));
            lock.device
                .destroy_descriptor_pool(ManuallyDrop::take(&mut self.descriptor_pool));
            lock.device
                .destroy_descriptor_set_layout(ManuallyDrop::take(
                    &mut self.main_descriptor_set_layout,
                ));
            lock.device
                .destroy_render_pass(ManuallyDrop::take(&mut self.main_render_pass));
            lock.device
                .destroy_graphics_pipeline(ManuallyDrop::take(&mut self.main_pipeline));
            lock.device
                .destroy_pipeline_layout(ManuallyDrop::take(&mut self.main_pipeline_layout));
            lock.device
                .destroy_sampler(ManuallyDrop::take(&mut self.sampler));
            lock.device
                .destroy_fence(ManuallyDrop::take(&mut self.complete_fence));
            lock.device
                .destroy_semaphore(ManuallyDrop::take(&mut self.complete_semaphore));
            lock.device
                .destroy_fence(ManuallyDrop::take(&mut self.transfer_fence));
        }
    }
}
