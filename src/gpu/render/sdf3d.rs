use super::{GPURender, InitializationError, Renderer};
use crate::lang::{LightType, ObjectType, ParameterBool, ShadingMode};
use crate::shader;
use crate::{
    gpu::{Backend, GPU},
    lang::{ParamBoxDescription, RenderField},
};
use gfx_hal as hal;
use gfx_hal::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use zerocopy::AsBytes;

static MAIN_VERTEX_SHADER_3D: &[u8] = shader!("quad");
static MAIN_FRAGMENT_SHADER_3D: &[u8] = shader!("sdf3d");

/// A 3D renderer using ray tracing/sphere tracing to display the PBR material
/// with real displacement. Designed for temporal multisampling
pub type RendererSDF3D<B> = GPURender<B, Uniforms>;

#[derive(AsBytes, Debug, Serialize, Deserialize)]
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
    environment_rotation: f32,
    ambient_occlusion_strength: f32,

    light_type: LightType,
    light_strength: f32,
    light_size: f32,
    fog_strength: f32,
    shadow: ParameterBool,
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
            environment_rotation: 0.,
            ambient_occlusion_strength: 0.5,
            light_type: LightType::PointLight,
            light_strength: 100.0,
            light_size: 1.0,
            fog_strength: 0.0,
            shadow: 1,
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

    fn serialize(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        serde_cbor::ser::to_vec(self)
    }

    fn deserialize(&mut self, data: &[u8]) -> Result<(), serde_cbor::Error> {
        // Get all the fields that must remain the same
        let res = self.resolution;

        // Get settings from slice
        *self = serde_cbor::de::from_slice(data)?;

        // Write back fields
        self.resolution = res;
        Ok(())
    }

    fn parameters(&self) -> ParamBoxDescription<RenderField> {
        use crate::lang::parameters::*;
        use strum::VariantNames;

        ParamBoxDescription {
            box_title: "renderer".to_string(),
            preset_tag: Some("renderer".to_string()),
            categories: vec![
                ParamCategory {
                    name: "renderer",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![],
                },
                ParamCategory {
                    name: "geometry",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![
                        Parameter {
                            name: "displacement-amount".to_string(),
                            control: Control::Slider {
                                value: self.displacement,
                                min: 0.0,
                                max: 1.0,
                            },
                            transmitter: RenderField::DisplacementAmount,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "tex-scale".to_string(),
                            control: Control::Slider {
                                value: self.tex_scale,
                                min: 0.0,
                                max: 4.0,
                            },
                            transmitter: RenderField::TextureScale,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
                ParamCategory {
                    name: "environment",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![
                        Parameter {
                            name: "hdri-strength".to_string(),
                            control: Control::Slider {
                                value: self.environment_strength,
                                min: 0.0,
                                max: 4.0,
                            },
                            transmitter: RenderField::EnvironmentStrength,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "hdri-blur".to_string(),
                            control: Control::Slider {
                                value: self.environment_blur,
                                min: 0.0,
                                max: 6.0,
                            },
                            transmitter: RenderField::EnvironmentBlur,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "hdri-rotation".to_string(),
                            control: Control::Slider {
                                value: self.environment_rotation,
                                min: 0.0,
                                max: std::f32::consts::TAU,
                            },
                            transmitter: RenderField::EnvironmentRotation,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "ambient-occlusion-strength".to_string(),
                            control: Control::Slider {
                                value: self.ambient_occlusion_strength,
                                min: 0.0,
                                max: 2.0,
                            },
                            transmitter: RenderField::AoStrength,
                            expose_status: None,
                            visibility: VisibilityFunction::on_parameter("shading-mode", |c| {
                                if let Control::Enum { selected, .. } = c {
                                    unsafe { ShadingMode::from_unchecked(*selected as u32) }
                                        .has_lights()
                                } else {
                                    false
                                }
                            }),
                            presetable: false,
                        },
                        Parameter {
                            name: "fog-strength".to_string(),
                            control: Control::Slider {
                                value: self.fog_strength,
                                min: 0.0,
                                max: 1.0,
                            },
                            transmitter: RenderField::FogStrength,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
                ParamCategory {
                    name: "matcap",
                    is_open: true,
                    visibility: VisibilityFunction::on_parameter("shading-mode", |c| {
                        if let Control::Enum { selected, .. } = c {
                            unsafe { ShadingMode::from_unchecked(*selected as u32) }.has_matcap()
                        } else {
                            false
                        }
                    }),
                    parameters: vec![],
                },
                ParamCategory {
                    name: "light",
                    is_open: true,
                    visibility: VisibilityFunction::on_parameter("shading-mode", |c| {
                        if let Control::Enum { selected, .. } = c {
                            unsafe { ShadingMode::from_unchecked(*selected as u32) }.has_lights()
                        } else {
                            false
                        }
                    }),
                    parameters: vec![
                        Parameter {
                            name: "light-type".to_string(),
                            control: Control::Enum {
                                selected: self.light_type as usize,
                                variants: LightType::VARIANTS
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect(),
                            },
                            transmitter: RenderField::LightType,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "light-strength".to_string(),
                            control: Control::Slider {
                                value: self.light_strength,
                                min: 0.0,
                                max: 1000.0,
                            },
                            transmitter: RenderField::LightStrength,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "light-size".to_string(),
                            control: Control::Slider {
                                value: self.light_size,
                                min: 0.01,
                                max: 2.0,
                            },
                            transmitter: RenderField::LightSize,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "shadow".to_string(),
                            control: Control::Toggle {
                                def: self.shadow == 1,
                            },
                            transmitter: RenderField::Shadow,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
                ParamCategory {
                    name: "camera",
                    is_open: true,
                    visibility: VisibilityFunction::default(),
                    parameters: vec![
                        Parameter {
                            name: "focal-length".to_string(),
                            control: Control::Slider {
                                value: self.focal_length,
                                min: 0.2,
                                max: 10.0,
                            },
                            transmitter: RenderField::FocalLength,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "aperture-size".to_string(),
                            control: Control::Slider {
                                value: self.aperture_size,
                                min: 0.0,
                                max: 0.1,
                            },
                            transmitter: RenderField::ApertureSize,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "aperture-blades".to_string(),
                            control: Control::DiscreteSlider {
                                value: self.aperture_blades,
                                min: 0,
                                max: 12,
                            },
                            transmitter: RenderField::ApertureBlades,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "aperture-rotation".to_string(),
                            control: Control::Slider {
                                value: self.aperture_rotation,
                                min: 0.0,
                                max: std::f32::consts::TAU,
                            },
                            transmitter: RenderField::ApertureRotation,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                        Parameter {
                            name: "focal-distance".to_string(),
                            control: Control::Slider {
                                value: self.focal_distance,
                                min: 1.0,
                                max: 40.0,
                            },
                            transmitter: RenderField::FocalDistance,
                            expose_status: None,
                            visibility: VisibilityFunction::default(),
                            presetable: false,
                        },
                    ],
                },
            ],
        }
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
        let mut renderer = Self::new(
            gpu,
            monitor_dimensions,
            viewport_dimensions,
            Uniforms::default(),
        )?;

        renderer.view.resolution = [viewport_dimensions.0 as f32, viewport_dimensions.1 as f32];
        renderer.object_type = Some(ObjectType::Cube);
        renderer.shading_mode = Some(ShadingMode::Pbr);

        Ok(renderer)
    }

    /// Switch the object type to be displayed. This function recreates the
    /// pipeline and will therefore incur a slight time penalty.
    pub fn switch_object_type(
        &mut self,
        object_type: ObjectType,
    ) -> Result<(), InitializationError> {
        self.object_type = Some(object_type);

        let lock = self.gpu.lock().unwrap();

        let (main_render_pass, main_pipeline, main_pipeline_layout) = Self::make_render_pipeline(
            &lock.device,
            hal::format::Format::Rgba32Sfloat,
            &*self.main_descriptor_set_layout,
            object_type,
            self.shading_mode.unwrap_or(ShadingMode::Pbr),
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

    /// Switch shading mode. This function recreates the pipeline and will
    /// therefore incur a slight time penalty.
    pub fn switch_shading_mode(
        &mut self,
        shading_mode: ShadingMode,
    ) -> Result<(), InitializationError> {
        self.shading_mode = Some(shading_mode);

        let lock = self.gpu.lock().unwrap();

        let (main_render_pass, main_pipeline, main_pipeline_layout) = Self::make_render_pipeline(
            &lock.device,
            hal::format::Format::Rgba32Sfloat,
            &*self.main_descriptor_set_layout,
            self.object_type.unwrap_or(ObjectType::Cube),
            shading_mode,
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

    /// Set the light size
    pub fn set_light_size(&mut self, size: f32) {
        self.view.light_size = size;
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

    /// Determine how much to blur the environment map background
    pub fn set_environment_rotation(&mut self, rotation: f32) {
        self.view.environment_rotation = rotation;
    }

    /// Set whether a shadow should be rendered for the light source
    pub fn set_shadow(&mut self, shadow: ParameterBool) {
        self.view.shadow = shadow;
    }

    /// Set the strength of AO to be rendered
    pub fn set_ao_strength(&mut self, ao_strength: f32) {
        self.view.ambient_occlusion_strength = ao_strength;
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
