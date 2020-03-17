use crate::{broker, gpu, lang::*};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

pub fn start_render_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    let (_sender, receiver) = broker.subscribe();
    thread::spawn(move || {
        log::info!("Starting Renderer");

        let mut render_manager = RenderManager::new(gpu);

        for event in receiver {
            match &*event {
                Lang::UserEvent(UserEvent::Quit) => break,
                Lang::UIEvent(UIEvent::RendererAdded(id, h, width, height)) => render_manager
                    .new_renderer(*id, h, *width, *height)
                    .unwrap(),
                Lang::UIEvent(UIEvent::RendererRedraw(id)) => render_manager.redraw(*id),
                Lang::UIEvent(UIEvent::RendererResize(id, width, height)) => {
                    render_manager.resize(*id, *width, *height)
                }
                Lang::ComputeEvent(ComputeEvent::OutputReady(res, img, layout, out_ty)) => {
                    render_manager.transfer_output(res, img, *layout, *out_ty)
                }
                _ => {}
            }
        }

        log::info!("Renderer terminating");
    })
}

struct RenderManager<B: gpu::Backend> {
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    renderers: HashMap<u64, gpu::render::GPURender<B>>,
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
        id: u64,
        handle: &H,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let surface = gpu::render::create_surface(&self.gpu, handle);
        let renderer = gpu::render::GPURender::new(&self.gpu, surface, width, height)?;
        self.renderers.insert(id, renderer);

        Ok(())
    }

    pub fn redraw_all(&mut self) {
        for r in self.renderers.values_mut() {
            r.render()
        }
    }

    pub fn redraw(&mut self, renderer_id: u64) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.render()
        } else {
            log::error!("Trying to redraw on non-existent renderer!");
        }
    }

    pub fn resize(&mut self, renderer_id: u64, width: u32, height: u32) {
        if let Some(r) = self.renderers.get_mut(&renderer_id) {
            r.set_dimensions(width, height);
            r.recreate_swapchain();
        }
    }

    pub fn transfer_output(
        &mut self,
        _res: &Resource,
        image: &gpu::BrokerImageView,
        layout: gpu::Layout,
        _output_type: OutputType,
    ) {
        for r in self.renderers.values_mut() {
            r.transfer_image(image.to::<B>(), layout).unwrap();
            r.render();
        }
    }
}
