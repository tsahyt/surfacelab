use crate::{broker, gpu, lang::*};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod shaders;

// TODO: Image sizes should not be hardcoded!
const IMG_SIZE: u32 = 1024;

pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (sender, receiver) = broker.subscribe();
    match gpu::compute::GPUCompute::new(gpu) {
        Err(e) => {
            log::error!("Failed to initialize GPU Compute: {}", e);
            panic!("Critical Error");
        }
        Ok(gpu) => thread::spawn(move || {
            let mut compute_mgr = ComputeManager::new(gpu);
            for event in receiver {
                match compute_mgr.process_event(event) {
                    None => break,
                    Some(response) => {
                        for ev in response {
                            if let Err(e) = sender.send(ev) {
                                log::error!("Compute lost connection to application bus! {}", e);
                            }
                        }
                    }
                }
            }

            log::info!("GPU Compute Handler terminating");
        }),
    }
}

struct ComputeManager<B: gpu::Backend> {
    gpu: gpu::compute::GPUCompute<B>,
    sockets: HashMap<Resource, gpu::compute::Image<B>>,
    shader_library: shaders::ShaderLibrary<B>,
}

impl<B> ComputeManager<B>
where
    B: gpu::Backend,
{
    pub fn new(gpu: gpu::compute::GPUCompute<B>) -> Self {
        let shader_library = shaders::ShaderLibrary::new(&gpu).unwrap();

        ComputeManager {
            gpu,
            sockets: HashMap::new(),
            shader_library,
        }
    }

    fn add_new_socket(&mut self, socket: Resource, _ty: ImageType) {
        let img = self.gpu.create_compute_image(IMG_SIZE).unwrap();
        self.sockets.insert(socket, img);
    }

    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();
        match &*event {
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, op) => {
                    for (socket, imgtype) in op.inputs().iter().chain(op.outputs().iter()) {
                        let socket_res = res.extend_fragment(&socket);
                        log::trace!("Adding socket {}", socket_res);
                        self.add_new_socket(socket_res, *imgtype);
                    }
                }
                GraphEvent::NodeRemoved(res) => self.sockets.retain(|s, _| !s.is_fragment_of(res)),
                GraphEvent::Recomputed(instrs) => {
                    for i in instrs.iter() {
                        let r = self.interpret(i);
                        if let Err(e) = r {
                            log::error!("Error during compute interpretation: {}", e);
                            log::error!("Aborting compute!");
                            break;
                        }
                    }
                }
                _ => {}
            },
            Lang::UserEvent(UserEvent::Quit) => return None,
            _ => {}
        }

        Some(response)
    }

    fn interpret(&mut self, instr: &Instruction) -> Result<(), String> {
        match instr {
            Instruction::Move(from, to) => {
                log::trace!("Moving texture from {} to {}", from, to);

                debug_assert!(self.sockets.get(from).is_some());
                debug_assert!(self.sockets.get(to).is_some());
                debug_assert!(from != to);

                let source = self.sockets.get(from).unwrap().get_alloc();
                if let Some(alloc) = source {
                    self.sockets.get_mut(to).unwrap().use_memory_from(alloc);
                } else {
                    log::warn!("Tried to move unallocated memory")
                }
            }
            Instruction::Execute(res, op) => {
                log::trace!("Executing operator {:?} of {}", op, res);

                // ensure images are allocated
                for (socket, _) in op.inputs().iter().chain(op.outputs().iter()) {
                    let socket_res = res.extend_fragment(&socket);
                    debug_assert!(self.sockets.get(&socket_res).is_some());
                    self.sockets
                        .get_mut(&socket_res)
                        .unwrap()
                        .ensure_alloc(&self.gpu)?;
                }

                // fill uniforms and execute shader
            }
        }

        Ok(())
    }
}

impl<B> Drop for ComputeManager<B>
where
    B: gpu::Backend,
{
    fn drop(&mut self) {
        self.sockets.clear();
    }
}
