use super::{Backend, GPU};
use crate::gpu::basic_mem::*;
use gfx_hal as hal;
use gfx_hal::prelude::*;
use image::hdr;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use thiserror::Error;

use super::brdf_lut::*;

static IRRADIANCE_SHADER: &[u8] = include_bytes!("../../../shaders/irradiance.spv");
static PREFILTER_SHADER: &[u8] = include_bytes!("../../../shaders/filter_env.spv");

/// Struct to hold all required information for dealing with environment maps
/// for PBR shading
///
/// It holds three images
/// 1. Irradiance Map for diffuse IBL
/// 2. Spec Map for specular IBL
/// 3. Preconvolved BRDF LUT, also for specular IBL
pub struct EnvironmentMaps<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,

    irradiance_image: ManuallyDrop<B::Image>,
    irradiance_view: ManuallyDrop<B::ImageView>,
    irradiance_memory: ManuallyDrop<B::Memory>,

    spec_image: ManuallyDrop<B::Image>,
    spec_view: ManuallyDrop<B::ImageView>,
    spec_memory: ManuallyDrop<B::Memory>,

    brdf_lut_image: ManuallyDrop<B::Image>,
    brdf_lut_view: ManuallyDrop<B::ImageView>,
    brdf_lut_memory: ManuallyDrop<B::Memory>,
}

const CUBE_COLOR_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    levels: 0..1,
    layers: 0..6,
};

const MIP_LEVELS: u8 = 6;

const CUBE_MIP_COLOR_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    levels: 0..MIP_LEVELS,
    layers: 0..6,
};

#[derive(Debug, Error)]
pub enum EnvironmentError {
    #[error("Failed to initialize GPU image")]
    ImageError(#[from] BasicImageBuilderError),
    #[error("Failed to initialize GPU buffer")]
    BufferError(#[from] BasicBufferBuilderError),
    #[error("HDRi IO failed")]
    HDRiIOFailure,
    #[error("GPU Out of Memory")]
    OutOfMemory(#[from] hal::device::OutOfMemory),
    #[error("Failed to create compute pipeline for convolution")]
    Pipeline(#[from] hal::pso::CreationError),
    #[error("Failed to allocate necessary resource for pipeline")]
    DeviceAllocation(#[from] hal::device::AllocationError),
    #[error("Failed to allocate necessary resource for pipeline")]
    PipelineAllocation(#[from] hal::pso::AllocationError),
    #[error("Failed to create MIP view")]
    ImageViewCreation(#[from] hal::image::ViewCreationError),
}

impl<B> EnvironmentMaps<B>
where
    B: Backend,
{
    const FORMAT: hal::format::Format = hal::format::Format::Rgba32Sfloat;
    const BRDF_FORMAT: hal::format::Format = hal::format::Format::Rg32Sfloat;

    /// Initialize GPU side structures for environment map without data, given a
    /// cubemap size.
    fn init(
        gpu: Arc<Mutex<GPU<B>>>,
        irradiance_size: usize,
        spec_size: usize,
    ) -> Result<Self, EnvironmentError> {
        let lock = gpu.lock().unwrap();

        // Irradiance cube map
        let (irradiance_image, irradiance_memory, irradiance_view) =
            BasicImageBuilder::new(&lock.memory_properties.memory_types)
                .size_cube(irradiance_size as u32)
                .format(Self::FORMAT)
                .usage(hal::image::Usage::SAMPLED | hal::image::Usage::STORAGE)
                .memory_type(hal::memory::Properties::DEVICE_LOCAL)
                .unwrap()
                .build::<B>(&lock.device)?;

        // Pre-filtered environment map
        let (spec_image, spec_memory, spec_view) =
            BasicImageBuilder::new(&lock.memory_properties.memory_types)
                .size_cube(spec_size as u32)
                .mip_levels(MIP_LEVELS)
                .format(Self::FORMAT)
                .usage(hal::image::Usage::SAMPLED | hal::image::Usage::STORAGE)
                .memory_type(hal::memory::Properties::DEVICE_LOCAL)
                .unwrap()
                .build::<B>(&lock.device)?;

        // Lookup table
        let (brdf_lut_image, brdf_lut_memory, brdf_lut_view) =
            BasicImageBuilder::new(&lock.memory_properties.memory_types)
                .size_2d(64, 64)
                .format(Self::BRDF_FORMAT)
                .usage(hal::image::Usage::SAMPLED | hal::image::Usage::TRANSFER_DST)
                .memory_type(hal::memory::Properties::DEVICE_LOCAL)
                .unwrap()
                .build::<B>(&lock.device)?;

        drop(lock);

        Ok(Self {
            gpu,
            irradiance_image: ManuallyDrop::new(irradiance_image),
            irradiance_memory: ManuallyDrop::new(irradiance_memory),
            irradiance_view: ManuallyDrop::new(irradiance_view),
            spec_image: ManuallyDrop::new(spec_image),
            spec_memory: ManuallyDrop::new(spec_memory),
            spec_view: ManuallyDrop::new(spec_view),
            brdf_lut_image: ManuallyDrop::new(brdf_lut_image),
            brdf_lut_memory: ManuallyDrop::new(brdf_lut_memory),
            brdf_lut_view: ManuallyDrop::new(brdf_lut_view),
        })
    }

    /// Create environment maps from a path to a HDRi file. Expects .hdr,
    /// equirectangular mapping!
    pub fn from_file<P: AsRef<Path>>(
        gpu: Arc<Mutex<GPU<B>>>,
        irradiance_size: usize,
        spec_size: usize,
        path: P,
    ) -> Result<Self, EnvironmentError> {
        use std::fs::File;
        use std::io::BufReader;

        // Initialize
        let env_maps = Self::init(gpu, irradiance_size, spec_size)?;

        // Read data from file
        let start_io = Instant::now();

        let reader = BufReader::new(File::open(path).map_err(|_| EnvironmentError::HDRiIOFailure)?);
        let decoder = hdr::HdrDecoder::new(reader).map_err(|_| EnvironmentError::HDRiIOFailure)?;
        let metadata = decoder.metadata();
        let raw_hdri: Vec<image::Rgba<f32>> = decoder
            .read_image_hdr()
            .map_err(|_| EnvironmentError::HDRiIOFailure)?
            .iter()
            .map(|rgb| image::Rgba([rgb[0], rgb[1], rgb[2], 1.0]))
            .collect();

        let raw_hdri_u8 = unsafe {
            std::slice::from_raw_parts(raw_hdri.as_ptr() as *const u8, raw_hdri.len() * 4)
        };

        log::debug!(
            "Read HDRi from disk in {}ms",
            start_io.elapsed().as_millis()
        );

        // Upload raw HDRi to staging buffer
        let mut lock = env_maps.gpu.lock().unwrap();

        let (staging_buffer, staging_memory) =
            BasicBufferBuilder::new(&lock.memory_properties.memory_types)
                .bytes((metadata.width * metadata.height * 4 * 4).max(BRDF_LUT_BYTES as u32) as u64)
                .usage(hal::buffer::Usage::TRANSFER_SRC)
                .data(raw_hdri_u8)
                .memory_type(hal::memory::Properties::CPU_VISIBLE)
                .unwrap()
                .build::<B>(&lock.device)?;

        // Move HDRi to device only memory for the compute shader
        let (equirect_image, equirect_memory, equirect_view) =
            BasicImageBuilder::new(&lock.memory_properties.memory_types)
                .size_2d(metadata.width, metadata.height)
                .format(Self::FORMAT)
                .usage(hal::image::Usage::SAMPLED | hal::image::Usage::TRANSFER_DST)
                .memory_type(hal::memory::Properties::DEVICE_LOCAL)
                .unwrap()
                .build::<B>(&lock.device)?;

        let mut command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::TRANSIENT,
            )
        }?;

        let fence = lock.device.create_fence(false).unwrap();

        unsafe {
            let mut command_buffer = command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[hal::memory::Barrier::Image {
                    states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                        ..(
                            hal::image::Access::TRANSFER_WRITE,
                            hal::image::Layout::TransferDstOptimal,
                        ),
                    target: &equirect_image,
                    families: None,
                    range: super::super::COLOR_RANGE,
                }],
            );
            command_buffer.copy_buffer_to_image(
                &staging_buffer,
                &equirect_image,
                hal::image::Layout::TransferDstOptimal,
                Some(hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: metadata.width,
                    buffer_height: metadata.height,
                    image_offset: hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: hal::image::Extent {
                        width: metadata.width,
                        height: metadata.height,
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
                &[hal::memory::Barrier::Image {
                    states: (
                        hal::image::Access::TRANSFER_WRITE,
                        hal::image::Layout::TransferDstOptimal,
                    )
                        ..(
                            hal::image::Access::SHADER_READ,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        ),
                    target: &equirect_image,
                    families: None,
                    range: super::super::COLOR_RANGE,
                }],
            );
            command_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&command_buffer), Some(&fence));
            lock.device.wait_for_fence(&fence, !0).unwrap();
            command_pool.free(Some(command_buffer));
        }

        unsafe {
            lock.device.reset_fence(&fence).unwrap();
        }

        // Upload BRDF LUT using same staging buffer
        unsafe {
            let mapping = lock
                .device
                .map_memory(
                    &staging_memory,
                    hal::memory::Segment {
                        offset: 0,
                        size: Some(BRDF_LUT_BYTES as u64),
                    },
                )
                .unwrap();
            let u8s: &[u8] =
                std::slice::from_raw_parts(BRDF_LUT.as_ptr() as *const u8, BRDF_LUT_BYTES);
            std::ptr::copy_nonoverlapping(u8s.as_ptr(), mapping, BRDF_LUT_BYTES);
            lock.device.unmap_memory(&staging_memory);
        }

        unsafe {
            let mut command_buffer = command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::TRANSFER,
                hal::memory::Dependencies::empty(),
                &[hal::memory::Barrier::Image {
                    states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                        ..(
                            hal::image::Access::TRANSFER_WRITE,
                            hal::image::Layout::TransferDstOptimal,
                        ),
                    target: &*env_maps.brdf_lut_image,
                    families: None,
                    range: super::super::COLOR_RANGE,
                }],
            );
            command_buffer.copy_buffer_to_image(
                &staging_buffer,
                &*env_maps.brdf_lut_image,
                hal::image::Layout::TransferDstOptimal,
                Some(hal::command::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_width: 64,
                    buffer_height: 64,
                    image_offset: hal::image::Offset { x: 0, y: 0, z: 0 },
                    image_extent: hal::image::Extent {
                        width: 64,
                        height: 64,
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
                &[hal::memory::Barrier::Image {
                    states: (
                        hal::image::Access::TRANSFER_WRITE,
                        hal::image::Layout::TransferDstOptimal,
                    )
                        ..(
                            hal::image::Access::SHADER_READ,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        ),
                    target: &*env_maps.brdf_lut_image,
                    families: None,
                    range: super::super::COLOR_RANGE,
                }],
            );
            command_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&command_buffer), Some(&fence));
            lock.device.wait_for_fence(&fence, !0).unwrap();
            command_pool.free(Some(command_buffer));
        }

        unsafe {
            lock.device.reset_fence(&fence).unwrap();
            lock.device.destroy_buffer(staging_buffer);
            lock.device.free_memory(staging_memory);
        }

        // Prepare compute pipeline
        let mut descriptor_pool = unsafe {
            use hal::pso::*;
            // Enough descriptors to have one per convolution task
            // i.e. MIP_LEVELS + 1 for the specular
            lock.device.create_descriptor_pool(
                MIP_LEVELS as usize + 1,
                &[
                    DescriptorRangeDesc {
                        ty: DescriptorType::Image {
                            ty: ImageDescriptorType::Sampled {
                                with_sampler: false,
                            },
                        },
                        count: MIP_LEVELS as usize + 1,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Image {
                            ty: ImageDescriptorType::Storage { read_only: false },
                        },
                        count: MIP_LEVELS as usize + 1,
                    },
                    DescriptorRangeDesc {
                        ty: DescriptorType::Sampler,
                        count: MIP_LEVELS as usize + 1,
                    },
                ],
                DescriptorPoolCreateFlags::empty(),
            )
        }?;

        let set_layout = unsafe {
            lock.device.create_descriptor_set_layout(
                &[
                    hal::pso::DescriptorSetLayoutBinding {
                        binding: 0,
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
                        binding: 1,
                        ty: hal::pso::DescriptorType::Sampler,
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

        let pipeline_layout = unsafe {
            lock.device.create_pipeline_layout(
                Some(&set_layout),
                // Push constant for roughness
                &[(hal::pso::ShaderStageFlags::COMPUTE, 0..4)],
            )
        }?;

        let irradiance_module = {
            let loaded_spirv =
                hal::pso::read_spirv(std::io::Cursor::new(IRRADIANCE_SHADER)).unwrap();
            unsafe { lock.device.create_shader_module(&loaded_spirv) }.unwrap()
        };

        let prefilter_module = {
            let loaded_spirv =
                hal::pso::read_spirv(std::io::Cursor::new(PREFILTER_SHADER)).unwrap();
            unsafe { lock.device.create_shader_module(&loaded_spirv) }.unwrap()
        };

        let irradiance_pipeline = unsafe {
            lock.device.create_compute_pipeline(
                &hal::pso::ComputePipelineDesc::new(
                    hal::pso::EntryPoint {
                        entry: "main",
                        module: &irradiance_module,
                        specialization: hal::pso::Specialization::default(),
                    },
                    &pipeline_layout,
                ),
                None,
            )
        }?;

        let prefilter_pipeline = unsafe {
            lock.device.create_compute_pipeline(
                &hal::pso::ComputePipelineDesc::new(
                    hal::pso::EntryPoint {
                        entry: "main",
                        module: &prefilter_module,
                        specialization: hal::pso::Specialization::default(),
                    },
                    &pipeline_layout,
                ),
                None,
            )
        }?;

        let sampler = unsafe {
            lock.device.create_sampler(&hal::image::SamplerDesc::new(
                hal::image::Filter::Linear,
                hal::image::WrapMode::Tile,
            ))
        }?;

        log::debug!("Starting convolution of HDRi");
        let start_conv = Instant::now();

        let irradiance_descriptors = unsafe { descriptor_pool.allocate_set(&set_layout) }?;

        unsafe {
            lock.device.write_descriptor_sets(
                vec![
                    hal::pso::DescriptorSetWrite {
                        set: &irradiance_descriptors,
                        binding: 0,
                        array_offset: 0,
                        descriptors: Some(hal::pso::Descriptor::Image(
                            &equirect_view,
                            hal::image::Layout::ShaderReadOnlyOptimal,
                        )),
                    },
                    hal::pso::DescriptorSetWrite {
                        set: &irradiance_descriptors,
                        binding: 1,
                        array_offset: 0,
                        descriptors: Some(hal::pso::Descriptor::Sampler(&sampler)),
                    },
                    hal::pso::DescriptorSetWrite {
                        set: &irradiance_descriptors,
                        binding: 2,
                        array_offset: 0,
                        descriptors: Some(hal::pso::Descriptor::Image(
                            &*env_maps.irradiance_view,
                            hal::image::Layout::General,
                        )),
                    },
                ]
                .into_iter(),
            );
        }

        let mut spec_mip_views = Vec::with_capacity(MIP_LEVELS as usize);

        for level in 0..MIP_LEVELS {
            let view = unsafe {
                lock.device.create_image_view(
                    &env_maps.spec_image,
                    hal::image::ViewKind::Cube,
                    Self::FORMAT,
                    hal::format::Swizzle::NO,
                    hal::image::SubresourceRange {
                        aspects: hal::format::Aspects::COLOR,
                        levels: level..level + 1,
                        layers: 0..6,
                    },
                )
            }?;
            spec_mip_views.push(view);
        }

        let mut prefilter_descriptors = Vec::with_capacity(MIP_LEVELS as usize);

        for level in 0..MIP_LEVELS {
            let descr = unsafe { descriptor_pool.allocate_set(&set_layout) }?;

            unsafe {
                lock.device.write_descriptor_sets(
                    vec![
                        hal::pso::DescriptorSetWrite {
                            set: &descr,
                            binding: 0,
                            array_offset: 0,
                            descriptors: Some(hal::pso::Descriptor::Image(
                                &equirect_view,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            )),
                        },
                        hal::pso::DescriptorSetWrite {
                            set: &descr,
                            binding: 1,
                            array_offset: 0,
                            descriptors: Some(hal::pso::Descriptor::Sampler(&sampler)),
                        },
                        hal::pso::DescriptorSetWrite {
                            set: &descr,
                            binding: 2,
                            array_offset: 0,
                            descriptors: Some(hal::pso::Descriptor::Image(
                                &spec_mip_views[level as usize],
                                hal::image::Layout::General,
                            )),
                        },
                    ]
                    .into_iter(),
                );
            }

            prefilter_descriptors.push(descr);
        }

        unsafe {
            let mut command_buffer = command_pool.allocate_one(hal::command::Level::Primary);
            command_buffer.begin_primary(hal::command::CommandBufferFlags::ONE_TIME_SUBMIT);
            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::TOP_OF_PIPE..hal::pso::PipelineStage::COMPUTE_SHADER,
                hal::memory::Dependencies::empty(),
                &[
                    hal::memory::Barrier::Image {
                        states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                            ..(
                                hal::image::Access::SHADER_WRITE,
                                hal::image::Layout::General,
                            ),
                        target: &*env_maps.irradiance_image,
                        families: None,
                        range: CUBE_COLOR_RANGE,
                    },
                    hal::memory::Barrier::Image {
                        states: (hal::image::Access::empty(), hal::image::Layout::Undefined)
                            ..(
                                hal::image::Access::SHADER_WRITE,
                                hal::image::Layout::General,
                            ),
                        target: &*env_maps.spec_image,
                        families: None,
                        range: CUBE_MIP_COLOR_RANGE,
                    },
                ],
            );

            // Convolve irradiance map
            command_buffer.bind_compute_pipeline(&irradiance_pipeline);
            command_buffer.bind_compute_descriptor_sets(
                &pipeline_layout,
                0,
                Some(&irradiance_descriptors),
                &[],
            );
            command_buffer.dispatch([irradiance_size as u32 / 8, irradiance_size as u32 / 8, 6]);

            // Pre-filter environment map
            command_buffer.bind_compute_pipeline(&prefilter_pipeline);
            for (level, descriptors) in prefilter_descriptors
                .iter()
                .enumerate()
                .take(MIP_LEVELS as usize)
            {
                let roughness = level as f32 / MIP_LEVELS as f32;
                command_buffer.bind_compute_descriptor_sets(
                    &pipeline_layout,
                    0,
                    Some(descriptors),
                    &[],
                );
                command_buffer.push_compute_constants(
                    &pipeline_layout,
                    0,
                    &[u32::from_ne_bytes(roughness.to_ne_bytes())],
                );
                command_buffer.dispatch([
                    (spec_size as u32 >> level) / 8,
                    (spec_size as u32 >> level) / 8,
                    6,
                ]);
            }

            command_buffer.pipeline_barrier(
                hal::pso::PipelineStage::COMPUTE_SHADER..hal::pso::PipelineStage::FRAGMENT_SHADER,
                hal::memory::Dependencies::empty(),
                &[
                    hal::memory::Barrier::Image {
                        states: (
                            hal::image::Access::SHADER_WRITE,
                            hal::image::Layout::General,
                        )
                            ..(
                                hal::image::Access::SHADER_READ,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            ),
                        target: &*env_maps.irradiance_image,
                        families: None,
                        range: CUBE_COLOR_RANGE,
                    },
                    hal::memory::Barrier::Image {
                        states: (
                            hal::image::Access::SHADER_WRITE,
                            hal::image::Layout::General,
                        )
                            ..(
                                hal::image::Access::SHADER_READ,
                                hal::image::Layout::ShaderReadOnlyOptimal,
                            ),
                        target: &*env_maps.spec_image,
                        families: None,
                        range: CUBE_MIP_COLOR_RANGE,
                    },
                ],
            );
            command_buffer.finish();

            lock.queue_group.queues[0]
                .submit_without_semaphores(Some(&command_buffer), Some(&fence));

            lock.device.wait_for_fence(&fence, !0).unwrap();
            command_pool.free(Some(command_buffer));
        };

        log::debug!(
            "Convoluted HDRi data in {}ms",
            start_conv.elapsed().as_millis()
        );

        // Clean up compute pipeline and equirectangular image
        unsafe {
            lock.device.destroy_command_pool(command_pool);
            lock.device.destroy_sampler(sampler);
            lock.device.destroy_descriptor_pool(descriptor_pool);
            lock.device.destroy_descriptor_set_layout(set_layout);
            lock.device.destroy_shader_module(irradiance_module);
            lock.device.destroy_fence(fence);
            lock.device.destroy_pipeline_layout(pipeline_layout);
            lock.device.destroy_compute_pipeline(irradiance_pipeline);
            lock.device.destroy_compute_pipeline(prefilter_pipeline);
            lock.device.destroy_image(equirect_image);
            lock.device.destroy_image_view(equirect_view);
            lock.device.free_memory(equirect_memory);
            for view in spec_mip_views.drain(0..) {
                lock.device.destroy_image_view(view);
            }
        }

        drop(lock);

        Ok(env_maps)
    }

    /// Obtain view on the irradiance map of this environment
    pub fn irradiance_view(&self) -> &B::ImageView {
        &*self.irradiance_view
    }

    /// Obtain view on the BRDF LUT of this environment
    pub fn brdf_lut_view(&self) -> &B::ImageView {
        &*self.brdf_lut_view
    }

    /// Obtain view on the filtered spec map of this environment
    pub fn spec_view(&self) -> &B::ImageView {
        &*self.spec_view
    }
}

impl<B> Drop for EnvironmentMaps<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::debug!("Dropping environment maps");

        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.irradiance_image));
            lock.device
                .destroy_image_view(ManuallyDrop::take(&mut self.irradiance_view));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.irradiance_memory));
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.spec_image));
            lock.device
                .destroy_image_view(ManuallyDrop::take(&mut self.spec_view));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.spec_memory));
            lock.device
                .destroy_image(ManuallyDrop::take(&mut self.brdf_lut_image));
            lock.device
                .destroy_image_view(ManuallyDrop::take(&mut self.brdf_lut_view));
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.brdf_lut_memory));
        }
    }
}
