use super::{GPURender, InitializationError, Renderer};
use crate::gpu::{Backend, GPU};
use crate::shader;
use serde_derive::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use zerocopy::AsBytes;

static MAIN_VERTEX_SHADER_2D: &[u8] = shader!("quad");
static MAIN_FRAGMENT_SHADER_2D: &[u8] = shader!("renderer2d");

/// A 2D renderer displaying a "top down" view on the texture channel.
pub type Renderer2D<B> = GPURender<B, Uniforms>;

#[derive(AsBytes, Debug, Serialize, Deserialize)]
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
            channel: 5,
        }
    }
}

impl Renderer for Uniforms {
    fn vertex_shader() -> &'static [u8] {
        MAIN_VERTEX_SHADER_2D
    }

    fn fragment_shader() -> &'static [u8] {
        MAIN_FRAGMENT_SHADER_2D
    }

    fn set_resolution(&mut self, w: f32, h: f32) {
        self.resolution = [w, h];
    }

    fn uniforms(&self) -> &[u8] {
        self.as_bytes()
    }

    fn serialize(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        serde_cbor::ser::to_vec(self)
    }

    fn deserialize(&mut self, data: &[u8]) -> Result<(), serde_cbor::Error> {
        *self = serde_cbor::de::from_slice(data)?;
        Ok(())
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
    ) -> Result<Self, InitializationError> {
        Self::new(
            gpu,
            monitor_dimensions,
            viewport_dimensions,
            Uniforms::default(),
        )
    }

    /// Set the camera center in absolute coordinates
    pub fn set_center(&mut self, x: f32, y: f32) {
        self.view.pan[0] = x;
        self.view.pan[1] = y;
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
