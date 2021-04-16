use super::RenderTarget;
use crate::gpu::{basic_mem::*, load_shader};
use crate::lang::{ImageType, ObjectType, ShadingMode, ToneMap};
use crate::shader;
use crate::util::HaltonSequence2D;

use gfx_hal as hal;
use gfx_hal::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use strum_macros::*;
use thiserror::Error;
use zerocopy::AsBytes;

use super::{Backend, PipelineError, GPU};

pub mod brdf_lut;
pub mod environment;
pub mod matcap;
pub mod renderer2d;
pub mod sdf3d;

pub use renderer2d::Renderer2D;
pub use sdf3d::RendererSDF3D;

use environment::EnvironmentMaps;
use matcap::Matcap;

static ACCUM_SHADER: &[u8] = shader!("accum");

const IRRADIANCE_SIZE: usize = 32;
const SPECMAP_SIZE: usize = 512;

/// Functions defining a renderer
pub trait Renderer {
    fn vertex_shader() -> &'static [u8];
    fn fragment_shader() -> &'static [u8];
    fn set_resolution(&mut self, w: f32, h: f32);
    fn uniforms(&self) -> &[u8];
    fn serialize(&self) -> Result<Vec<u8>, serde_cbor::Error>;
    fn deserialize(&mut self, data: &[u8]) -> Result<(), serde_cbor::Error>;
}

#[derive(Debug, Error)]
pub enum InitializationError {
    #[error("Failed to initialize shader")]
    ShaderError(#[from] super::ShaderError),
    #[error("Out of memory encountered")]
    OutOfMemory(#[from] hal::device::OutOfMemory),
    #[error("Error during memory allocation")]
    AllocationError(#[from] hal::device::AllocationError),
    #[error("Error during pipeline object allocation")]
    PsoAllocationError(#[from] hal::pso::AllocationError),
    #[error("Failed to build buffer")]
    BufferCreation(#[from] BasicBufferBuilderError),
    #[error("Failed to build image")]
    ImageCreation(#[from] BasicImageBuilderError),
    #[error("Failed to initialize render target")]
    RenderTarget(#[from] super::RenderTargetError),
}

pub struct GPURender<B: Backend, U: Renderer> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: ManuallyDrop<B::CommandPool>,

    // Target Data and Geometry
    viewport: hal::pso::Viewport,
    dimensions: hal::window::Extent2D,
    render_target: RenderTarget<B>,
    accum_target: RenderTarget<B>,
    current_sample: usize,
    tone_map: ToneMap,

    // Uniforms and specific/optional data
    view: U,
    object_type: Option<ObjectType>,
    shading_mode: Option<ShadingMode>,

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
    environment_maps: EnvironmentMaps<B>,
    matcap: Matcap<B>,

    // Synchronization
    complete_fence: ManuallyDrop<B::Fence>,
    transfer_fence: ManuallyDrop<B::Fence>,
}

/// Uses of an image
#[derive(PartialEq, Clone, Copy, Debug, EnumIter)]
pub enum ImageUse {
    Albedo,
    Roughness,
    Normal,
    Displacement,
    Metallic,
    AmbientOcclusion,
    View(ImageType),
}

impl std::convert::TryFrom<crate::lang::OutputType> for ImageUse {
    type Error = &'static str;

    fn try_from(value: crate::lang::OutputType) -> Result<Self, Self::Error> {
        use crate::lang::OutputType;

        match value {
            OutputType::Albedo => Ok(ImageUse::Albedo),
            OutputType::Roughness => Ok(ImageUse::Roughness),
            OutputType::Normal => Ok(ImageUse::Normal),
            OutputType::Displacement => Ok(ImageUse::Displacement),
            OutputType::Metallic => Ok(ImageUse::Metallic),
            OutputType::AmbientOcclusion => Ok(ImageUse::AmbientOcclusion),
            _ => Err("Invalid OutputType for ImageUse"),
        }
    }
}

pub struct ImageSlots<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    albedo: ImageSlot<B>,
    roughness: ImageSlot<B>,
    normal: ImageSlot<B>,
    displacement: ImageSlot<B>,
    metallic: ImageSlot<B>,
    ao: ImageSlot<B>,
    view: ImageSlot<B>,
    view_type: ImageType,
}

impl<B: Backend> ImageSlots<B> {
    pub fn new(gpu: Arc<Mutex<GPU<B>>>, image_size: u32) -> Result<Self, BasicImageBuilderError> {
        let lock = gpu.lock().unwrap();
        let device = &lock.device;
        let memory_properties = &lock.memory_properties;

        Ok(Self {
            gpu: gpu.clone(),
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
            ao: ImageSlot::new(
                device,
                memory_properties,
                hal::format::Format::R16Sfloat,
                image_size,
            )?,
            view: ImageSlot::new(
                device,
                memory_properties,
                hal::format::Format::Rgba16Sfloat,
                image_size,
            )?,
            view_type: ImageType::Grayscale,
        })
    }

    /// Vacate a texture, making it inaccessible for the shader.
    ///
    /// This does not modify any GPU memory, it merely marks the slot as
    /// unoccupied in the occupancy uniforms.
    pub fn vacate(&mut self, image_use: ImageUse) {
        let slot = match image_use {
            ImageUse::Displacement => &mut self.displacement,
            ImageUse::Albedo => &mut self.albedo,
            ImageUse::Roughness => &mut self.roughness,
            ImageUse::Normal => &mut self.normal,
            ImageUse::Metallic => &mut self.metallic,
            ImageUse::AmbientOcclusion => &mut self.ao,
            ImageUse::View(..) => &mut self.view,
        };

        slot.occupied = false;
    }

    /// Build the SlotOccupancy uniform from the stored image slots.
    pub fn occupancy(&self) -> SlotOccupancy {
        fn from_bool(x: bool) -> u32 {
            if x {
                1
            } else {
                0
            }
        }

        SlotOccupancy {
            albedo: from_bool(self.albedo.occupied),
            roughness: from_bool(self.roughness.occupied),
            normal: from_bool(self.normal.occupied),
            displacement: from_bool(self.displacement.occupied),
            metallic: from_bool(self.metallic.occupied),
            ao: from_bool(self.ao.occupied),
            view: from_bool(self.view.occupied),
            view_type: match self.view_type {
                ImageType::Grayscale => 0,
                ImageType::Rgb => 1,
            },
        }
    }
}

impl<B> Drop for ImageSlots<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::trace!("Releasing Image Slots");

        let lock = self.gpu.lock().unwrap();

        fn free_slot<B: Backend>(device: &B::Device, slot: &mut ImageSlot<B>) {
            unsafe {
                device.free_memory(ManuallyDrop::take(&mut slot.memory));
                device.destroy_image_view(ManuallyDrop::take(&mut slot.view));
                device.destroy_image(ManuallyDrop::take(&mut slot.image));
            }
        }

        // Destroy Image Slots
        free_slot(&lock.device, &mut self.albedo);
        free_slot(&lock.device, &mut self.roughness);
        free_slot(&lock.device, &mut self.normal);
        free_slot(&lock.device, &mut self.displacement);
        free_slot(&lock.device, &mut self.metallic);
        free_slot(&lock.device, &mut self.ao);
        free_slot(&lock.device, &mut self.view);
    }
}

/// Uniform struct to pass to the shader to make decisions on what to render and
/// where to use defaults. The u32s encode boolean values for use in shaders.
#[derive(AsBytes)]
#[repr(C)]
pub struct SlotOccupancy {
    albedo: u32,
    roughness: u32,
    normal: u32,
    displacement: u32,
    metallic: u32,
    ao: u32,
    view: u32,
    view_type: u32,
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
    level_count: Some(MIP_LEVELS),
    level_start: 0,
    layer_start: 0,
    layer_count: Some(1),
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
    ) -> Result<Self, BasicImageBuilderError> {
        let (image, image_memory, image_view) =
            BasicImageBuilder::new(&memory_properties.memory_types)
                .size_2d(image_size, image_size)
                .mip_levels(MIP_LEVELS)
                .format(format)
                .usage(hal::image::Usage::SAMPLED | hal::image::Usage::TRANSFER_DST)
                .tiling(hal::image::Tiling::Optimal)
                .memory_type(hal::memory::Properties::DEVICE_LOCAL)
                .unwrap()
                .build::<B>(&device)?;

        Ok(ImageSlot {
            image: ManuallyDrop::new(image),
            view: ManuallyDrop::new(image_view),
            memory: ManuallyDrop::new(image_memory),
            mip_levels: MIP_LEVELS,
            image_size: image_size as i32,
            occupied: false,
        })
    }
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("Failed failling uniforms during render")]
    UniformFill,
    #[error("Render fence timed out")]
    FenceTimeout,
}

#[derive(Debug, Serialize, Deserialize)]
struct RendererSettings {
    view_data: Vec<u8>,
    tone_map: ToneMap,
    object_type: Option<ObjectType>,
    hdri_path: std::path::PathBuf,
}

impl<B, U> GPURender<B, U>
where
    B: Backend,
    U: Renderer,
{
    const UNIFORM_BUFFER_SIZE: u64 = 512;
    const FINAL_FORMAT: hal::format::Format = hal::format::Format::Rgba16Sfloat;

    /// Create a new renderer
    fn new(
        gpu: &Arc<Mutex<GPU<B>>>,
        monitor_dimensions: (u32, u32),
        viewport_dimensions: (u32, u32),
        view: U,
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
                .join("artist_workshop_2k.hdr"),
        )
        .unwrap();
        let matcap = Matcap::from_file(
            gpu.clone(),
            find_folder::Search::KidsThenParents(3, 5)
                .for_folder("assets")
                .unwrap()
                .join("matcap.png"),
        )
        .unwrap();

        let lock = gpu.lock().unwrap();
        log::debug!("Using render format {:?}", Self::FINAL_FORMAT);

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::TRANSIENT,
            )
        }?;

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
        }?;

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
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 11,
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
                        binding: 12,
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
                        binding: 13,
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
        }?;

        let main_descriptor_set = unsafe { descriptor_pool.allocate_set(&main_set_layout) }?;

        let (main_render_pass, main_pipeline, main_pipeline_layout) = Self::make_render_pipeline(
            &lock.device,
            hal::format::Format::Rgba32Sfloat,
            &main_set_layout,
            ObjectType::Cube,
            ShadingMode::Pbr,
            U::vertex_shader(),
            U::fragment_shader(),
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
        }?;

        let accum_descriptor_set = unsafe { descriptor_pool.allocate_set(&accum_set_layout) }?;

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
        }?;

        // Uniforms
        let mut buffer_builder = BasicBufferBuilder::new(&lock.memory_properties.memory_types);
        buffer_builder
            .bytes(Self::UNIFORM_BUFFER_SIZE)
            .usage(hal::buffer::Usage::TRANSFER_DST | hal::buffer::Usage::UNIFORM);

        // Pick memory type for buffer builder for AMD/Nvidia
        if buffer_builder
            .memory_type(
                hal::memory::Properties::CPU_VISIBLE | hal::memory::Properties::DEVICE_LOCAL,
            )
            .is_none()
        {
            buffer_builder
                .memory_type(
                    hal::memory::Properties::CPU_VISIBLE | hal::memory::Properties::COHERENT,
                )
                .expect("Failed to find appropriate memory type for uniforms");
        }

        let (uniform_buf, uniform_mem) = buffer_builder.build::<B>(&lock.device)?;
        let (occupancy_buf, occupancy_mem) = buffer_builder.build::<B>(&lock.device)?;

        // Synchronization primitives
        let fence = lock.device.create_fence(true).unwrap();
        let tfence = lock.device.create_fence(false).unwrap();

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
            tone_map: ToneMap::Reinhard,

            view,
            object_type: None,
            shading_mode: None,

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

            environment_maps,
            matcap,

            occupancy_buffer: ManuallyDrop::new(occupancy_buf),
            occupancy_memory: ManuallyDrop::new(occupancy_mem),
            uniform_buffer: ManuallyDrop::new(uniform_buf),
            uniform_memory: ManuallyDrop::new(uniform_mem),

            complete_fence: ManuallyDrop::new(fence),
            transfer_fence: ManuallyDrop::new(tfence),
        })
    }

    pub fn object_type(&self) -> Option<ObjectType> {
        self.object_type
    }

    pub fn serialize_settings(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        serde_cbor::ser::to_vec(&RendererSettings {
            view_data: self.view.serialize()?,
            tone_map: self.tone_map,
            object_type: self.object_type,
            hdri_path: self.environment_maps.path().clone(),
        })
    }

    pub fn deserialize_settings(&mut self, data: &[u8]) -> Result<(), serde_cbor::Error> {
        let settings: RendererSettings = serde_cbor::de::from_slice(data)?;
        self.view.deserialize(&settings.view_data)?;
        self.tone_map = settings.tone_map;
        self.object_type = settings.object_type;
        self.load_environment(&settings.hdri_path)
            .expect("Failed to load hdri");
        Ok(())
    }

    /// Create the render pipeline for this renderer
    #[allow(clippy::type_complexity)]
    fn make_render_pipeline(
        device: &B::Device,
        format: hal::format::Format,
        set_layout: &B::DescriptorSetLayout,
        object_type: ObjectType,
        shading_mode: ShadingMode,
        vertex_shader: &'static [u8],
        fragment_shader: &'static [u8],
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
        }?;

        // Pipeline
        let pipeline_layout = unsafe {
            device.create_pipeline_layout(
                std::iter::once(set_layout),
                &[(hal::pso::ShaderStageFlags::FRAGMENT, 0..8)],
            )
        }?;

        let pipeline = {
            let vs_module = load_shader::<B>(device, vertex_shader)?;
            let fs_module = load_shader::<B>(device, fragment_shader)?;

            let pipeline = {
                let subpass = hal::pass::Subpass {
                    index: 0,
                    main_pass: &render_pass,
                };

                let mut pipeline_desc = hal::pso::GraphicsPipelineDesc::new(
                    hal::pso::PrimitiveAssemblerDesc::Vertex {
                        buffers: &[],
                        attributes: &[],
                        input_assembler: hal::pso::InputAssemblerDesc {
                            primitive: hal::pso::Primitive::TriangleList,
                            with_adjacency: false,
                            restart_index: None,
                        },
                        vertex: hal::pso::EntryPoint {
                            entry: "main",
                            module: &vs_module,
                            specialization: hal::pso::Specialization::default(),
                        },
                        tessellation: None,
                        geometry: None,
                    },
                    hal::pso::Rasterizer::FILL,
                    Some(hal::pso::EntryPoint {
                        entry: "main",
                        module: &fs_module,
                        specialization: hal::spec_const_list![
                            object_type as u32,
                            shading_mode as u32
                        ],
                    }),
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

    /// Create the accumulator compute pipeline for this renderer
    fn make_accum_pipeline(
        device: &B::Device,
        set_layout: &B::DescriptorSetLayout,
        accum_shader: &'static [u8],
    ) -> Result<(B::ComputePipeline, B::PipelineLayout), InitializationError> {
        let pipeline_layout = unsafe {
            device.create_pipeline_layout(
                std::iter::once(set_layout),
                &[(hal::pso::ShaderStageFlags::COMPUTE, 0..8)],
            )
        }?;

        let shader_module = load_shader::<B>(device, accum_shader)?;

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

    /// Set the viewport dimensions.
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

        self.view.set_resolution(width as _, height as _);
    }

    fn synchronize_at_fence(&self) -> Result<(), RenderError> {
        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .wait_for_fence(&self.complete_fence, 10_000_000_000)
                .map_err(|_| RenderError::FenceTimeout)?;
            lock.device
                .reset_fence(&self.complete_fence)
                .expect("Failed to reset render fence");
        }

        Ok(())
    }

    /// Fill the uniform buffers
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

    /// Reset the sampling process.
    pub fn reset_sampling(&mut self) {
        self.current_sample = 0;
        self.halton_sampler = HaltonSequence2D::default();
    }

    /// Render a single frame
    pub fn render(&mut self, image_slots: &ImageSlots<B>) -> Result<(), RenderError> {
        // Wait on previous fence to make sure the last frame has been rendered.
        self.synchronize_at_fence()?;

        {
            let lock = self.gpu.lock().unwrap();

            let occupancy = image_slots.occupancy();
            let uniforms = self.view.uniforms();
            self.fill_uniforms(&lock.device, occupancy.as_bytes(), uniforms)
                .map_err(|_| RenderError::UniformFill)?;

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
                lock.device.write_descriptor_sets(
                    vec![
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
                                &*image_slots.displacement.view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 4,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                &*image_slots.albedo.view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 5,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                &*image_slots.normal.view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 6,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                &*image_slots.roughness.view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 7,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                &*image_slots.metallic.view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 8,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                &*image_slots.ao.view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 9,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                &*image_slots.view.view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 10,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                self.environment_maps.irradiance_view(),
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 11,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                self.environment_maps.spec_view(),
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 12,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                self.environment_maps.brdf_lut_view(),
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        DescriptorSetWrite {
                            set: &self.main_descriptor_set,
                            binding: 13,
                            array_offset: 0,
                            descriptors: Some(Descriptor::Image(
                                self.matcap.matcap_view(),
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
                    ]
                    .into_iter(),
                );
            }

            // Drop lock here to free the device for other threads while building command buffer
            drop(lock);

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
                            target: &*image_slots.displacement.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*image_slots.albedo.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*image_slots.normal.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*image_slots.roughness.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*image_slots.metallic.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*image_slots.ao.image,
                            families: None,
                            range: IMG_SLOT_RANGE.clone(),
                        },
                        hal::memory::Barrier::Image {
                            states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                                ..(
                                    hal::image::Access::SHADER_READ,
                                    hal::image::Layout::ShaderReadOnlyOptimal,
                                ),
                            target: &*image_slots.view.image,
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
                    &[
                        u32::from_ne_bytes(((self.current_sample + 1) as f32).to_ne_bytes()),
                        self.tone_map as u32,
                    ],
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

            // Reacquire lock for submission
            let mut lock = self.gpu.lock().unwrap();

            // Submit for render
            unsafe {
                lock.queue_group.queues[0].submit_without_semaphores(
                    std::iter::once(&cmd_buffer),
                    Some(&self.complete_fence),
                );
                lock.device
                    .wait_for_fence(&self.complete_fence, 10_000_000_000)
                    .map_err(|_| RenderError::FenceTimeout)?;
            }

            unsafe {
                lock.device.destroy_framebuffer(framebuffer);
                self.command_pool.free(Some(cmd_buffer));
            };
        }

        self.current_sample += 1;

        Ok(())
    }

    /// Obtain an image view for the render target
    pub fn target_view(&self) -> &Arc<Mutex<B::ImageView>> {
        self.accum_target.image_view()
    }

    /// Transfer an external (usually compute) image to an image slot in the
    /// ImageSlots struct passed in. This allows hijacking the command buffers
    /// of this renderer to fulfil this task on externally held images.
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
        image_slots: &mut ImageSlots<B>,
        source: &B::Image,
        source_layout: hal::image::Layout,
        source_access: hal::image::Access,
        source_size: i32,
        image_use: ImageUse,
    ) {
        let image_slot = match image_use {
            ImageUse::Displacement => &mut image_slots.displacement,
            ImageUse::Albedo => &mut image_slots.albedo,
            ImageUse::Roughness => &mut image_slots.roughness,
            ImageUse::Normal => &mut image_slots.normal,
            ImageUse::Metallic => &mut image_slots.metallic,
            ImageUse::AmbientOcclusion => &mut image_slots.ao,
            ImageUse::View(..) => &mut image_slots.view,
        };

        image_slot.occupied = true;

        match image_use {
            ImageUse::View(ty) => {
                image_slots.view_type = ty;
            }
            _ => {}
        }

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
                            level_count: Some(image_slot.mip_levels),
                            ..Default::default()
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
                (0..image_slot.mip_levels).map(|level| hal::command::ImageBlit {
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
                }),
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

    /// Load a new environment from a HDRi file.
    pub fn load_environment<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), environment::EnvironmentError> {
        let new_env =
            EnvironmentMaps::from_file(self.gpu.clone(), IRRADIANCE_SIZE, SPECMAP_SIZE, path)?;
        self.environment_maps = new_env;
        Ok(())
    }

    /// Load a new matcap from a file.
    pub fn load_matcap<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), matcap::MatcapError> {
        let new_matcap = Matcap::from_file(self.gpu.clone(), path)?;
        self.matcap = new_matcap;
        Ok(())
    }

    pub fn set_tone_map(&mut self, tone_map: ToneMap) {
        self.tone_map = tone_map;
    }
}

impl<B, U> Drop for GPURender<B, U>
where
    B: Backend,
    U: Renderer,
{
    fn drop(&mut self) {
        // Finish all rendering before destruction of resources
        self.synchronize_at_fence()
            .expect("Failed to synchronize with fence at drop time");

        log::info!("Releasing GPU Render resources");

        let lock = self.gpu.lock().unwrap();

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
