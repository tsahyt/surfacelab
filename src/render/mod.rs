use crate::{broker, gpu, lang::*};
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
                Lang::UIEvent(UIEvent::RendererAdded(h)) => render_manager.new_renderer(h).unwrap(),
                Lang::UIEvent(UIEvent::RendererRedraw) => render_manager.redraw_all(),
                _ => {}
            }
        }

        log::info!("Renderer terminating");
    })
}

struct RenderManager<B: gpu::Backend> {
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    renderers: Vec<gpu::render::GPURender<B>>,
}

impl<B> RenderManager<B>
where
    B: gpu::Backend,
{
    pub fn new(gpu: Arc<Mutex<gpu::GPU<B>>>) -> Self {
        RenderManager {
            gpu,
            renderers: Vec::new(),
        }
    }

    pub fn new_renderer<H: raw_window_handle::HasRawWindowHandle>(
        &mut self,
        handle: &H,
    ) -> Result<(), String> {
        let surface = gpu::render::create_surface(&self.gpu, handle);
        let renderer = gpu::render::GPURender::new(&self.gpu, surface)?;
        self.renderers.push(renderer);

        Ok(())
    }

    pub fn redraw_all(&mut self) {
        for r in self.renderers.iter_mut() {
            r.render()
        }
    }
}
