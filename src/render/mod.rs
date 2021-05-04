use crate::{broker, gpu, lang::*, util::*};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use strum::IntoEnumIterator;

const DEFAULT_SAMPLES: usize = 24;
const DEFAULT_IMAGE_SIZE: u32 = 1024;
const TIMING_DECAY: f64 = 0.85;

/// Start the render thread. This thread manages renderers.
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
                    // Prioritize message if it exists
                    render_manager.step(Some(message))
                } else if render_manager.must_step() {
                    // Otherwise wait a bit and render if there are more samples to do
                    thread::sleep(std::time::Duration::from_millis(5));
                    render_manager.step(None)
                } else {
                    // Otherwise block until there's an event
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

/// Enum wrapping different renderer types, such that they can be stored in the
/// render manager.
enum ManagedRenderer<B: gpu::Backend> {
    RendererSDF3D(gpu::render::RendererSDF3D<B>),
    Renderer2D(gpu::render::Renderer2D<B>),
}

impl<B> ManagedRenderer<B>
where
    B: gpu::Backend,
{
    pub fn serialize_settings(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.serialize_settings(),
            ManagedRenderer::Renderer2D(r) => r.serialize_settings(),
        }
    }

    pub fn deserialize_settings(&mut self, data: &[u8]) -> Result<(), serde_cbor::Error> {
        match self {
            ManagedRenderer::RendererSDF3D(r) => {
                r.deserialize_settings(data)?;
                if let Some(ot) = r.object_type() {
                    r.switch_object_type(ot)
                        .expect("Failed to update object type");
                }
                Ok(())
            }
            ManagedRenderer::Renderer2D(r) => r.deserialize_settings(data),
        }
    }

    /// Run update function on an SDF 3D renderer. A NOP for all other types
    pub fn update_sdf3d<F: Fn(&mut gpu::render::RendererSDF3D<B>) -> ()>(&mut self, f: F) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => f(r),
            ManagedRenderer::Renderer2D(_) => {}
        }
    }

    /// Run update function on a 2D renderer. A NOP for all other types
    pub fn update_2d<F: Fn(&mut gpu::render::Renderer2D<B>) -> ()>(&mut self, f: F) {
        match self {
            ManagedRenderer::RendererSDF3D(_) => {}
            ManagedRenderer::Renderer2D(r) => f(r),
        }
    }

    /// Reset the sampling
    pub fn reset_sampling(&mut self) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.reset_sampling(),
            ManagedRenderer::Renderer2D(r) => r.reset_sampling(),
        }
    }

    /// Instruct GPU to render a frame
    pub fn render(&mut self, image_slots: &gpu::render::ImageSlots<B>) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.render(image_slots),
            ManagedRenderer::Renderer2D(r) => r.render(image_slots),
        }
        .expect("Rendering failed")
    }

    /// Obtain the render target view from the contained renderer
    pub fn target_view(&self) -> &Arc<Mutex<B::ImageView>> {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.target_view(),
            ManagedRenderer::Renderer2D(r) => r.target_view(),
        }
    }

    /// Resize the viewport dimensions in the renderer
    pub fn set_viewport_dimensions(&mut self, width: u32, height: u32) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.set_viewport_dimensions(width, height),
            ManagedRenderer::Renderer2D(r) => r.set_viewport_dimensions(width, height),
        }
    }

    /// Hijack GPU structures of renderer to transfer image. See GPU side
    /// documentation for more details.
    pub fn transfer_image(
        &mut self,
        image_slots: &mut gpu::render::ImageSlots<B>,
        source: &B::Image,
        source_layout: gfx_hal::image::Layout,
        source_access: gfx_hal::image::Access,
        source_size: i32,
        image_use: gpu::render::ImageUse,
    ) {
        match self {
            ManagedRenderer::RendererSDF3D(r) => r.transfer_image(
                image_slots,
                source,
                source_layout,
                source_access,
                source_size,
                image_use,
            ),
            ManagedRenderer::Renderer2D(r) => r.transfer_image(
                image_slots,
                source,
                source_layout,
                source_access,
                source_size,
                image_use,
            ),
        }
    }
}

/// A renderer contains a managed renderer, which is the GPU side component, as
/// well as extra information such as samples left to go and frame timings for
/// statistics output.
struct Renderer<B: gpu::Backend> {
    gpu: ManagedRenderer<B>,
    samples_to_go: usize,
    max_samples: usize,
    frametime_ema: EMA<f64>,
}

impl<B: gpu::Backend> Renderer<B> {
    /// Create a new renderer, given a managed renderer.
    pub fn new(gpu: ManagedRenderer<B>, max_samples: usize) -> Self {
        Self {
            gpu,
            samples_to_go: 0,
            max_samples,
            frametime_ema: EMA::new(TIMING_DECAY),
        }
    }

    /// Reset the sampling of this renderer
    pub fn reset_sampling(&mut self) {
        self.samples_to_go = self.max_samples;
        self.gpu.reset_sampling();
    }
}

/// Renderers dereference to their inner managed renderers
impl<B> std::ops::Deref for Renderer<B>
where
    B: gpu::Backend,
{
    type Target = ManagedRenderer<B>;

    fn deref(&self) -> &Self::Target {
        &self.gpu
    }
}

/// Renderers dereference to their inner managed renderers
impl<B> std::ops::DerefMut for Renderer<B>
where
    B: gpu::Backend,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gpu
    }
}

/// The render manager manages various renderers present in the system,
/// identifier by their RendererID.
struct RenderManager<B: gpu::Backend> {
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    image_slots: gpu::render::ImageSlots<B>,
    renderers: HashMap<RendererID, Renderer<B>>,
}

impl<B> RenderManager<B>
where
    B: gpu::Backend,
{
    /// Spawn a new render manager. No renderers will be registered after creation.
    pub fn new(gpu: Arc<Mutex<gpu::GPU<B>>>) -> Self {
        let image_slots = gpu::render::ImageSlots::new(gpu.clone(), DEFAULT_IMAGE_SIZE)
            .expect("Failed to build image slots");
        RenderManager {
            gpu,
            image_slots,
            renderers: HashMap::new(),
        }
    }

    /// Returns whether any renderer managed by this manager must render another
    /// sample.
    pub fn must_step(&self) -> bool {
        self.renderers.values().any(|r| r.samples_to_go > 0)
    }

    /// Handle the given event and render if appropriate.
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
            Lang::UserIOEvent(UserIOEvent::NewSurface) => {
                self.reset_all();
                response.extend(
                    self.renderers
                        .keys()
                        .map(|r| Lang::RenderEvent(RenderEvent::RendererRedrawn(*r))),
                );
            }
            Lang::IOEvent(IOEvent::RenderSettingsLoaded(data)) => {
                self.renderers
                    .values_mut()
                    .next()?
                    .deserialize_settings(data)
                    .ok()?;
            }
            Lang::UserIOEvent(UserIOEvent::SaveSurface(..)) => {
                let data = self.renderers.values().next()?.serialize_settings().ok()?;
                response.push(Lang::RenderEvent(RenderEvent::Serialized(data)));
            }
            Lang::SurfaceEvent(SurfaceEvent::ParentSizeSet(new_size)) => {
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
            Lang::ComputeEvent(ComputeEvent::SocketViewReady(img, layout, access, size, ty)) => {
                self.transfer_socket_view(img, *layout, *access, *size as i32, *ty);
            }
            Lang::GraphEvent(GraphEvent::OutputRemoved(_res, out_ty)) => {
                use std::convert::TryInto;

                if let Some(img_use) = (*out_ty).try_into().ok() {
                    self.disconnect_image(img_use);
                }

                self.force_redraw_all();

                response.extend(
                    self.renderers
                        .keys()
                        .map(|r| Lang::RenderEvent(RenderEvent::RendererRedrawn(*r))),
                );
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
            Lang::UserRenderEvent(UserRenderEvent::ShadingMode(id, shading_mode)) => {
                self.switch_shading_mode(*id, *shading_mode);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::ToneMap(id, tone_map)) => {
                self.set_tone_map(*id, *tone_map);
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
            Lang::UserRenderEvent(UserRenderEvent::EnvironmentRotation(id, rotation)) => {
                self.set_environment_rotation(*id, *rotation);
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
            Lang::UserRenderEvent(UserRenderEvent::LightSize(id, size)) => {
                self.set_light_size(*id, *size);
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
            Lang::UserRenderEvent(UserRenderEvent::AoStrength(id, ao_strength)) => {
                self.set_ao_strength(*id, *ao_strength);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::LoadHdri(id, Some(path))) => {
                self.load_hdri(*id, path);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::LoadMatcap(id, Some(path))) => {
                self.load_matcap(*id, path);
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
            Lang::UserRenderEvent(UserRenderEvent::ApertureBlades(id, blades)) => {
                self.set_aperture_blades(*id, *blades);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::ApertureRotation(id, rot)) => {
                self.set_aperture_rotation(*id, *rot);
                self.redraw(*id);
                response.push(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)));
            }
            Lang::UserRenderEvent(UserRenderEvent::SampleCount(id, samples)) => {
                self.set_sample_count(*id, *samples as usize);
            }
            Lang::UserRenderEvent(UserRenderEvent::CenterCamera(id)) => {
                self.center_camera(*id);
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
                    )
                    .map_err(|e| format!("{:?}", e))?,
                ),
                DEFAULT_SAMPLES,
            ),
            RendererType::Renderer2D => Renderer::new(
                ManagedRenderer::Renderer2D(
                    gpu::render::GPURender::new_2d(
                        &self.gpu,
                        monitor_dimensions,
                        viewport_dimensions,
                    )
                    .map_err(|e| format!("{:?}", e))?,
                ),
                1,
            ),
        };

        let now = Instant::now();
        renderer.render(&self.image_slots);
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

    pub fn reset_all(&mut self) {
        for output in gpu::render::ImageUse::iter() {
            self.disconnect_image(output);
        }

        self.force_redraw_all();
    }

    pub fn redraw(&mut self, renderer_id: RendererID) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            let now = Instant::now();
            r.render(&self.image_slots);
            r.frametime_ema.update(now.elapsed().as_micros() as f64);
            r.samples_to_go = r.samples_to_go.saturating_sub(1);
        } else {
            log::error!("Trying to redraw on non-existent renderer!");
        }
    }

    pub fn resize_images(&mut self, new_size: u32) {
        let image_slots = gpu::render::ImageSlots::new(self.gpu.clone(), new_size)
            .expect("Failed to build image slots");
        self.image_slots = image_slots;
    }

    pub fn resize(&mut self, renderer_id: RendererID, width: u32, height: u32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_viewport_dimensions(width, height);
            r.reset_sampling();
        }
    }

    pub fn set_sample_count(&mut self, renderer_id: RendererID, samples: usize) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            let previous = r.max_samples;
            r.max_samples = if samples == 0 { usize::MAX } else { samples };
            r.samples_to_go = r.max_samples.saturating_sub(previous);
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
        use std::convert::TryInto;
        log::trace!("Transferring output image for {:?}", output_type);

        if let Some(image_use) = output_type.try_into().ok() {
            if let Some(r) = self.renderers.values_mut().next() {
                match image.clone().to::<B>().and_then(|i| i.upgrade()) {
                    Some(img) => {
                        let image_lock = img.lock().unwrap();
                        r.transfer_image(
                            &mut self.image_slots,
                            &image_lock,
                            layout,
                            access,
                            image_size,
                            image_use,
                        );
                    }
                    None => {
                        log::warn!("Failed to acquire output image for transfer!");
                    }
                }
            }

            for r in self.renderers.values_mut() {
                r.reset_sampling();
            }
        }
    }

    pub fn transfer_socket_view(
        &mut self,
        image: &gpu::BrokerImage,
        layout: gpu::Layout,
        access: gpu::Access,
        image_size: i32,
        image_type: ImageType,
    ) {
        log::trace!("Transferring socket view image");

        if let Some(r) = self.renderers.values_mut().next() {
            match image.clone().to::<B>().and_then(|i| i.upgrade()) {
                Some(img) => {
                    let image_lock = img.lock().unwrap();
                    r.transfer_image(
                        &mut self.image_slots,
                        &image_lock,
                        layout,
                        access,
                        image_size,
                        gpu::render::ImageUse::View(image_type),
                    );
                }
                None => {
                    log::warn!("Failed to acquire output image for transfer!");
                }
            }
        }

        for r in self.renderers.values_mut() {
            r.reset_sampling();
        }
    }

    pub fn force_redraw_all(&mut self) {
        for r in self.renderers.values_mut() {
            r.reset_sampling();
        }

        for r in self.renderers.keys().cloned().collect::<Vec<_>>() {
            self.redraw(r);
        }
    }

    pub fn disconnect_image(&mut self, image_use: gpu::render::ImageUse) {
        self.image_slots.vacate(image_use);
    }

    pub fn rotate_camera(&mut self, renderer_id: RendererID, phi: f32, theta: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.rotate_camera(phi, theta));
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
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.move_light(x, y));
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
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_2d(|r| {
                r.set_channel(match channel {
                    MaterialChannel::Displacement => 0,
                    MaterialChannel::Albedo => 1,
                    MaterialChannel::Normal => 2,
                    MaterialChannel::Roughness => 3,
                    MaterialChannel::Metallic => 4,
                    MaterialChannel::Alpha => 5,
                })
            });
            r.reset_sampling();
        }
    }

    pub fn switch_object_type(&mut self, renderer_id: RendererID, object_type: ObjectType) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| {
                r.switch_object_type(object_type)
                    .expect("Failed to switch object type")
            });
            r.reset_sampling();
        }
    }

    pub fn switch_shading_mode(&mut self, renderer_id: RendererID, shading_mode: ShadingMode) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| {
                r.switch_shading_mode(shading_mode)
                    .expect("Failed to switch shading mode")
            });
            r.reset_sampling();
        }
    }

    pub fn set_tone_map(&mut self, renderer_id: RendererID, tone_map: ToneMap) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            match &mut r.gpu {
                ManagedRenderer::RendererSDF3D(x) => x.set_tone_map(tone_map),
                ManagedRenderer::Renderer2D(x) => x.set_tone_map(tone_map),
            }
            r.reset_sampling();
        }
    }

    pub fn set_displacement_amount(&mut self, renderer_id: RendererID, displacement: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_displacement_amount(displacement));
            r.reset_sampling();
        }
    }

    pub fn set_texture_scale(&mut self, renderer_id: RendererID, scale: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_texture_scale(scale));
            r.reset_sampling();
        }
    }

    pub fn set_light_type(&mut self, renderer_id: RendererID, light_type: LightType) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_light_type(light_type));
            r.reset_sampling();
        }
    }

    pub fn set_light_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_light_strength(strength));
            r.reset_sampling();
        }
    }

    pub fn set_light_size(&mut self, renderer_id: RendererID, size: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_light_size(size));
            r.reset_sampling();
        }
    }

    pub fn set_fog_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_fog_strength(strength));
            r.reset_sampling();
        }
    }

    pub fn set_environment_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_environment_strength(strength));
            r.reset_sampling();
        }
    }

    pub fn set_environment_blur(&mut self, renderer_id: RendererID, blur: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_environment_blur(blur));
            r.reset_sampling();
        }
    }

    pub fn set_environment_rotation(&mut self, renderer_id: RendererID, rotation: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_environment_rotation(rotation));
            r.reset_sampling();
        }
    }

    pub fn set_shadow(&mut self, renderer_id: RendererID, shadow: ParameterBool) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_shadow(shadow));
            r.reset_sampling();
        }
    }

    pub fn set_ao_strength(&mut self, renderer_id: RendererID, ao_strength: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_ao_strength(ao_strength));
            r.reset_sampling();
        }
    }

    pub fn set_focal_length(&mut self, renderer_id: RendererID, focal_length: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_focal_length(focal_length));
            r.reset_sampling();
        }
    }

    pub fn set_focal_distance(&mut self, renderer_id: RendererID, focal_distance: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_focal_distance(focal_distance));
            r.reset_sampling();
        }
    }

    pub fn set_aperture_size(&mut self, renderer_id: RendererID, aperture_size: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_aperture_size(aperture_size));
            r.reset_sampling();
        }
    }

    pub fn set_aperture_blades(&mut self, renderer_id: RendererID, aperture_blades: i32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_aperture_blades(aperture_blades));
            r.reset_sampling();
        }
    }

    pub fn set_aperture_rotation(&mut self, renderer_id: RendererID, aperture_rotation: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.update_sdf3d(|r| r.set_aperture_rotation(aperture_rotation));
            r.reset_sampling();
        }
    }

    pub fn load_hdri<P: AsRef<std::path::Path>>(&mut self, renderer_id: RendererID, path: P) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            if let ManagedRenderer::RendererSDF3D(r) = &mut r.gpu {
                r.load_environment(path).unwrap();
            }
            r.reset_sampling();
        }
    }

    pub fn load_matcap<P: AsRef<std::path::Path>>(&mut self, renderer_id: RendererID, path: P) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            if let ManagedRenderer::RendererSDF3D(r) = &mut r.gpu {
                r.load_matcap(path).unwrap();
            }
            r.reset_sampling();
        }
    }

    pub fn center_camera(&mut self, renderer_id: RendererID) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            match &mut r.gpu {
                ManagedRenderer::RendererSDF3D(r) => r.set_center(0., 0.),
                ManagedRenderer::Renderer2D(r) => r.set_center(0., 0.),
            };
            r.reset_sampling();
        }
    }
}
