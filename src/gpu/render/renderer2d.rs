use super::{Backend, GPURender, Renderer};
use zerocopy::AsBytes;

static MAIN_FRAGMENT_SHADER_2D: &[u8] = include_bytes!("../../../shaders/renderer2d.spv");

#[derive(AsBytes, Debug)]
#[repr(C)]
/// Uniforms for a 2D Renderer
struct Uniforms2D {
    resolution: [f32; 2],
    pan: [f32; 2],
    zoom: f32,
    channel: u32,
}

impl Default for Uniforms2D {
    fn default() -> Self {
        Self {
            resolution: [1024.0, 1024.0],
            pan: [0., 0.],
            zoom: 1.,
            channel: 0,
        }
    }
}

impl Renderer for Uniforms2D {
    fn fragment_shader() -> &'static [u8] {
        MAIN_FRAGMENT_SHADER_2D
    }

    fn set_resolution(&mut self, w: f32, h: f32) {
        self.resolution = [w, h];
    }
}

impl<B> GPURender<B, Uniforms2D>
where
    B: Backend,
{
    pub fn pan_camera(&mut self, x: f32, y: f32) {
        self.view.pan[0] += x;
        self.view.pan[1] += y;
    }

    pub fn zoom_camera(&mut self, z: f32) {
        self.view.zoom += z;
    }

    pub fn set_channel(&mut self, channel: u32) {
        self.view.channel = channel;
    }
}
