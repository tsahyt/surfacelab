use super::{Backend, InitializationError, PipelineError, GPU};
use gfx_hal as hal;
use gfx_hal::prelude::*;
use image::hdr;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Mutex};

static IRRADIANCE_SHADER: &[u8] = include_bytes!("../../../shaders/irradiance.spv");

pub struct EnvironmentMaps<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    irradiance_image: ManuallyDrop<B::Image>,
    irradiance_view: ManuallyDrop<B::ImageView>,
    irradiance_memory: ManuallyDrop<B::Memory>,
}

impl<B> EnvironmentMaps<B>
where
    B: Backend,
{
    const FORMAT: hal::format::Format = hal::format::Format::Rgba32Sfloat;

    /// Initialize GPU side structures for environment map without data, given a
    /// cubemap size.
    fn init(gpu: Arc<Mutex<GPU<B>>>, size: usize) -> Result<Self, &'static str> {
        let lock = gpu.lock().unwrap();

        // Irradiance cube map
        let mut irradiance_image = unsafe {
            lock.device.create_image(
                hal::image::Kind::D2(size as u32, size as u32, 6, 1),
                1,
                Self::FORMAT,
                hal::image::Tiling::Linear,
                hal::image::Usage::SAMPLED | hal::image::Usage::STORAGE,
                hal::image::ViewCapabilities::KIND_CUBE,
            )
        }
        .map_err(|_| "Failed to acquire cube map image")?;

        let requirements = unsafe { lock.device.get_image_requirements(&irradiance_image) };
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
        let irradiance_memory =
            unsafe { lock.device.allocate_memory(memory_type, requirements.size) }
                .map_err(|_| "Failed to allocate memory for cube map")?;
        unsafe {
            lock.device
                .bind_image_memory(&irradiance_memory, 0, &mut irradiance_image)
        }
        .unwrap();

        let irradiance_view = unsafe {
            lock.device.create_image_view(
                &irradiance_image,
                hal::image::ViewKind::Cube,
                Self::FORMAT,
                hal::format::Swizzle::NO,
                super::super::COLOR_RANGE.clone(),
            )
        }
        .map_err(|_| "Failed to create cube map view")?;

        drop(lock);

        Ok(Self {
            gpu,
            irradiance_image: ManuallyDrop::new(irradiance_image),
            irradiance_memory: ManuallyDrop::new(irradiance_memory),
            irradiance_view: ManuallyDrop::new(irradiance_view),
        })
    }

    /// Create environment maps from a path to a HDRi file. Expects .hdr,
    /// equirectangular mapping!
    pub fn from_file<P: AsRef<Path>>(
        gpu: Arc<Mutex<GPU<B>>>,
        cubemap_size: usize,
        path: P,
    ) -> Result<Self, &'static str> {
        use std::fs::File;
        use std::io::BufReader;

        // Initialize
        let env_maps = Self::init(gpu, cubemap_size)?;

        // Read data from file
        let reader = BufReader::new(File::open(path).map_err(|_| "Failed to open HDRi file")?);
        let decoder = hdr::HdrDecoder::new(reader).map_err(|_| "Failed to decode HDRi file")?;
        let metadata = decoder.metadata();
        let raw_hdri = decoder
            .read_image_hdr()
            .map_err(|_| "Failed to read from HDRi file")?;

        // Prepare compute pipeline
        let lock = env_maps.gpu.lock().unwrap();

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "")?;
        let descriptor_pool = unsafe {
            use hal::pso::*;
            lock.device
                .create_descriptor_pool(4, &[], DescriptorPoolCreateFlags::empty())
        }
        .map_err(|_| "Failed to create descriptor pool")?;

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
        }
        .map_err(|_| "Failed to create descriptor set layout")?;

        let pipeline_layout = unsafe { lock.device.create_pipeline_layout(Some(&set_layout), &[]) }
            .map_err(|_| "Failed to create compute pipeline")?;

        let irradiance_module = {
            let loaded_spirv =
                hal::pso::read_spirv(std::io::Cursor::new(IRRADIANCE_SHADER)).map_err(|_| "")?;
            unsafe { lock.device.create_shader_module(&loaded_spirv) }.map_err(|_| "")?
        };

        let entry_point = hal::pso::EntryPoint {
            entry: "main",
            module: &irradiance_module,
            specialization: hal::pso::Specialization::default(),
        };

        let pipeline = unsafe {
            lock.device.create_compute_pipeline(
                &hal::pso::ComputePipelineDesc::new(entry_point, &pipeline_layout),
                None,
            )
        }
        .map_err(|_| "Failed to create compute pipeline")?;

        let fence = lock.device.create_fence(false).unwrap();
        let sampler = unsafe {
            lock.device.create_sampler(&hal::image::SamplerDesc::new(
                hal::image::Filter::Linear,
                hal::image::WrapMode::Tile,
            ))
        }
        .map_err(|_| "Failed to create sampler")?;

        // Convolve irradiance map

        // TODO: Pre-filter environment map

        // Clean up compute pipeline
        unsafe {
            lock.device.destroy_command_pool(command_pool);
            lock.device.destroy_sampler(sampler);
            lock.device.destroy_descriptor_pool(descriptor_pool);
            lock.device.destroy_descriptor_set_layout(set_layout);
            lock.device.destroy_shader_module(irradiance_module);
            lock.device.destroy_fence(fence);
            lock.device.destroy_pipeline_layout(pipeline_layout);
            lock.device.destroy_compute_pipeline(pipeline);
        }
        todo!()
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
        }
    }
}
