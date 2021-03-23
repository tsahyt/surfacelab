use super::{GPURender, InitializationError, Renderer};
use crate::gpu::{Backend, GPU};
use crate::lang::{LightType, ObjectType, ParameterBool};
use crate::shader;
use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use zerocopy::AsBytes;

static MAIN_VERTEX_SHADER_3D: &[u8] = shader!("quad");
static MAIN_FRAGMENT_SHADER_3D: &[u8] = shader!("sdf3d");

/// A 3D renderer using ray tracing/sphere tracing to display the PBR material
/// with real displacement. Designed for temporal multisampling
pub type RendererSDF3D<B> = GPURender<B, Uniforms>;

#[derive(AsBytes, Debug)]
#[repr(C)]
/// Uniforms for a 3D Renderer
pub struct Uniforms {
    center: [f32; 4],
    light_pos: [f32; 4],
    resolution: [f32; 2],
    focal_length: f32,
    aperture_size: f32,
    aperture_blades: i32,
    aperture_rotation: f32,
    focal_distance: f32,

    phi: f32,
    theta: f32,
    rad: f32,

    displacement: f32,
    tex_scale: f32,

    environment_strength: f32,
    environment_blur: f32,

    light_type: LightType,
    light_strength: f32,
    fog_strength: f32,

    shadow: ParameterBool,
    ao: ParameterBool,
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            resolution: [1024.0, 1024.0],
            center: [0., 0., 0., 0.],
            light_pos: [0., 3., 0., 0.],
            focal_length: 1.0,
            aperture_size: 0.0,
            aperture_blades: 6,
            aperture_rotation: 0.,
            focal_distance: 5.0,
            phi: 1.,
            theta: 1.,
            rad: 6.,
            displacement: 0.1,
            tex_scale: 1.,
            environment_strength: 1.0,
            environment_blur: 3.0,
            light_type: LightType::PointLight,
            light_strength: 100.0,
            fog_strength: 0.0,
            shadow: 1,
            ao: 0,
        }
    }
}

impl Renderer for Uniforms {
    fn vertex_shader() -> &'static [u8] {
        MAIN_VERTEX_SHADER_3D
    }

    fn fragment_shader() -> &'static [u8] {
        MAIN_FRAGMENT_SHADER_3D
    }

    fn set_resolution(&mut self, w: f32, h: f32) {
        self.resolution = [w, h];
    }

    fn uniforms(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<B> GPURender<B, Uniforms>
where
    B: Backend,
{
    /// Create a new SDF3D renderer
    pub fn new_sdf3d(
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

    /// Switch the object type to be displayed. This function recreates the
    /// pipeline and will therefore incur a slight time penalty.
    pub fn switch_object_type(
        &mut self,
        object_type: ObjectType,
    ) -> Result<(), InitializationError> {
        let lock = self.gpu.lock().unwrap();

        let (main_render_pass, main_pipeline, main_pipeline_layout) = Self::make_render_pipeline(
            &lock.device,
            hal::format::Format::Rgba32Sfloat,
            &*self.main_descriptor_set_layout,
            object_type,
            Uniforms::vertex_shader(),
            Uniforms::fragment_shader(),
        )?;

        unsafe {
            lock.device
                .destroy_render_pass(ManuallyDrop::take(&mut self.main_render_pass));
            lock.device
                .destroy_graphics_pipeline(ManuallyDrop::take(&mut self.main_pipeline));
            lock.device
                .destroy_pipeline_layout(ManuallyDrop::take(&mut self.main_pipeline_layout));
        }

        self.main_render_pass = ManuallyDrop::new(main_render_pass);
        self.main_pipeline = ManuallyDrop::new(main_pipeline);
        self.main_pipeline_layout = ManuallyDrop::new(main_pipeline_layout);

        Ok(())
    }

    /// Rotate the camera by two angles theta and phi
    pub fn rotate_camera(&mut self, theta: f32, phi: f32) {
        self.view.phi += phi;
        self.view.theta += theta;
    }

    /// Set the camera center in absolute coordinates
    pub fn set_center(&mut self, x: f32, y: f32) {
        self.view.center[0] = x;
        self.view.center[2] = y;
    }

    /// Pan the camera given screen space input deltas
    pub fn pan_camera(&mut self, x: f32, y: f32) {
        let point = (self.view.theta.cos(), self.view.theta.sin());
        let normal = (point.1, -point.0);

        let delta = (point.0 * y + normal.0 * x, point.1 * y + normal.1 * x);

        self.view.center[0] += delta.0;
        self.view.center[2] += delta.1;
    }

    /// Zoom the camera linearly
    pub fn zoom_camera(&mut self, z: f32) {
        self.view.rad += z;
    }

    /// Move the light given screen space input deltas
    pub fn move_light(&mut self, x: f32, y: f32) {
        self.view.light_pos[0] += x;
        self.view.light_pos[2] += y;
    }

    /// Update the displacement amount to be renderered
    pub fn set_displacement_amount(&mut self, displacement: f32) {
        self.view.displacement = displacement;
    }

    /// Update the texture scale to be rendered
    pub fn set_texture_scale(&mut self, scale: f32) {
        self.view.tex_scale = scale;
    }

    /// Set the light type to be rendered
    pub fn set_light_type(&mut self, light_type: LightType) {
        self.view.light_type = light_type;
    }

    /// Set the light strength
    pub fn set_light_strength(&mut self, strength: f32) {
        self.view.light_strength = strength;
    }

    /// Set the fog strength
    pub fn set_fog_strength(&mut self, strength: f32) {
        self.view.fog_strength = strength;
    }

    /// Set the strength of environment lighting (IBL)
    pub fn set_environment_strength(&mut self, strength: f32) {
        self.view.environment_strength = strength;
    }

    /// Determine how much to blur the environment map background
    pub fn set_environment_blur(&mut self, blur: f32) {
        self.view.environment_blur = blur;
    }

    /// Set whether a shadow should be rendered for the light source
    pub fn set_shadow(&mut self, shadow: ParameterBool) {
        self.view.shadow = shadow;
    }

    /// Set whether ambient occlusion should be rendered
    pub fn set_ao(&mut self, ao: ParameterBool) {
        self.view.ao = ao;
    }

    /// Set the camera focal length
    pub fn set_focal_length(&mut self, focal_length: f32) {
        self.view.focal_length = focal_length;
    }

    /// Set the camera aperture size
    pub fn set_aperture_size(&mut self, aperture_size: f32) {
        self.view.aperture_size = aperture_size;
    }

    /// Set the camera aperture rotation
    pub fn set_aperture_rotation(&mut self, aperture_rotation: f32) {
        self.view.aperture_rotation = aperture_rotation;
    }

    /// Set the camera aperture blade count
    pub fn set_aperture_blades(&mut self, aperture_blades: i32) {
        self.view.aperture_blades = aperture_blades;
    }

    /// Set the camera focal distance
    pub fn set_focal_distance(&mut self, focal_distance: f32) {
        self.view.focal_distance = focal_distance;
    }
}
