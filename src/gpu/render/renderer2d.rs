use super::{GPURender, Renderer};
use crate::gpu::{Backend, InitializationError, GPU};
use std::sync::{Arc, Mutex};
use zerocopy::AsBytes;

static MAIN_FRAGMENT_SHADER_2D: &[u8] = include_bytes!("../../../shaders/renderer2d.spv");

pub type Renderer2D<B> = GPURender<B, Uniforms>;

#[derive(AsBytes, Debug)]
#[repr(C)]
/// Uniforms for a 2D Renderer
pub struct Uniforms {
    resolution: [f32; 2],
    pan: [f32; 2],
    zoom: f32,
    channel: u32,
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            resolution: [1024.0, 1024.0],
            pan: [0., 0.],
            zoom: 1.,
            channel: 0,
        }
    }
}

impl Renderer for Uniforms {
    fn fragment_shader() -> &'static [u8] {
        MAIN_FRAGMENT_SHADER_2D
    }

    fn set_resolution(&mut self, w: f32, h: f32) {
        self.resolution = [w, h];
    }
}

impl<B> GPURender<B, Uniforms>
where
    B: Backend,
{
    pub fn new_2d(
        gpu: &Arc<Mutex<GPU<B>>>,
        monitor_dimensions: (u32, u32),
        viewport_dimensions: (u32, u32),
        image_size: u32,
    ) -> Result<Self, InitializationError> {
        Self::new(
            gpu,
            monitor_dimensions,
            viewport_dimensions,
            image_size,
            Uniforms::default(),
        )
    }

    /// Pan the camera in x and y directions
    pub fn pan_camera(&mut self, x: f32, y: f32) {
        self.view.pan[0] += x;
        self.view.pan[1] += y;
    }

    /// Zoom the camera linearly
    pub fn zoom_camera(&mut self, z: f32) {
        self.view.zoom += z;
    }

    /// Set the channel to be displayed
    pub fn set_channel(&mut self, channel: u32) {
        self.view.channel = channel;
    }
}
