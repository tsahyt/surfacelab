use crate::{broker, gpu, lang::*, util::*};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use strum::IntoEnumIterator;

const MAX_SAMPLES: usize = 16;
const TIMING_DECAY: f64 = 0.15;

pub fn start_render_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    let (sender, receiver, disconnector) = broker.subscribe();
    thread::Builder::new()
        .name("render".to_string())
        .spawn(move || {
            log::info!("Starting Renderer");

            let mut render_manager = RenderManager::new(gpu);

            loop {
                let res = if let Ok(message) = receiver.try_recv() {
                    render_manager.step(Some(message))
                } else if render_manager.must_step() {
                    thread::sleep(std::time::Duration::from_millis(5));
                    render_manager.step(None)
                } else {
                    render_manager.step(receiver.recv().ok())
                };

                match res {
                    None => break,
                    Some(res) => {
                        for r in res {
                            sender.send(r).unwrap();
                        }
                    }
                }
            }

            log::info!("Renderer terminating");
            disconnector.disconnect();
        })
        .expect("Failed to spawn render thread!")
}

enum ManagedRenderer<B: gpu::Backend> {
    RendererSDF3D(gpu::render::RendererSDF3D<B>),
    Renderer2D(gpu::render::Renderer2D<B>),
}

impl<B> ManagedRenderer<B>
where
    B: gpu::Backend,
{
    pub fn as_sdf3d(&mut self) -> Option<&mut gpu::render::RendererSDF3D<B>> {
        match self {
            ManagedRenderer::RendererSDF3D(r) => Some(r),
            ManagedRenderer::Renderer2D(_) => None,
        }
    }

    pub fn as_2d(&mut self) -> Option<&mut gpu::render::Renderer2D<B>> {
        match self {
            ManagedRenderer::RendererSDF3D(_) => None,
            ManagedRenderer::Renderer2D(r) => Some(r),
        }
    }

    pub fn reset_sampling(&mut self) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.reset_sampling(),
            ManagedRenderer::Renderer2D(r) => r.reset_sampling(),
        }
    }

    pub fn render(&mut self) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.render(),
            ManagedRenderer::Renderer2D(r) => r.render(),
        }
    }

    pub fn target_view(&self) -> &Arc<Mutex<B::ImageView>> {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.target_view(),
            ManagedRenderer::Renderer2D(r) => r.target_view(),
        }
    }

    pub fn recreate_image_slots(&mut self, image_size: u32) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.recreate_image_slots(image_size),
            ManagedRenderer::Renderer2D(r) => r.recreate_image_slots(image_size),
        }
        .expect("Failed to resize images in renderer");
    }

    pub fn set_viewport_dimensions(&mut self, width: u32, height: u32) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.set_viewport_dimensions(width, height),
            ManagedRenderer::Renderer2D(r) => r.set_viewport_dimensions(width, height),
        }
    }

    pub fn transfer_image(
        &mut self,
        source: &B::Image,
        source_layout: gfx_hal::image::Layout,
        source_access: gfx_hal::image::Access,
        source_size: i32,
        image_use: crate::lang::OutputType,
    ) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => {
                r.transfer_image(source, source_layout, source_access, source_size, image_use)
            }
            ManagedRenderer::Renderer2D(r) => {
                r.transfer_image(source, source_layout, source_access, source_size, image_use)
            }
        }
    }

    pub fn vacate_image(&mut self, image_use: crate::lang::OutputType) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.vacate_image(image_use),
            ManagedRenderer::Renderer2D(r) => r.vacate_image(image_use),
        }
    }
}

struct Renderer<B: gpu::Backend> {
    gpu: ManagedRenderer<B>,
    samples_to_go: usize,
    max_samples: usize,
    frametime_ema: EMA<f64>,
}

impl<B: gpu::Backend> Renderer<B> {
    pub fn new(gpu: ManagedRenderer<B>, max_samples: usize) -> Self {
        Self {
            gpu,
            samples_to_go: 0,
            max_samples,
            frametime_ema: EMA::new(0., TIMING_DECAY),
        }
    }

    pub fn reset_sampling(&mut self) {
        self.samples_to_go = self.max_samples;
        self.gpu.reset_sampling();
    }
}

impl<B> std::ops::Deref for Renderer<B>
where
    B: gpu::Backend,
{
    type Target = ManagedRenderer<B>;

    fn deref(&self) -> &Self::Target {
        &self.gpu
    }
}

impl<B> std::ops::DerefMut for Renderer<B>
where
    B: gpu::Backend,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gpu
    }
}

struct RenderManager<B: gpu::Backend> {
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    renderers: HashMap<RendererID, Renderer<B>>,
}

impl<B> RenderManager<B>
where
    B: gpu::Backend,
{
    pub fn new(gpu: Arc<Mutex<gpu::GPU<B>>>) -> Self {
        RenderManager {
            gpu,
            renderers: HashMap::new(),
        }
    }

    pub fn must_step(&self) -> bool {
        self.renderers.values().any(|r| r.samples_to_go > 0)
    }

    pub fn step(&mut self, event: Option<Arc<Lang>>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();

        if let Some(ev) = event {
            response.append(&mut self.handle_event(&ev)?);
        } else {
            for id in self
                .renderers
                .iter()
                .filter(|(_, renderer)| renderer.samples_to_go > 0)
                .map(|x| x.0)
                .copied()
                .collect::<SmallVec<[_; 4]>>()
            {
                self.redraw(id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(id)));
            }
        }

        Some(response)
    }

    fn handle_event(&mut self, event: &Lang) -> Option<Vec<Lang>> {
        let mut response = Vec::new();

        match event {
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::UserIOEvent(UserIOEvent::OpenSurface(..)) => self.reset_all(),
            Lang::UserIOEvent(UserIOEvent::NewSurface) => self.reset_all(),
            Lang::UserIOEvent(UserIOEvent::SetParentSize(new_size)) => {
                self.resize_images(*new_size)
            }
            Lang::UIEvent(UIEvent::RendererRequested(id, monitor_size, view_size, ty)) => {
                let view = self
                    .new_renderer(*id, *monitor_size, *view_size, *ty)
                    .unwrap();
                response.push(Lang::RenderEvent(RenderEvent::RendererAdded(*id, view)))
            }
            Lang::UIEvent(UIEvent::RendererRedraw(id)) => self.redraw(*id),
            Lang::UIEvent(UIEvent::RendererResize(id, width, height)) => {
                self.resize(*id, *width, *height);
                self.redraw(*id);
            }
            Lang::UIEvent(UIEvent::RendererRemoved(id)) => self.remove(*id),
            Lang::ComputeEvent(ComputeEvent::OutputReady(
                _res,
                img,
                layout,
                access,
                size,
                out_ty,
            )) => self.transfer_output(img, *layout, *access, *size as i32, *out_ty),
            Lang::GraphEvent(GraphEvent::OutputRemoved(_res, out_ty)) => {
                self.disconnect_output(*out_ty);
            }
            Lang::UserRenderEvent(UserRenderEvent::Rotate(id, theta, phi)) => {
                self.rotate_camera(*id, *theta, *phi);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::Pan(id, x, y)) => {
                self.pan_camera(*id, *x, *y);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::Zoom(id, z)) => {
                self.zoom_camera(*id, *z);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::LightMove(id, x, y)) => {
                self.move_light(*id, *x, *y);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::ChannelChange2D(id, channel)) => {
                self.set_channel(*id, *channel);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::ObjectType(id, object_type)) => {
                self.switch_object_type(*id, *object_type);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::DisplacementAmount(id, displ)) => {
                self.set_displacement_amount(*id, *displ);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::TextureScale(id, scale)) => {
                self.set_texture_scale(*id, *scale);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::EnvironmentStrength(id, strength)) => {
                self.set_environment_strength(*id, *strength);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::EnvironmentBlur(id, blur)) => {
                self.set_environment_blur(*id, *blur);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::LightType(id, light_type)) => {
                self.set_light_type(*id, *light_type);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::LightStrength(id, strength)) => {
                self.set_light_strength(*id, *strength);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::FogStrength(id, strength)) => {
                self.set_fog_strength(*id, *strength);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::SetShadow(id, shadow)) => {
                self.set_shadow(*id, *shadow);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::SetAO(id, ao)) => {
                self.set_ao(*id, *ao);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::LoadHDRI(id, path)) => {
                self.load_hdri(*id, path);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::FocalLength(id, focal_length)) => {
                self.set_focal_length(*id, *focal_length);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::FocalDistance(id, focal_distance)) => {
                self.set_focal_distance(*id, *focal_distance);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::ApertureSize(id, aperture)) => {
                self.set_aperture_size(*id, *aperture);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            _ => {}
        }

        Some(response)
    }

    pub fn new_renderer(
        &mut self,
        id: RendererID,
        monitor_dimensions: (u32, u32),
        viewport_dimensions: (u32, u32),
        ty: RendererType,
    ) -> Result<gpu::BrokerImageView, String> {
        let mut renderer = match ty {
            RendererType::Renderer3D => Renderer::new(
                ManagedRenderer::RendererSDF3D(
                    gpu::render::GPURender::new_sdf3d(
                        &self.gpu,
                        monitor_dimensions,
                        viewport_dimensions,
                        1024,
                    )
                    .map_err(|e| format!("{:?}", e))?,
                ),
                MAX_SAMPLES,
            ),
            RendererType::Renderer2D => Renderer::new(
                ManagedRenderer::Renderer2D(
                    gpu::render::GPURender::new_2d(
                        &self.gpu,
                        monitor_dimensions,
                        viewport_dimensions,
                        1024,
                    )
                    .map_err(|e| format!("{:?}", e))?,
                ),
                1,
            ),
        };

        let now = Instant::now();
        renderer.render();
        renderer
            .frametime_ema
            .update(now.elapsed().as_micros() as f64);
        let view = gpu::BrokerImageView::from::<B>(renderer.target_view());
        self.renderers.insert(id, renderer);

        Ok(view)
    }

    pub fn remove(&mut self, renderer_id: RendererID) {
        self.renderers.remove(&renderer_id);
    }

    pub fn redraw_all(&mut self) {
        for r in self.renderers.values_mut() {
            let now = Instant::now();
            r.render();
            r.frametime_ema.update(now.elapsed().as_micros() as f64);
            r.samples_to_go = r.samples_to_go.saturating_sub(1);
        }
    }

    pub fn reset_all(&mut self) {
        for output in OutputType::iter() {
            self.disconnect_output(output);
        }
        self.redraw_all();
    }

    pub fn redraw(&mut self, renderer_id: RendererID) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            let now = Instant::now();
            r.render();
            r.frametime_ema.update(now.elapsed().as_micros() as f64);
            r.samples_to_go = r.samples_to_go.saturating_sub(1);
        } else {
            log::error!("Trying to redraw on non-existent renderer!");
        }
    }

    pub fn resize_images(&mut self, new_size: u32) {
        for r in self.renderers.values_mut() {
            r.recreate_image_slots(new_size);
            r.reset_sampling();
        }
    }

    pub fn resize(&mut self, renderer_id: RendererID, width: u32, height: u32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_viewport_dimensions(width, height);
            r.reset_sampling();
        }
    }

    pub fn transfer_output(
        &mut self,
        image: &gpu::BrokerImage,
        layout: gpu::Layout,
        access: gpu::Access,
        image_size: i32,
        output_type: OutputType,
    ) {
        for r in self.renderers.values_mut() {
            if let Some(img) = image.clone().to::<B>().and_then(|i| i.upgrade()) {
                {
                    let lock = img.lock().unwrap();
                    r.transfer_image(&lock, layout, access, image_size, output_type);
                }
                r.reset_sampling();
            }
        }
    }

    pub fn disconnect_output(&mut self, output_type: OutputType) {
        for r in self.renderers.values_mut() {
            r.vacate_image(output_type);
        }
    }

    pub fn rotate_camera(&mut self, renderer_id: RendererID, phi: f32, theta: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.rotate_camera(phi, theta);
            r.reset_sampling();
        }
    }

    pub fn zoom_camera(&mut self, renderer_id: RendererID, z: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            match &mut r.gpu {
                ManagedRenderer::RendererSDF3D(r) => r.zoom_camera(z),
                ManagedRenderer::Renderer2D(r) => r.zoom_camera(z),
            };
            r.reset_sampling();
        }
    }

    pub fn move_light(&mut self, renderer_id: RendererID, x: f32, y: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.move_light(x, y);
            r.reset_sampling();
        }
    }

    pub fn pan_camera(&mut self, renderer_id: RendererID, x: f32, y: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            match &mut r.gpu {
                ManagedRenderer::RendererSDF3D(r) => r.pan_camera(x, y),
                ManagedRenderer::Renderer2D(r) => r.pan_camera(x, y),
            };
            r.reset_sampling();
        }
    }

    pub fn set_channel(&mut self, renderer_id: RendererID, channel: MaterialChannel) {
        if let Some(r) = self.renderers.get_mut(&renderer_id).and_then(|r| r.as_2d()) {
            r.set_channel(match channel {
                MaterialChannel::Displacement => 0,
                MaterialChannel::Albedo => 1,
                MaterialChannel::Normal => 2,
                MaterialChannel::Roughness => 3,
                MaterialChannel::Metallic => 4,
            });
            r.reset_sampling();
        }
    }

    pub fn switch_object_type(&mut self, renderer_id: RendererID, object_type: ObjectType) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.switch_object_type(object_type)
                .expect("Failed to switch object type");
            r.reset_sampling();
        }
    }

    pub fn set_displacement_amount(&mut self, renderer_id: RendererID, displacement: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_displacement_amount(displacement);
            r.reset_sampling();
        }
    }

    pub fn set_texture_scale(&mut self, renderer_id: RendererID, scale: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_texture_scale(scale);
            r.reset_sampling();
        }
    }

    pub fn set_light_type(&mut self, renderer_id: RendererID, light_type: LightType) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_light_type(light_type);
            r.reset_sampling();
        }
    }

    pub fn set_light_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_light_strength(strength);
            r.reset_sampling();
        }
    }

    pub fn set_fog_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_fog_strength(strength);
            r.reset_sampling();
        }
    }

    pub fn set_environment_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_environment_strength(strength);
            r.reset_sampling();
        }
    }

    pub fn set_environment_blur(&mut self, renderer_id: RendererID, blur: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_environment_blur(blur);
            r.reset_sampling();
        }
    }

    pub fn set_shadow(&mut self, renderer_id: RendererID, shadow: ParameterBool) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_shadow(shadow);
            r.reset_sampling();
        }
    }

    pub fn set_ao(&mut self, renderer_id: RendererID, ao: ParameterBool) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_ao(ao);
            r.reset_sampling();
        }
    }

    pub fn set_focal_length(&mut self, renderer_id: RendererID, focal_length: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_focal_length(focal_length);
            r.reset_sampling();
        }
    }

    pub fn set_focal_distance(&mut self, renderer_id: RendererID, focal_distance: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_focal_distance(focal_distance);
            r.reset_sampling();
        }
    }

    pub fn set_aperture_size(&mut self, renderer_id: RendererID, aperture_size: f32) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.set_aperture_size(aperture_size);
            r.reset_sampling();
        }
    }

    pub fn load_hdri<P: AsRef<std::path::Path>>(&mut self, renderer_id: RendererID, path: P) {
        if let Some(r) = self
            .renderers
            .get_mut(&renderer_id)
            .and_then(|r| r.as_sdf3d())
        {
            r.load_environment(path);
            r.reset_sampling();
        }
    }
}
