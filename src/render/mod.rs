use crate::{broker, gpu, lang::*};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use strum::IntoEnumIterator;

pub fn start_render_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    let (_sender, receiver, disconnector) = broker.subscribe();
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
                    Lang::UIEvent(UIEvent::RendererAdded(id, h, width, height, ty)) => {
                        render_manager
                            .new_renderer(*id, h, *width, *height, *ty)
                            .unwrap()
                    }
                    Lang::UIEvent(UIEvent::RendererRedraw(id)) => render_manager.redraw(*id),
                    Lang::UIEvent(UIEvent::RendererResize(id, width, height)) => {
                        render_manager.resize(*id, *width, *height)
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
                        render_manager.disconnect_output(*out_ty)
                    }
                    Lang::UserRenderEvent(UserRenderEvent::Rotate(id, theta, phi)) => {
                        render_manager.rotate_camera(*id, *theta, *phi)
                    }
                    Lang::UserRenderEvent(UserRenderEvent::Pan(id, x, y)) => {
                        render_manager.pan_camera(*id, *x, *y)
                    }
                    Lang::UserRenderEvent(UserRenderEvent::Zoom(id, z)) => {
                        render_manager.zoom_camera(*id, *z)
                    }
                    Lang::UserRenderEvent(UserRenderEvent::LightMove(id, x, y)) => {
                        render_manager.move_light(*id, *x, *y)
                    }
                    Lang::UserRenderEvent(UserRenderEvent::ChannelChange2D(id, channel)) => {
                        render_manager.set_channel(*id, *channel)
                    }
                    Lang::UserRenderEvent(UserRenderEvent::DisplacementAmount(id, displ)) => {
                        render_manager.set_displacement_amount(*id, *displ)
                    }
                    _ => {}
                }
            }

            log::info!("Renderer terminating");
            disconnector.disconnect();
        })
        .expect("Failed to spawn render thread!")
}

struct RenderManager<B: gpu::Backend> {
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    renderers: HashMap<RendererID, gpu::render::GPURender<B>>,
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

    pub fn new_renderer<H: raw_window_handle::HasRawWindowHandle>(
        &mut self,
        id: RendererID,
        handle: &H,
        width: u32,
        height: u32,
        ty: RendererType,
    ) -> Result<(), String> {
        let surface = gpu::render::create_surface(&self.gpu, handle);
        let renderer = gpu::render::GPURender::new(&self.gpu, surface, width, height, 1024, ty)?;
        self.renderers.insert(id, renderer);

        Ok(())
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
        }
    }

    pub fn resize(&mut self, renderer_id: RendererID, width: u32, height: u32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_dimensions(width, height);
            r.recreate_swapchain();
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
            r.transfer_image(image.to::<B>(), layout, access, image_size, output_type)
                .unwrap();
            r.render();
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
            r.render();
        }
    }

    pub fn zoom_camera(&mut self, renderer_id: RendererID, z: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.zoom_camera(z);
            r.render();
        }
    }

    pub fn move_light(&mut self, renderer_id: RendererID, x: f32, y: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.move_light(x, y);
            r.render();
        }
    }

    pub fn pan_camera(&mut self, renderer_id: RendererID, x: f32, y: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.pan_camera(x, y);
            r.render();
        }
    }

    pub fn set_channel(&mut self, renderer_id: RendererID, channel: RenderChannel) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_channel(match channel {
                RenderChannel::Displacement => 0,
                RenderChannel::Albedo => 1,
                RenderChannel::Normal => 2,
                RenderChannel::Roughness => 3,
                RenderChannel::Metallic => 4,
            });
            r.render();
        }
    }

    pub fn set_displacement_amount(&mut self, renderer_id: RendererID, displacement: f32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_displacement_amount(displacement);
            r.render();
        }
    }
}
