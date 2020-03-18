use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::borrow::Borrow;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};

static MAIN_VERTEX_SHADER: &[u8] = include_bytes!("../../shaders/quad.spv");
static MAIN_FRAGMENT_SHADER: &[u8] = include_bytes!("../../shaders/basic.spv");

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
    main_descriptor_set: ManuallyDrop<B::DescriptorSet>,
    sampler: ManuallyDrop<B::Sampler>,
    image_slot: ImageSlot<B>,

    // Synchronization
    complete_fence: ManuallyDrop<B::Fence>,
    complete_semaphore: ManuallyDrop<B::Semaphore>,
    transfer_fence: ManuallyDrop<B::Fence>,
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
/// All image slots have the same format, since one the way to display,
/// grayscale has to be converted to RGBA anyway. Furthermore the bit depth is
/// generally set by the surface and is generally also lower than 16 bits. We
/// therefore initialize all images with the *surface format*.
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
}

impl<B> ImageSlot<B>
where
    B: Backend,
{
    pub const FORMAT: hal::format::Format = hal::format::Format::Rgba8Snorm;

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
    pub fn new(
        gpu: &Arc<Mutex<GPU<B>>>,
        mut surface: B::Surface,
        width: u32,
        height: u32,
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
                        ty: hal::pso::DescriptorType::SampledImage,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 1,
                        ty: hal::pso::DescriptorType::Sampler,
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
            main_set_layout,
            MAIN_VERTEX_SHADER,
            MAIN_FRAGMENT_SHADER,
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

        // Synchronization primitives
        let fence = lock.device.create_fence(true).unwrap();
        let tfence = lock.device.create_fence(false).unwrap();
        let semaphore = lock.device.create_semaphore().unwrap();

        // Image slots
        let image_slot = ImageSlot::new(&lock.device, &lock.memory_properties)?;

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
            main_descriptor_set: ManuallyDrop::new(main_descriptor_set),
            image_slot,
            sampler: ManuallyDrop::new(sampler),

            complete_fence: ManuallyDrop::new(fence),
            complete_semaphore: ManuallyDrop::new(semaphore),
            transfer_fence: ManuallyDrop::new(tfence),
        })
    }

    fn new_pipeline(
        device: &B::Device,
        format: hal::format::Format,
        set_layout: B::DescriptorSetLayout,
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
                .create_pipeline_layout(std::iter::once(&set_layout), &[])
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
                        set: &*self.main_descriptor_set,
                        binding: 0,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slot.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &*self.main_descriptor_set,
                        binding: 1,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Sampler(&*self.sampler)),
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
                    &[hal::memory::Barrier::Image {
                        states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                            ..(
                                hal::image::Access::SHADER_READ,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            ),
                        target: &*self.image_slot.image,
                        families: None,
                        range: super::COLOR_RANGE.clone(),
                    }],
                );

                cmd_buffer.bind_graphics_descriptor_sets(
                    &self.main_pipeline_layout,
                    0,
                    std::iter::once(&*self.main_descriptor_set),
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
    ) -> Result<(), String> {
        let blits: Vec<_> = (0..self.image_slot.mip_levels)
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
                        target: &*self.image_slot.image,
                        families: None,
                        range: hal::image::SubresourceRange {
                            aspects: hal::format::Aspects::COLOR,
                            levels: 0..self.image_slot.mip_levels,
                            layers: 0..1,
                        },
                    },
                ],
            );
            cmd_buffer.blit_image(
                source,
                hal::image::Layout::TransferSrcOptimal,
                &*self.image_slot.image,
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
                        target: &*self.image_slot.image,
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
            lock.device.wait_for_fence(&*self.transfer_fence, 5_000_000_000).unwrap();
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

        unsafe {
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.image_slot.memory));
            lock.device
                .destroy_image_view(ManuallyDrop::take(&mut self.image_slot.view));
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.image_slot.image));
        }

        unsafe {
            // TODO: destroy render resources
            lock.device
                .destroy_render_pass(ManuallyDrop::take(&mut self.main_render_pass));
            lock.device
                .destroy_command_pool(ManuallyDrop::take(&mut self.command_pool));
        }
    }
}
