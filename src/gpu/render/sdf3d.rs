use super::{GPURender, Renderer};
use crate::lang::{LightType, ParameterBool, ObjectType};
use crate::gpu::{Backend, InitializationError};
use zerocopy::AsBytes;
use std::mem::ManuallyDrop;
use gfx_hal as hal;
use gfx_hal::prelude::*;

static MAIN_FRAGMENT_SHADER_3D: &[u8] = include_bytes!("../../../shaders/renderer3d.spv");

#[derive(AsBytes, Debug)]
#[repr(C)]
/// Uniforms for a 3D Renderer
pub struct Uniforms {
    center: [f32; 4],
    light_pos: [f32; 4],
    resolution: [f32; 2],
    focal_length: f32,
    aperture_size: f32,
    focal_distance: f32,

    phi: f32,
    theta: f32,
    rad: f32,

    displacement: f32,
    tex_scale: f32,
    texel_size: f32,

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
            focal_distance: 5.0,
            phi: 1.,
            theta: 1.,
            rad: 6.,
            displacement: 0.1,
            tex_scale: 1.,
            texel_size: 1. / 1024.,
            environment_strength: 1.0,
            environment_blur: 0.5,
            light_type: LightType::PointLight,
            light_strength: 100.0,
            fog_strength: 0.2,
            shadow: 1,
            ao: 0,
        }
    }
}

impl Renderer for Uniforms {
    fn fragment_shader() -> &'static [u8] {
        MAIN_FRAGMENT_SHADER_3D
    }

    fn set_resolution(&mut self, w: f32, h: f32) {
        self.resolution = [w, h];
    }
}

impl<B> GPURender<B, Uniforms>
where
    B: Backend,
{
    pub fn switch_object_type(
        &mut self,
        object_type: ObjectType,
    ) -> Result<(), InitializationError> {
        let lock = self.gpu.lock().unwrap();
        let (main_render_pass, main_pipeline, main_pipeline_layout) =
            Self::make_render_pipeline(
                &lock.device,
                hal::format::Format::Rgba32Sfloat,
                &*self.main_descriptor_set_layout,
                object_type,
                super::MAIN_VERTEX_SHADER,
                MAIN_FRAGMENT_SHADER_3D,
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

    pub fn rotate_camera(&mut self, theta: f32, phi: f32) {
        self.view.phi += phi;
        self.view.theta += theta;
    }

    pub fn pan_camera(&mut self, x: f32, y: f32) {
        let point = (self.view.theta.cos(), self.view.theta.sin());
        let normal = (point.1, -point.0);

        let delta = (point.0 * y + normal.0 * x, point.1 * y + normal.1 * x);

        self.view.center[0] += delta.0;
        self.view.center[2] += delta.1;
    }

    pub fn zoom_camera(&mut self, z: f32) {
        self.view.rad += z;
    }

    pub fn move_light(&mut self, x: f32, y: f32) {
        self.view.light_pos[0] += x;
        self.view.light_pos[2] += y;
    }

    pub fn set_displacement_amount(&mut self, displacement: f32) {
        self.view.displacement = displacement;
    }

    pub fn set_texture_scale(&mut self, scale: f32) {
        self.view.tex_scale = scale;
        self.view.texel_size = scale / self.image_size as f32;
    }

    pub fn set_light_type(&mut self, light_type: LightType) {
        self.view.light_type = light_type;
    }

    pub fn set_light_strength(&mut self, strength: f32) {
        self.view.light_strength = strength;
    }

    pub fn set_fog_strength(&mut self, strength: f32) {
        self.view.fog_strength = strength;
    }

    pub fn set_environment_strength(&mut self, strength: f32) {
        self.view.environment_strength = strength;
    }

    pub fn set_environment_blur(&mut self, blur: f32) {
        self.view.environment_blur = blur;
    }

    pub fn set_shadow(&mut self, shadow: ParameterBool) {
        self.view.shadow = shadow;
    }

    pub fn set_ao(&mut self, ao: ParameterBool) {
        self.view.ao = ao;
    }

    pub fn set_focal_length(&mut self, focal_length: f32) {
        self.view.focal_length = focal_length;
    }

    pub fn set_aperture_size(&mut self, aperture_size: f32) {
        self.view.aperture_size = aperture_size;
    }

    pub fn set_focal_distance(&mut self, focal_distance: f32) {
        self.view.focal_distance = focal_distance;
    }
}
