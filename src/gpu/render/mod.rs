use super::RenderTarget;
use crate::lang::{LightType, ParameterBool};

use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use zerocopy::AsBytes;

use super::{Backend, InitializationError, PipelineError, GPU};

pub mod brdf_lut;
pub mod environment;

use environment::EnvironmentMaps;

static MAIN_VERTEX_SHADER: &[u8] = include_bytes!("../../../shaders/quad.spv");
static MAIN_FRAGMENT_SHADER_2D: &[u8] = include_bytes!("../../../shaders/renderer2d.spv");
static MAIN_FRAGMENT_SHADER_3D: &[u8] = include_bytes!("../../../shaders/renderer3d.spv");
static ACCUM_SHADER: &[u8] = include_bytes!("../../../shaders/accum.spv");

const IRRADIANCE_SIZE: usize = 32;
const SPECMAP_SIZE: usize = 512;

#[derive(AsBytes, Debug)]
#[repr(C)]
/// Uniforms for a 2D Renderer
struct RenderView2D {
    resolution: [f32; 2],
    pan: [f32; 2],
    zoom: f32,
    channel: u32,
}

impl Default for RenderView2D {
    fn default() -> Self {
        Self {
            resolution: [1024.0, 1024.0],
            pan: [0., 0.],
            zoom: 1.,
            channel: 0,
        }
    }
}

#[derive(AsBytes, Debug)]
#[repr(C)]
/// Uniforms for a 3D Renderer
struct RenderView3D {
    center: [f32; 4],
    light_pos: [f32; 4],
    resolution: [f32; 2],

    phi: f32,
    theta: f32,
    rad: f32,

    displacement: f32,
    tex_scale: f32,
    texel_size: f32,

    environment_strength: f32,

    light_type: LightType,
    light_strength: f32,
    fog_strength: f32,

    shadow: ParameterBool,
    ao: ParameterBool,
}

impl Default for RenderView3D {
    fn default() -> Self {
        Self {
            resolution: [1024.0, 1024.0],
            center: [0., 0., 0., 0.],
            light_pos: [0., 3., 0., 0.],
            phi: 1.,
            theta: 1.,
            rad: 6.,
            displacement: 0.5,
            tex_scale: 8.,
            texel_size: 8. / 1024.,
            environment_strength: 1.0,
            light_type: LightType::PointLight,
            light_strength: 100.0,
            fog_strength: 0.2,
            shadow: 1,
            ao: 0,
        }
    }
}

#[derive(Debug)]
enum RenderView {
    RenderView2D(RenderView2D),
    RenderView3D(RenderView3D),
}

/// An iterator over a 2D (2,3)-Halton sequence for QMC, except index 0 is added
/// as (0.5, 0.5) to get a clean center sample first.
pub struct HaltonSequence2D {
    idx: usize,
    base1: f32,
    base2: f32,
}

impl Default for HaltonSequence2D {
    fn default() -> Self {
        Self::new(2, 3)
    }
}

impl HaltonSequence2D {
    pub fn new(base1: usize, base2: usize) -> Self {
        Self {
            idx: 0,
            base1: base1 as f32,
            base2: base2 as f32,
        }
    }

    fn halton_1d(mut idx: f32, base: f32) -> f32 {
        let mut fraction = 1.0;
        let mut result = 0.0;

        while idx > 0.0 {
            fraction /= base;
            result += fraction * (idx % base);
            idx = (idx / base).floor();
        }

        return result;
    }
}

impl Iterator for HaltonSequence2D {
    type Item = (f32, f32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == 0 {
            self.idx += 1;
            Some((0.5, 0.5))
        } else {
            let x = Self::halton_1d((self.idx - 1) as f32, self.base1);
            let y = Self::halton_1d((self.idx - 1) as f32, self.base2);
            self.idx += 1;
            Some((x, y))
        }
    }
}

pub struct GPURender<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: ManuallyDrop<B::CommandPool>,

    // Target Data and Geometry
    viewport: hal::pso::Viewport,
    dimensions: hal::window::Extent2D,
    render_target: RenderTarget<B>,
    accum_target: RenderTarget<B>,
    current_sample: usize,

    // View
    view: RenderView,

    // Rendering Data
    halton_sampler: HaltonSequence2D,
    descriptor_pool: ManuallyDrop<B::DescriptorPool>,
    main_render_pass: ManuallyDrop<B::RenderPass>,
    main_pipeline: ManuallyDrop<B::GraphicsPipeline>,
    main_pipeline_layout: ManuallyDrop<B::PipelineLayout>,
    main_descriptor_set: B::DescriptorSet,
    main_descriptor_set_layout: ManuallyDrop<B::DescriptorSetLayout>,

    accum_pipeline: ManuallyDrop<B::ComputePipeline>,
    accum_pipeline_layout: ManuallyDrop<B::PipelineLayout>,
    accum_descriptor_set: B::DescriptorSet,
    accum_descriptor_set_layout: ManuallyDrop<B::DescriptorSetLayout>,

    sampler: ManuallyDrop<B::Sampler>,

    // Buffers
    occupancy_buffer: ManuallyDrop<B::Buffer>,
    occupancy_memory: ManuallyDrop<B::Memory>,
    uniform_buffer: ManuallyDrop<B::Buffer>,
    uniform_memory: ManuallyDrop<B::Memory>,
    image_slots: ImageSlots<B>,
    environment_maps: EnvironmentMaps<B>,
    image_size: u32,

    // Synchronization
    complete_fence: ManuallyDrop<B::Fence>,
    transfer_fence: ManuallyDrop<B::Fence>,
}

pub struct ImageSlots<B: Backend> {
    albedo: ImageSlot<B>,
    roughness: ImageSlot<B>,
    normal: ImageSlot<B>,
    displacement: ImageSlot<B>,
    metallic: ImageSlot<B>,
}

impl<B: Backend> ImageSlots<B> {
    pub fn new(
        device: &B::Device,
        memory_properties: &hal::adapter::MemoryProperties,
        image_size: u32,
    ) -> Result<Self, InitializationError> {
        Ok(Self {
            albedo: ImageSlot::new(
                device,
                memory_properties,
                hal::format::Format::Rgba16Sfloat,
                image_size,
            )?,
            roughness: ImageSlot::new(
                device,
                memory_properties,
                hal::format::Format::R16Sfloat,
                image_size,
            )?,
            normal: ImageSlot::new(
                device,
                memory_properties,
                hal::format::Format::Rgba16Sfloat,
                image_size,
            )?,
            displacement: ImageSlot::new(
                device,
                memory_properties,
                hal::format::Format::R32Sfloat,
                image_size,
            )?,
            metallic: ImageSlot::new(
                device,
                memory_properties,
                hal::format::Format::R16Sfloat,
                image_size,
            )?,
        })
    }
}

/// Uniform struct to pass to the shader to make decisions on what to render and
/// where to use defaults. The u32s encode boolean values for use in shaders.
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

    /// Image Size is stored as an i32 because the gfx-hal API expects i32s for
    /// blitting dimensions and this way we save a bunch of casts at the expense
    /// of one upfront cast
    image_size: i32,
    occupied: bool,
}

pub const MIP_LEVELS: u8 = 8;

pub const IMG_SLOT_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    levels: 0..MIP_LEVELS,
    layers: 0..1,
};

impl<B> ImageSlot<B>
where
    B: Backend,
{
    pub fn new(
        device: &B::Device,
        memory_properties: &hal::adapter::MemoryProperties,
        format: hal::format::Format,
        image_size: u32,
    ) -> Result<Self, InitializationError> {
        // Create Image
        let mut image = unsafe {
            device.create_image(
                hal::image::Kind::D2(image_size, image_size, 1, 1),
                MIP_LEVELS,
                format,
                hal::image::Tiling::Optimal,
                hal::image::Usage::SAMPLED | hal::image::Usage::TRANSFER_DST,
                hal::image::ViewCapabilities::empty(),
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Image"))?;

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
            .map_err(|_| InitializationError::Allocation("Render Image"))?;
        unsafe { device.bind_image_memory(&image_memory, 0, &mut image) }.unwrap();

        let image_view = unsafe {
            device.create_image_view(
                &image,
                hal::image::ViewKind::D2,
                format,
                hal::format::Swizzle::NO,
                IMG_SLOT_RANGE.clone(),
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Image View"))?;

        Ok(ImageSlot {
            image: ManuallyDrop::new(image),
            view: ManuallyDrop::new(image_view),
            memory: ManuallyDrop::new(image_memory),
            mip_levels: 8,
            image_size: image_size as i32,
            occupied: false,
        })
    }
}

impl<B> GPURender<B>
where
    B: Backend,
{
    const UNIFORM_BUFFER_SIZE: u64 = 512;
    const FINAL_FORMAT: hal::format::Format = hal::format::Format::Rgba16Sfloat;

    pub fn new(
        gpu: &Arc<Mutex<GPU<B>>>,
        monitor_dimensions: (u32, u32),
        viewport_dimensions: (u32, u32),
        image_size: u32,
        ty: crate::lang::RendererType,
    ) -> Result<Self, InitializationError> {
        log::info!("Obtaining GPU Render Resources");
        let render_target = RenderTarget::new(
            gpu.clone(),
            hal::format::Format::Rgba32Sfloat,
            1,
            false,
            monitor_dimensions,
        )?;
        let accum_target =
            RenderTarget::new(gpu.clone(), Self::FINAL_FORMAT, 1, true, monitor_dimensions)?;
        let environment_maps = EnvironmentMaps::from_file(
            gpu.clone(),
            IRRADIANCE_SIZE,
            SPECMAP_SIZE,
            find_folder::Search::KidsThenParents(3, 5)
                .for_folder("assets")
                .unwrap()
                .join("urban_alley_01_2k.hdr"),
        )
        .unwrap();

        let lock = gpu.lock().unwrap();
        log::debug!("Using render format {:?}", Self::FINAL_FORMAT);

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Command Pool"))?;

        let mut descriptor_pool = unsafe {
            use hal::pso::*;
            lock.device.create_descriptor_pool(
                2,
                &[
                    DescriptorRangeDesc {
                        ty: DescriptorType::Buffer {
                            ty: BufferDescriptorType::Uniform,
                            format: BufferDescriptorFormat::Structured {
                                dynamic_offset: false,
                            },
                        },
                        count: 8,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Sampler,
                        count: 2,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Image {
                            ty: ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 16,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Image {
                            ty: ImageDescriptorType::Storage { read_only: false },
                        },
                        count: 1,
                    },
                ],
                DescriptorPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Descriptor Pool"))?;

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
                        ty: hal::pso::DescriptorType::Buffer {
                            ty: hal::pso::BufferDescriptorType::Uniform,
                            format: hal::pso::BufferDescriptorFormat::Structured {
                                dynamic_offset: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 2,
                        ty: hal::pso::DescriptorType::Buffer {
                            ty: hal::pso::BufferDescriptorType::Uniform,
                            format: hal::pso::BufferDescriptorFormat::Structured {
                                dynamic_offset: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 3,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 4,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 5,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 6,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 7,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 8,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 9,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 10,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::FRAGMENT,
                        immutable_samplers: false,
                    },
                ],
                &[],
            )
        }
        .map_err(|_| {
            InitializationError::ResourceAcquisition("Render Main Descriptor Set Layout")
        })?;

        let main_descriptor_set = unsafe { descriptor_pool.allocate_set(&main_set_layout) }
            .map_err(|_| InitializationError::ResourceAcquisition("Render Descriptor Set"))?;

        let (main_render_pass, main_pipeline, main_pipeline_layout) = Self::make_render_pipeline(
            &lock.device,
            hal::format::Format::Rgba32Sfloat,
            &main_set_layout,
            MAIN_VERTEX_SHADER,
            match ty {
                crate::lang::RendererType::Renderer2D => MAIN_FRAGMENT_SHADER_2D,
                crate::lang::RendererType::Renderer3D => MAIN_FRAGMENT_SHADER_3D,
            },
        )?;

        // Accumulation Buffer Data
        let accum_set_layout = unsafe {
            lock.device.create_descriptor_set_layout(
                &[
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 0,
                        ty: hal::pso::DescriptorType::Sampler,
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::COMPUTE,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 1,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::COMPUTE,
                        immutable_samplers: false,
                    },
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 2,
                        ty: hal::pso::DescriptorType::Image {
                            ty: hal::pso::ImageDescriptorType::Storage { read_only: false },
                        },
                        count: 1,
                        stage_flags: hal::pso::ShaderStageFlags::COMPUTE,
                        immutable_samplers: false,
                    },
                ],
                &[],
            )
        }
        .map_err(|_| {
            InitializationError::ResourceAcquisition("Render Accumulation Descriptor Set Layout")
        })?;

        let accum_descriptor_set = unsafe { descriptor_pool.allocate_set(&accum_set_layout) }
            .map_err(|_| {
                InitializationError::ResourceAcquisition("Render Accumulation Descriptor Set")
            })?;

        let (accum_pipeline, accum_pipeline_layout) =
            Self::make_accum_pipeline(&lock.device, &accum_set_layout, ACCUM_SHADER)?;

        // Rendering setup
        let viewport = hal::pso::Viewport {
            rect: hal::pso::Rect {
                x: 0,
                y: 0,
                w: viewport_dimensions.0 as _,
                h: viewport_dimensions.1 as _,
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
        .map_err(|_| InitializationError::ResourceAcquisition("Render Sampler"))?;

        // Uniforms
        let (uniform_buf, uniform_mem) =
            Self::new_uniform_buffer(&lock.device, &lock.memory_properties)?;
        let (occupancy_buf, occupancy_mem) =
            Self::new_uniform_buffer(&lock.device, &lock.memory_properties)?;

        // Synchronization primitives
        let fence = lock.device.create_fence(true).unwrap();
        let tfence = lock.device.create_fence(false).unwrap();

        // Image slots
        let image_slots = ImageSlots::new(&lock.device, &lock.memory_properties, image_size)?;

        Ok(GPURender {
            gpu: gpu.clone(),
            command_pool: ManuallyDrop::new(command_pool),

            viewport,
            dimensions: hal::window::Extent2D {
                width: monitor_dimensions.0,
                height: monitor_dimensions.1,
            },
            render_target,
            accum_target,
            current_sample: 0,

            view: match ty {
                crate::lang::RendererType::Renderer2D => RenderView::RenderView2D(RenderView2D {
                    resolution: [viewport_dimensions.0 as _, viewport_dimensions.1 as _],
                    ..RenderView2D::default()
                }),
                crate::lang::RendererType::Renderer3D => RenderView::RenderView3D(RenderView3D {
                    resolution: [viewport_dimensions.0 as _, viewport_dimensions.1 as _],
                    ..RenderView3D::default()
                }),
            },

            halton_sampler: HaltonSequence2D::default(),
            descriptor_pool: ManuallyDrop::new(descriptor_pool),
            main_render_pass: ManuallyDrop::new(main_render_pass),
            main_pipeline: ManuallyDrop::new(main_pipeline),
            main_pipeline_layout: ManuallyDrop::new(main_pipeline_layout),
            main_descriptor_set,
            main_descriptor_set_layout: ManuallyDrop::new(main_set_layout),

            accum_pipeline: ManuallyDrop::new(accum_pipeline),
            accum_pipeline_layout: ManuallyDrop::new(accum_pipeline_layout),
            accum_descriptor_set,
            accum_descriptor_set_layout: ManuallyDrop::new(accum_set_layout),

            sampler: ManuallyDrop::new(sampler),

            image_slots,
            environment_maps,
            image_size: 1024,

            occupancy_buffer: ManuallyDrop::new(occupancy_buf),
            occupancy_memory: ManuallyDrop::new(occupancy_mem),
            uniform_buffer: ManuallyDrop::new(uniform_buf),
            uniform_memory: ManuallyDrop::new(uniform_mem),

            complete_fence: ManuallyDrop::new(fence),
            transfer_fence: ManuallyDrop::new(tfence),
        })
    }

    fn new_uniform_buffer(
        device: &B::Device,
        memory_properties: &hal::adapter::MemoryProperties,
    ) -> Result<(B::Buffer, B::Memory), InitializationError> {
        let mut buf = unsafe {
            device.create_buffer(
                Self::UNIFORM_BUFFER_SIZE,
                hal::buffer::Usage::TRANSFER_DST | hal::buffer::Usage::UNIFORM,
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Uniform Buffer"))?;
        let buffer_req = unsafe { device.get_buffer_requirements(&buf) };
        let upload_type = memory_properties
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
        let mem = unsafe { device.allocate_memory(upload_type, Self::UNIFORM_BUFFER_SIZE) }
            .map_err(|_| InitializationError::Allocation("Render Uniform Buffer"))?;
        unsafe { device.bind_buffer_memory(&mem, 0, &mut buf) }
            .map_err(|_| InitializationError::Bind)?;
        Ok((buf, mem))
    }

    #[allow(clippy::type_complexity)]
    fn make_render_pipeline(
        device: &B::Device,
        format: hal::format::Format,
        set_layout: &B::DescriptorSetLayout,
        vertex_shader: &[u8],
        fragment_shader: &[u8],
    ) -> Result<(B::RenderPass, B::GraphicsPipeline, B::PipelineLayout), InitializationError> {
        // Create Render Pass
        let render_pass = {
            let attachment = hal::pass::Attachment {
                format: Some(format),
                samples: 1,
                ops: hal::pass::AttachmentOps::new(
                    hal::pass::AttachmentLoadOp::Load,
                    hal::pass::AttachmentStoreOp::Store,
                ),
                stencil_ops: hal::pass::AttachmentOps::DONT_CARE,
                layouts: hal::image::Layout::ColorAttachmentOptimal
                    ..hal::image::Layout::ShaderReadOnlyOptimal,
            };

            let subpass = hal::pass::SubpassDesc {
                colors: &[(0, hal::image::Layout::ColorAttachmentOptimal)],
                depth_stencil: None,
                inputs: &[],
                resolves: &[],
                preserves: &[],
            };

            unsafe { device.create_render_pass(&[attachment], &[subpass], &[]) }
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Pass"))?;

        // Pipeline
        let pipeline_layout = unsafe {
            device.create_pipeline_layout(
                std::iter::once(set_layout),
                &[(hal::pso::ShaderStageFlags::FRAGMENT, 0..8)],
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Render Pipeline Layout"))?;

        let pipeline = {
            let vs_module = {
                let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(vertex_shader))
                    .map_err(|_| InitializationError::ShaderSPIRV)?;
                unsafe { device.create_shader_module(&loaded_spirv) }
                    .map_err(|_| InitializationError::ShaderModule)?
            };
            let fs_module = {
                let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(fragment_shader))
                    .map_err(|_| InitializationError::ShaderSPIRV)?;
                unsafe { device.create_shader_module(&loaded_spirv) }
                    .map_err(|_| InitializationError::ShaderModule)?
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
                        blend: Some(hal::pso::BlendState::ADD),
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

    fn make_accum_pipeline(
        device: &B::Device,
        set_layout: &B::DescriptorSetLayout,
        accum_shader: &[u8],
    ) -> Result<(B::ComputePipeline, B::PipelineLayout), InitializationError> {
        let pipeline_layout = unsafe {
            device.create_pipeline_layout(
                std::iter::once(set_layout),
                &[(hal::pso::ShaderStageFlags::COMPUTE, 0..4)],
            )
        }
        .map_err(|_| InitializationError::ResourceAcquisition("Accum Pipeline Layout"))?;

        let shader_module = {
            let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(accum_shader))
                .map_err(|_| InitializationError::ShaderSPIRV)?;
            unsafe { device.create_shader_module(&loaded_spirv) }
                .map_err(|_| InitializationError::ShaderModule)?
        };

        let pipeline = unsafe {
            device
                .create_compute_pipeline(
                    &hal::pso::ComputePipelineDesc::new(
                        hal::pso::EntryPoint {
                            entry: "main",
                            module: &shader_module,
                            specialization: hal::pso::Specialization::default(),
                        },
                        &pipeline_layout,
                    ),
                    None,
                )
                .unwrap()
        };

        unsafe {
            device.destroy_shader_module(shader_module);
        }

        Ok((pipeline, pipeline_layout))
    }

    pub fn recreate_image_slots(&mut self, image_size: u32) -> Result<(), InitializationError> {
        let lock = self.gpu.lock().unwrap();
        self.image_size = image_size;
        self.image_slots = ImageSlots::new(&lock.device, &lock.memory_properties, image_size)?;

        Ok(())
    }

    pub fn set_viewport_dimensions(&mut self, width: u32, height: u32) {
        self.viewport = hal::pso::Viewport {
            rect: hal::pso::Rect {
                x: 0,
                y: 0,
                w: width as i16,
                h: height as i16,
            },
            depth: 0.0..1.0,
        };

        match &mut self.view {
            RenderView::RenderView2D(view) => view.resolution = [width as _, height as _],
            RenderView::RenderView3D(view) => view.resolution = [width as _, height as _],
        }
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

    fn build_occupancy(&self) -> SlotOccupancy {
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

    fn fill_uniforms(
        &self,
        device: &B::Device,
        occupancy: &[u8],
        uniforms: &[u8],
    ) -> Result<(), PipelineError> {
        debug_assert!(uniforms.len() <= Self::UNIFORM_BUFFER_SIZE as usize);
        debug_assert!(occupancy.len() <= Self::UNIFORM_BUFFER_SIZE as usize);

        unsafe {
            let mapping = device
                .map_memory(
                    &*self.uniform_memory,
                    hal::memory::Segment {
                        offset: 0,
                        size: Some(Self::UNIFORM_BUFFER_SIZE),
                    },
                )
                .map_err(|_| PipelineError::UniformMapping)?;
            std::ptr::copy_nonoverlapping(uniforms.as_ptr(), mapping, uniforms.len());
            device.unmap_memory(&*self.uniform_memory);
        }

        unsafe {
            let mapping = device
                .map_memory(
                    &*self.occupancy_memory,
                    hal::memory::Segment {
                        offset: 0,
                        size: Some(Self::UNIFORM_BUFFER_SIZE),
                    },
                )
                .map_err(|_| PipelineError::UniformMapping)?;
            std::ptr::copy_nonoverlapping(occupancy.as_ptr(), mapping, occupancy.len());
            device.unmap_memory(&*self.occupancy_memory);
        }
        Ok(())
    }

    pub fn reset_sampling(&mut self) {
        self.current_sample = 0;
        self.halton_sampler = HaltonSequence2D::default();
    }

    pub fn render(&mut self) {
        // Wait on previous fence to make sure the last frame has been rendered.
        self.synchronize_at_fence();

        {
            let mut lock = self.gpu.lock().unwrap();

            let occupancy = self.build_occupancy();
            let uniforms = match &self.view {
                RenderView::RenderView2D(v) => v.as_bytes(),
                RenderView::RenderView3D(v) => v.as_bytes(),
            };
            self.fill_uniforms(&lock.device, occupancy.as_bytes(), uniforms)
                .expect("Error filling uniforms during render");

            let framebuffer = unsafe {
                lock.device
                    .create_framebuffer(
                        &self.main_render_pass,
                        std::iter::once(
                            &self.render_target.image_view().lock().unwrap() as &B::ImageView
                        ),
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
                        descriptors: Some(Descriptor::Buffer(
                            &*self.occupancy_buffer,
                            hal::buffer::SubRange::WHOLE,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 2,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Buffer(
                            &*self.uniform_buffer,
                            hal::buffer::SubRange::WHOLE,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 3,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.displacement.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 4,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.albedo.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 5,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.normal.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 6,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.roughness.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 7,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.image_slots.metallic.view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 8,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            self.environment_maps.irradiance_view(),
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 9,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            self.environment_maps.spec_view(),
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.main_descriptor_set,
                        binding: 10,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            self.environment_maps.brdf_lut_view(),
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.accum_descriptor_set,
                        binding: 0,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Sampler(&*self.sampler)),
                    },
                    DescriptorSetWrite {
                        set: &self.accum_descriptor_set,
                        binding: 1,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.render_target.image_view().lock().unwrap(),
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    DescriptorSetWrite {
                        set: &self.accum_descriptor_set,
                        binding: 2,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Image(
                            &*self.accum_target.image_view().lock().unwrap(),
                            hal::image::Layout::General,
                        )),
                    },
                ]);
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
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.albedo.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.normal.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.roughness.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*self.image_slots.metallic.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                    ],
                );
                cmd_buffer.pipeline_barrier(
                    hal::pso::PipelineStage::TOP_OF_PIPE
                        ..hal::pso::PipelineStage::COLOR_ATTACHMENT_OUTPUT,
                    hal::memory::Dependencies::empty(),
                    std::iter::once(self.render_target.barrier_before()),
                );
                cmd_buffer.bind_graphics_descriptor_sets(
                    &self.main_pipeline_layout,
                    0,
                    std::iter::once(&self.main_descriptor_set),
                    &[],
                );
                cmd_buffer.bind_graphics_pipeline(&self.main_pipeline);

                let sample_offset = self.halton_sampler.next().unwrap();
                cmd_buffer.push_graphics_constants(
                    &self.main_pipeline_layout,
                    hal::pso::ShaderStageFlags::FRAGMENT,
                    0,
                    &[
                        u32::from_ne_bytes(sample_offset.0.to_ne_bytes()),
                        u32::from_ne_bytes(sample_offset.1.to_ne_bytes()),
                    ],
                );
                cmd_buffer.begin_render_pass(
                    &self.main_render_pass,
                    &framebuffer,
                    self.viewport.rect,
                    &[],
                    hal::command::SubpassContents::Inline,
                );
                if self.current_sample == 0 {
                    cmd_buffer.clear_attachments(
                        &[hal::command::AttachmentClear::Color {
                            index: 0,
                            value: hal::command::ClearColor {
                                float32: [0.0, 0.0, 0.0, 0.0],
                            },
                        }],
                        &[hal::pso::ClearRect {
                            rect: self.viewport.rect,
                            layers: 0..1,
                        }],
                    );
                }
                cmd_buffer.draw(0..6, 0..1);
                cmd_buffer.end_render_pass();
                cmd_buffer.pipeline_barrier(
                    hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::COMPUTE_SHADER,
                    hal::memory::Dependencies::empty(),
                    std::iter::once(self.accum_target.barrier_before()),
                );
                cmd_buffer.bind_compute_descriptor_sets(
                    &self.accum_pipeline_layout,
                    0,
                    std::iter::once(&self.accum_descriptor_set),
                    &[],
                );
                cmd_buffer.bind_compute_pipeline(&self.accum_pipeline);
                cmd_buffer.push_compute_constants(
                    &self.accum_pipeline_layout,
                    0,
                    &[u32::from_ne_bytes(
                        ((self.current_sample + 1) as f32).to_ne_bytes(),
                    )],
                );
                cmd_buffer.dispatch([self.viewport.rect.w as u32, self.viewport.rect.h as u32, 1]);
                cmd_buffer.pipeline_barrier(
                    hal::pso::PipelineStage::COMPUTE_SHADER
                        ..hal::pso::PipelineStage::FRAGMENT_SHADER,
                    hal::memory::Dependencies::empty(),
                    &[self.accum_target.barrier_after()],
                );
                cmd_buffer.finish();

                cmd_buffer
            };

            // Submit for render
            unsafe {
                lock.queue_group.queues[0].submit_without_semaphores(
                    std::iter::once(&cmd_buffer),
                    Some(&self.complete_fence),
                );
                lock.device
                    .wait_for_fence(&self.complete_fence, 10_000_000_000)
                    .expect("Failed to wait for fence after render");
            }

            unsafe { lock.device.destroy_framebuffer(framebuffer) };
        }

        self.current_sample += 1;
    }

    pub fn target_view(&self) -> &Arc<Mutex<B::ImageView>> {
        self.accum_target.image_view()
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
    // TODO: Ugly seams on images smaller than the render image
    pub fn transfer_image(
        &mut self,
        source: &B::Image,
        source_layout: hal::image::Layout,
        source_access: hal::image::Access,
        source_size: i32,
        image_use: crate::lang::OutputType,
    ) {
        let image_slot = match image_use {
            crate::lang::OutputType::Displacement => &mut self.image_slots.displacement,
            crate::lang::OutputType::Albedo => &mut self.image_slots.albedo,
            crate::lang::OutputType::Roughness => &mut self.image_slots.roughness,
            crate::lang::OutputType::Normal => &mut self.image_slots.normal,
            crate::lang::OutputType::Metallic => &mut self.image_slots.metallic,
            _ => return,
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
                    x: source_size,
                    y: source_size,
                    z: 1,
                },
                dst_subresource: hal::image::SubresourceLayers {
                    aspects: hal::format::Aspects::COLOR,
                    level,
                    layers: 0..1,
                },
                dst_bounds: hal::image::Offset { x: 0, y: 0, z: 0 }..hal::image::Offset {
                    x: image_slot.image_size >> level,
                    y: image_slot.image_size >> level,
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
                hal::image::Filter::Linear,
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
                        range: IMG_SLOT_RANGE.clone(),
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
    }

    pub fn vacate_image(&mut self, image_use: crate::lang::OutputType) {
        let image_slot = match image_use {
            crate::lang::OutputType::Displacement => &mut self.image_slots.displacement,
            crate::lang::OutputType::Albedo => &mut self.image_slots.albedo,
            crate::lang::OutputType::Roughness => &mut self.image_slots.roughness,
            crate::lang::OutputType::Normal => &mut self.image_slots.normal,
            crate::lang::OutputType::Metallic => &mut self.image_slots.metallic,
            _ => return,
        };

        image_slot.occupied = false;
    }

    pub fn rotate_camera(&mut self, theta: f32, phi: f32) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.phi += phi;
            view.theta += theta;
        }
    }

    pub fn pan_camera(&mut self, x: f32, y: f32) {
        match &mut self.view {
            RenderView::RenderView2D(view) => {
                view.pan[0] += x;
                view.pan[1] += y;
            }
            RenderView::RenderView3D(view) => {
                let point = (view.theta.cos(), view.theta.sin());
                let normal = (point.1, -point.0);

                let delta = (point.0 * y + normal.0 * x, point.1 * y + normal.1 * x);

                view.center[0] += delta.0;
                view.center[2] += delta.1;
            }
        }
    }

    pub fn zoom_camera(&mut self, z: f32) {
        match &mut self.view {
            RenderView::RenderView2D(view) => {
                view.zoom += z;
            }
            RenderView::RenderView3D(view) => {
                view.rad += z;
            }
        }
    }

    pub fn move_light(&mut self, x: f32, y: f32) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.light_pos[0] += x;
            view.light_pos[2] += y;
        }
    }

    pub fn set_channel(&mut self, channel: u32) {
        if let RenderView::RenderView2D(view) = &mut self.view {
            view.channel = channel;
        }
    }

    pub fn set_displacement_amount(&mut self, displacement: f32) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.displacement = displacement;
        }
    }

    pub fn set_texture_scale(&mut self, scale: f32) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.tex_scale = scale;
            view.texel_size = scale / self.image_size as f32;
        }
    }

    pub fn set_light_type(&mut self, light_type: LightType) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.light_type = light_type;
        }
    }

    pub fn set_light_strength(&mut self, strength: f32) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.light_strength = strength;
        }
    }

    pub fn set_fog_strength(&mut self, strength: f32) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.fog_strength = strength;
        }
    }

    pub fn set_environment_strength(&mut self, strength: f32) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.environment_strength = strength;
        }
    }

    pub fn set_shadow(&mut self, shadow: ParameterBool) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.shadow = shadow;
        }
    }

    pub fn set_ao(&mut self, ao: ParameterBool) {
        if let RenderView::RenderView3D(view) = &mut self.view {
            view.ao = ao;
        }
    }

    pub fn load_environment<P: AsRef<std::path::Path>>(&mut self, path: P) {
        let new_env =
            EnvironmentMaps::from_file(self.gpu.clone(), IRRADIANCE_SIZE, SPECMAP_SIZE, path)
                .unwrap();
        self.environment_maps = new_env;
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
                .destroy_buffer(ManuallyDrop::take(&mut self.occupancy_buffer));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.occupancy_memory));
            lock.device
                .destroy_command_pool(ManuallyDrop::take(&mut self.command_pool));
            lock.device
                .destroy_descriptor_pool(ManuallyDrop::take(&mut self.descriptor_pool));
            lock.device
                .destroy_descriptor_set_layout(ManuallyDrop::take(
                    &mut self.main_descriptor_set_layout,
                ));
            lock.device
                .destroy_descriptor_set_layout(ManuallyDrop::take(
                    &mut self.accum_descriptor_set_layout,
                ));
            lock.device
                .destroy_render_pass(ManuallyDrop::take(&mut self.main_render_pass));
            lock.device
                .destroy_graphics_pipeline(ManuallyDrop::take(&mut self.main_pipeline));
            lock.device
                .destroy_pipeline_layout(ManuallyDrop::take(&mut self.main_pipeline_layout));
            lock.device
                .destroy_compute_pipeline(ManuallyDrop::take(&mut self.accum_pipeline));
            lock.device
                .destroy_pipeline_layout(ManuallyDrop::take(&mut self.accum_pipeline_layout));
            lock.device
                .destroy_sampler(ManuallyDrop::take(&mut self.sampler));
            lock.device
                .destroy_fence(ManuallyDrop::take(&mut self.complete_fence));
            lock.device
                .destroy_fence(ManuallyDrop::take(&mut self.transfer_fence));
        }
    }
}
