use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::{Backend, CommandBuffer, Shader, ShaderType, GPU};

pub struct GPURender<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    command_pool: B::CommandPool,
}

impl<B> GPURender<B>
where
    B: Backend,
{
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, String> {
        log::info!("Obtaining GPU Render Resources");
        let lock = gpu.lock().unwrap();

        let command_pool = unsafe {
            lock.device.create_command_pool(
                lock.queue_group.family,
                hal::pool::CommandPoolCreateFlags::empty(),
            )
        }
        .map_err(|_| "Can't create command pool!")?;

        Ok(GPURender {
            gpu: gpu.clone(),
            command_pool: command_pool,
        })
    }
}
