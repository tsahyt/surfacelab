use crate::{broker, gpu, lang::*};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use strum::IntoEnumIterator;

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

            for event in receiver {
                match &*event {
                    Lang::UserIOEvent(UserIOEvent::Quit) => break,
                    Lang::UserIOEvent(UserIOEvent::OpenSurface(..)) => render_manager.reset_all(),
                    Lang::UserIOEvent(UserIOEvent::NewSurface) => render_manager.reset_all(),
                    Lang::UserIOEvent(UserIOEvent::SetParentSize(new_size)) => {
                        render_manager.resize_images(*new_size)
                    }
                    Lang::UIEvent(UIEvent::RendererRequested(id, monitor_size, view_size, ty)) => {
                        let view = render_manager
                            .new_renderer(*id, *monitor_size, *view_size, *ty)
                            .unwrap();
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererAdded(*id, view)))
                            .unwrap();
                    }
                    Lang::UIEvent(UIEvent::RendererRedraw(id)) => render_manager.redraw(*id),
                    Lang::UIEvent(UIEvent::RendererResize(id, width, height)) => {
                        render_manager.resize(*id, *width, *height);
                        render_manager.redraw(*id);
                    }
                    Lang::UIEvent(UIEvent::RendererRemoved(id)) => render_manager.remove(*id),
                    Lang::ComputeEvent(ComputeEvent::OutputReady(
                        _res,
                        img,
                        layout,
                        access,
                        size,
                        out_ty,
                    )) => {
                        render_manager.transfer_output(img, *layout, *access, *size as i32, *out_ty)
                    }
                    Lang::GraphEvent(GraphEvent::OutputRemoved(_res, out_ty)) => {
                        render_manager.disconnect_output(*out_ty);
                    }
                    Lang::UserRenderEvent(UserRenderEvent::Rotate(id, theta, phi)) => {
                        render_manager.rotate_camera(*id, *theta, *phi);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::Pan(id, x, y)) => {
                        render_manager.pan_camera(*id, *x, *y);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::Zoom(id, z)) => {
                        render_manager.zoom_camera(*id, *z);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::LightMove(id, x, y)) => {
                        render_manager.move_light(*id, *x, *y);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::ChannelChange2D(id, channel)) => {
                        render_manager.set_channel(*id, *channel);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::DisplacementAmount(id, displ)) => {
                        render_manager.set_displacement_amount(*id, *displ);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::TextureScale(id, scale)) => {
                        render_manager.set_texture_scale(*id, *scale);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::EnvironmentStrength(id, strength)) => {
                        render_manager.set_environment_strength(*id, *strength);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::LightType(id, light_type)) => {
                        render_manager.set_light_type(*id, *light_type);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::LightStrength(id, strength)) => {
                        render_manager.set_light_strength(*id, *strength);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::FogStrength(id, strength)) => {
                        render_manager.set_fog_strength(*id, *strength);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::SetShadow(id, shadow)) => {
                        render_manager.set_shadow(*id, *shadow);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::SetAO(id, ao)) => {
                        render_manager.set_ao(*id, *ao);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    Lang::UserRenderEvent(UserRenderEvent::LoadHDRI(id, path)) => {
                        render_manager.load_hdri(*id, path);
                        render_manager.redraw(*id);
                        sender
                            .send(Lang::RenderEvent(RenderEvent::RendererRedrawn(*id)))
                            .unwrap();
                    }
                    _ => {}
                }
            }

            log::info!("Renderer terminating");
            disconnector.disconnect();
        })
        .expect("Failed to spawn render thread!")
}

struct Renderer<B: gpu::Backend> {
    gpu: gpu::render::GPURender<B>,
}

impl<B: gpu::Backend> Renderer<B> {
    pub fn new(gpu: gpu::render::GPURender<B>) -> Self {
        Self { gpu }
    }
}

impl<B> std::ops::Deref for Renderer<B>
where
    B: gpu::Backend,
{
    type Target = gpu::render::GPURender<B>;

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

    pub fn new_renderer(
        &mut self,
        id: RendererID,
        monitor_dimensions: (u32, u32),
        viewport_dimensions: (u32, u32),
        ty: RendererType,
    ) -> Result<gpu::BrokerImageView, String> {
        let mut renderer = Renderer::new(
            gpu::render::GPURender::new(
                &self.gpu,
                monitor_dimensions,
                viewport_dimensions,
                1024,
                ty,
            )
            .map_err(|e| format!("{:?}", e))?,
        );
        renderer.render();
        let view = gpu::BrokerImageView::from::<B>(renderer.target_view());
        self.renderers.insert(id, renderer);

        Ok(view)
    }

    pub fn remove(&mut self, renderer_id: RendererID) {
        self.renderers.remove(&renderer_id);
    }

    pub fn redraw_all(&mut self) {
        for r in self.renderers.values_mut() {
            r.render()
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
            r.render()
        } else {
            log::error!("Trying to redraw on non-existent renderer!");
        }
    }

    pub fn resize_images(&mut self, new_size: u32) {
        for r in self.renderers.values_mut() {
            r.recreate_image_slots(new_size)
                .expect("Failed to resize images in renderer");
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
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.rotate_camera(phi, theta);
            r.reset_sampling();
        }
    }

    pub fn zoom_camera(&mut self, renderer_id: RendererID, z: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.zoom_camera(z);
            r.reset_sampling();
        }
    }

    pub fn move_light(&mut self, renderer_id: RendererID, x: f32, y: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.move_light(x, y);
            r.reset_sampling();
        }
    }

    pub fn pan_camera(&mut self, renderer_id: RendererID, x: f32, y: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.pan_camera(x, y);
            r.reset_sampling();
        }
    }

    pub fn set_channel(&mut self, renderer_id: RendererID, channel: MaterialChannel) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
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

    pub fn set_displacement_amount(&mut self, renderer_id: RendererID, displacement: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_displacement_amount(displacement);
            r.reset_sampling();
        }
    }

    pub fn set_texture_scale(&mut self, renderer_id: RendererID, scale: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_texture_scale(scale);
            r.reset_sampling();
        }
    }

    pub fn set_light_type(&mut self, renderer_id: RendererID, light_type: LightType) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_light_type(light_type);
            r.reset_sampling();
        }
    }

    pub fn set_light_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_light_strength(strength);
            r.reset_sampling();
        }
    }

    pub fn set_fog_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_fog_strength(strength);
            r.reset_sampling();
        }
    }

    pub fn set_environment_strength(&mut self, renderer_id: RendererID, strength: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_environment_strength(strength);
            r.reset_sampling();
        }
    }

    pub fn set_shadow(&mut self, renderer_id: RendererID, shadow: ParameterBool) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_shadow(shadow);
            r.reset_sampling();
        }
    }

    pub fn set_ao(&mut self, renderer_id: RendererID, ao: ParameterBool) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_ao(ao);
            r.reset_sampling();
        }
    }

    pub fn load_hdri<P: AsRef<std::path::Path>>(&mut self, renderer_id: RendererID, path: P) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.load_environment(path);
            r.reset_sampling();
        }
    }
}
