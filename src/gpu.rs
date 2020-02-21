use gfx_backend_vulkan as back;
use gfx_hal as hal;
use gfx_hal::prelude::*;

pub struct GPU<B: hal::Backend> {
    instance: B::Instance,
    device: B::Device,
    queue_group: hal::queue::QueueGroup<B>,
}

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
    B: hal::Backend,
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
        })
    }
}

impl<B> Drop for GPU<B>
where
    B: hal::Backend,
{
    fn drop(&mut self) {
        log::info!("Dropping GPU")
    }
}
