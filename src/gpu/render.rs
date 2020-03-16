use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::borrow::Borrow;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};

static BLIT_VERTEX_SHADER: &[u8] = include_bytes!("../../shaders/blit-vert.spv");
static BLIT_FRAGMENT_SHADER: &[u8] = include_bytes!("../../shaders/blit-frag.spv");

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
    main_render_pass: ManuallyDrop<B::RenderPass>,
    main_pipeline: ManuallyDrop<B::GraphicsPipeline>,
    image_slots: Vec<ImageSlot<B>>,

    // Synchronization
    complete_fence: ManuallyDrop<B::Fence>,
    complete_semaphore: ManuallyDrop<B::Semaphore>,
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
pub struct ImageSlot<B: Backend> {
    image: B::Image,
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

        // Descriptors
        let set_layout = unsafe { lock.device.create_descriptor_set_layout(&[], &[]) }
            .expect("Can't create descriptor set layout");

        let (render_pass, pipeline) = Self::new_pipeline(
            &lock.device,
            format,
            set_layout,
            include_bytes!("../../shaders/quad.spv"),
            include_bytes!("../../shaders/basic.spv"),
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

        // Synchronization primitives
        let fence = lock.device.create_fence(true).unwrap();
        let semaphore = lock.device.create_semaphore().unwrap();

        // Image slots
        let image_slots = vec![];

        Ok(GPURender {
            gpu: gpu.clone(),
            command_pool: ManuallyDrop::new(command_pool),

            surface: ManuallyDrop::new(surface),
            viewport,
            format,
            dimensions: hal::window::Extent2D { width, height },

            main_render_pass: ManuallyDrop::new(render_pass),
            main_pipeline: ManuallyDrop::new(pipeline),
            image_slots,

            complete_fence: ManuallyDrop::new(fence),
            complete_semaphore: ManuallyDrop::new(semaphore),
        })
    }

    fn new_pipeline(
        device: &B::Device,
        format: hal::format::Format,
        set_layout: B::DescriptorSetLayout,
        vertex_shader: &[u8],
        fragment_shader: &[u8],
    ) -> Result<(B::RenderPass, B::GraphicsPipeline), String> {
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

        Ok((render_pass, pipeline))
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
                .expect("Failed to wait for render fence after 1s");
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

            let cmd_buffer = unsafe {
                let mut cmd_buffer = self.command_pool.allocate_one(hal::command::Level::Primary);
                cmd_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
                cmd_buffer.set_viewports(0, &[self.viewport.clone()]);
                cmd_buffer.set_scissors(0, &[self.viewport.rect]);

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
    fn transfer_image(
        &mut self,
        source: &B::ImageView,
        source_layout: hal::image::Layout,
        destination: &ImageSlot<B>,
    ) -> Result<(), String> {
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

        // for img in self.image_slots.iter_mut() {
        //     unsafe {
        //         lock.device.free_memory(ManuallyDrop::take(img).memory);
        //         lock.device.destroy_image(ManuallyDrop::take(img).image);
        //     }
        // }

        unsafe {
            // TODO: destroy render resources
            lock.device
                .destroy_render_pass(ManuallyDrop::take(&mut self.main_render_pass));
            lock.device
                .destroy_command_pool(ManuallyDrop::take(&mut self.command_pool));
        }
    }
}
