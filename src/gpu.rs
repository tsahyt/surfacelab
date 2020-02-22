use gfx_backend_vulkan as back;
use gfx_hal as hal;
use gfx_hal::prelude::*;
pub use gfx_hal::Backend;
use std::collections::HashMap;
use std::sync::Arc;

pub struct GPU<B: Backend> {
    instance: B::Instance,
    device: B::Device,
    queue_group: hal::queue::QueueGroup<B>,
    shaders: HashMap<(ShaderType, &'static str), B::ShaderModule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderType {
    Compute,
    Vertex,
    Fragment,
}

pub struct Shader<B: Backend>(Arc<B::ShaderModule>);

/// Initialize the GPU, optionally headless. When headless is specified,
/// no graphics capable family is required.
pub fn initialize_gpu(headless: bool) -> Result<GPU<back::Backend>, String> {
    log::info!("Initializing GPU");

    let instance = back::Instance::create("surfacelab", 1)
        .map_err(|e| format!("Failed to create an instance! {:?}", e))?;
    let adapter = instance
        .enumerate_adapters()
        .into_iter()
        .find(|adapter| {
            adapter.queue_families.iter().any(|family| {
                family.queue_type().supports_compute()
                    && (headless || family.queue_type().supports_graphics())
            })
        })
        .ok_or(if headless {
            "Failed to find a GPU with compute support"
        } else {
            "Failed to find a GPU with compute and graphics support!"
        })?;

    GPU::new(instance, adapter, headless)
}

impl<B> GPU<B>
where
    B: Backend,
{
    pub fn new(
        instance: B::Instance,
        adapter: hal::adapter::Adapter<B>,
        headless: bool,
    ) -> Result<Self, String> {
        log::debug!("Using adapter {:?}", adapter);

        let memory_properties = adapter.physical_device.memory_properties();

        let family = adapter
            .queue_families
            .iter()
            .find(|family| {
                family.queue_type().supports_compute()
                    && (headless || family.queue_type().supports_graphics())
            })
            .unwrap();
        let mut gpu = unsafe {
            adapter
                .physical_device
                .open(&[(family, &[1.0])], hal::Features::empty())
                .unwrap()
        };

        let queue_group = gpu.queue_groups.pop().unwrap();
        let device = gpu.device;

        Ok(GPU {
            instance,
            device,
            queue_group,
            shaders: HashMap::new(),
        })
    }

    pub fn register_shader(
        &mut self,
        spirv: &[u8],
        name: &'static str,
        ty: ShaderType,
    ) -> Result<(), String> {
        let loaded_spirv = hal::pso::read_spirv(std::io::Cursor::new(spirv))
            .map_err(|e| format!("Failed to load SPIR-V: {}", e))?;
        let shader = unsafe { self.device.create_shader_module(&loaded_spirv) }
            .map_err(|e| format!("Failed to build shader module: {}", e))?;
        self.shaders.insert((ty, name), shader);
        Ok(())
    }
}

impl<B> Drop for GPU<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        log::info!("Dropping GPU")
    }
}
