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
    pub fn new(mut gpu: gpu::compute::GPUCompute<B>) -> Self {
        let shader_library = shaders::ShaderLibrary::new(&mut gpu).unwrap();

        ComputeManager {
            gpu,
            sockets: HashMap::new(),
            shader_library,
        }
    }

    fn add_new_socket(&mut self, socket: Resource, ty: ImageType) {
        let px_width = match ty {
            ImageType::Rgb => 8,
            ImageType::Rgba => 8,
            ImageType::Value => 4,
        };
        let img = self.gpu.create_compute_image(IMG_SIZE, px_width).unwrap();
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
        use shaders::Uniforms;
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
                match op {
                    Operator::Image { .. } => {
                        log::trace!("Processing Image operator {}", res);
                    }
                    Operator::Output { .. } => {
                        for (socket, ty) in op.inputs().iter() {
                            log::trace!("Processing Output operator {} socket {}", res, socket);

                            let socket_res = res.extend_fragment(&socket);
                            debug_assert!(self.sockets.get(&socket_res).is_some());
                            let image = self.sockets.get(&socket_res).unwrap();
                            let raw = self.gpu.download_image(image).unwrap();

                            log::debug!("Downloaded image size {:?}", raw.len());

                            let converted = convert_image(&raw, *ty);

                            let path = format!("/tmp/{}.png", res.path().to_str().unwrap());
                            log::debug!("Saving converted image to {}", path);
                            image::save_buffer(
                                path,
                                &converted,
                                IMG_SIZE,
                                IMG_SIZE,
                                match ty {
                                    ImageType::Value => image::ColorType::L16,
                                    ImageType::Rgb => image::ColorType::Rgb16,
                                    ImageType::Rgba => image::ColorType::Rgba16,
                                },
                            )
                            .map_err(|e| format!("Error saving image: {}", e))?;
                            log::debug!("Saved image!");
                        }
                    }
                    _ => {
                        log::trace!("Executing operator {:?} of {}", op, res);

                        // ensure images are allocated and build alloc mapping
                        for (socket, _) in op.inputs().iter().chain(op.outputs().iter()) {
                            let socket_res = res.extend_fragment(&socket);
                            debug_assert!(self.sockets.get(&socket_res).is_some());
                            self.sockets
                                .get_mut(&socket_res)
                                .unwrap()
                                .ensure_alloc(&self.gpu)?;
                        }

                        let mut inputs = HashMap::new();
                        for socket in op.inputs().keys() {
                            let socket_res = res.extend_fragment(&socket);
                            inputs.insert(socket.clone(), self.sockets.get(&socket_res).unwrap());
                        }

                        let mut outputs = HashMap::new();
                        for socket in op.outputs().keys() {
                            let socket_res = res.extend_fragment(&socket);
                            outputs.insert(socket.clone(), self.sockets.get(&socket_res).unwrap());
                        }

                        // fill uniforms and execute shader
                        let pipeline = self.shader_library.pipeline_for(&op);
                        let desc_set = self.shader_library.descriptor_set_for(&op);
                        let uniforms = op.uniforms();
                        let descriptors = shaders::operator_write_desc(
                            op,
                            desc_set,
                            self.gpu.uniform_buffer(),
                            &inputs,
                            &outputs,
                        );

                        self.gpu.fill_uniforms(uniforms)?;
                        self.gpu.write_descriptor_sets(descriptors);
                        self.gpu.run_pipeline(
                            IMG_SIZE,
                            inputs.values().map(|x| *x).collect(),
                            outputs.values().map(|x| *x).collect(),
                            pipeline,
                            desc_set,
                        );
                    }
                }
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

fn convert_image(raw: &[u8], ty: ImageType) -> Vec<u8> {
    match ty {
        // Underlying memory is formatted as rgba16f, expected to be Rgb16
        ImageType::Rgb => unsafe {
            // TODO: sizes?
            let u16s: Vec<[u16; 3]> =
                std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() * 2)
                    .chunks(4)
                    .map(|chunk| {
                        [
                            chunk[0].to_f32() as u16,
                            chunk[1].to_f32() as u16,
                            chunk[2].to_f32() as u16,
                        ]
                    })
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u8, u16s.len() * 2 * 3).to_owned()
        },
        // Underlying memory is formatted as rgba16f, expected to be Rgba16
        ImageType::Rgba => unsafe {
            // TODO: sizes?
            let u16s: Vec<[u16; 4]> =
                std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() * 2)
                    .chunks(4)
                    .map(|chunk| {
                        [
                            chunk[0].to_f32() as u16,
                            chunk[1].to_f32() as u16,
                            chunk[2].to_f32() as u16,
                            chunk[3].to_f32() as u16,
                        ]
                    })
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u8, u16s.len() * 2 * 4).to_owned()
        },
        // Underlying memory is formatted as r32f, expected to be L16
        ImageType::Value => unsafe {
            let u16s: Vec<u16> =
                std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                    .iter()
                    .map(|x| (*x * 65536.) as u16)
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u8, u16s.len() * 2).to_owned()
        },
    }
}
