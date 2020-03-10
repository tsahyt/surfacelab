use crate::{broker, gpu, lang::*};
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Clone, Copy)]
enum ExternalImageState {
    Uploaded,
    InMemory,
}

struct ExternalImage {
    state: ExternalImageState,
    buffer: Vec<u16>,
}

struct ComputeManager<B: gpu::Backend> {
    gpu: gpu::compute::GPUCompute<B>,

    /// Output sockets always map to an image, which may or may not be allocated
    output_sockets: HashMap<Resource, gpu::compute::Image<B>>,

    /// Required to keep track of polymorphic outputs. Kept separately to keep
    /// output_sockets ownership structure simple.
    known_output_sockets: HashSet<Resource>,

    /// Input sockets only map to the output sockets they are connected to
    input_sockets: HashMap<Resource, Resource>,

    shader_library: shaders::ShaderLibrary<B>,
    external_images: HashMap<std::path::PathBuf, ExternalImage>,
}

impl<B> ComputeManager<B>
where
    B: gpu::Backend,
{
    pub fn new(mut gpu: gpu::compute::GPUCompute<B>) -> Self {
        let shader_library = shaders::ShaderLibrary::new(&mut gpu).unwrap();

        ComputeManager {
            gpu,
            output_sockets: HashMap::new(),
            known_output_sockets: HashSet::new(),
            input_sockets: HashMap::new(),
            shader_library,
            external_images: HashMap::new(),
        }
    }

    fn add_new_output_socket(&mut self, socket: Resource, ty: ImageType) {
        let img = self.gpu.create_compute_image(IMG_SIZE, ty).unwrap();
        self.output_sockets.insert(socket, img);
    }

    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let response = Vec::new();
        match &*event {
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, op) => {
                    for (socket, imgtype) in op.inputs().iter().chain(op.outputs().iter()) {
                        let socket_res = res.extend_fragment(&socket);

                        // If the type is monomorphic, we can create the socket
                        // right away, otherwise creation needs to be delayed
                        // until the type is known.
                        if let OperatorType::Monomorphic(ty) = imgtype {
                            log::trace!("Adding monomorphic socket {}", socket_res);
                            self.add_new_output_socket(socket_res.clone(), *ty);
                        }

                        self.known_output_sockets.insert(socket_res);
                    }
                }
                GraphEvent::NodeRemoved(res) => {
                    // TODO: directly find and remove sockets
                    self.output_sockets.retain(|s, _| !s.is_fragment_of(res));
                    self.known_output_sockets.retain(|s| !s.is_fragment_of(res));
                }
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
                GraphEvent::SocketMonomorphized(res, ty) => {
                    if self.known_output_sockets.contains(res) {
                        log::trace!("Adding monomorphized socket {}", res);
                        self.add_new_output_socket(res.clone(), *ty)
                    }
                }
                GraphEvent::SocketDemonomorphized(res) => {
                    if self.known_output_sockets.contains(res) {
                        self.output_sockets.remove(res);
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

                debug_assert!(self.output_sockets.get(from).is_some());

                self.input_sockets.insert(to.to_owned(), from.to_owned());
            }
            Instruction::Execute(res, op) => {
                match op {
                    Operator::Image { path } => {
                        log::trace!("Processing Image operator {}", res);

                        let image = self
                            .output_sockets
                            .get_mut(&res.extend_fragment("image"))
                            .expect("Trying to process missing socket");

                        let external_image =
                            self.external_images.entry(path.clone()).or_insert_with(|| {
                                log::trace!("Loading external image {:?}", path);
                                let buf = load_rgba16f_image(path).expect("Failed to read image");
                                ExternalImage {
                                    state: ExternalImageState::InMemory,
                                    buffer: buf,
                                }
                            });

                        match external_image.state {
                            ExternalImageState::InMemory => {
                                log::trace!("Uploading image to GPU");
                                image.ensure_alloc(&self.gpu)?;
                                self.gpu.upload_image(&image, &external_image.buffer)?;
                                external_image.state = ExternalImageState::Uploaded
                            }
                            ExternalImageState::Uploaded => {
                                log::trace!("Reusing uploaded image");
                            }
                        }
                    }
                    Operator::Output { .. } => {
                        for (socket, ty) in op.inputs().iter() {
                            log::trace!("Processing Output operator {} socket {}", res, socket);

                            let socket_res = res.extend_fragment(&socket);

                            // Ensure socket exists and is backed in debug builds
                            debug_assert!({
                                let output_res = self.input_sockets.get(&socket_res).unwrap();
                                self.output_sockets.get(&output_res).unwrap().is_backed()
                            });

                            let image = self
                                .output_sockets
                                .get(self.input_sockets.get(&socket_res).unwrap())
                                .unwrap();
                            let raw = self.gpu.download_image(image).unwrap();

                            log::debug!("Downloaded image size {:?}", raw.len());

                            let ty = ty
                                .monomorphic()
                                .expect("Output Type must always be monomorphic!");
                            let converted = convert_image(&raw, ty);

                            let path = format!("/tmp/{}.png", res.path().to_str().unwrap());
                            log::debug!("Saving converted image to {}", path);
                            image::save_buffer(
                                path,
                                &converted,
                                IMG_SIZE,
                                IMG_SIZE,
                                match ty {
                                    ImageType::Grayscale => image::ColorType::L16,
                                    ImageType::Rgb => image::ColorType::Rgb16,
                                },
                            )
                            .map_err(|e| format!("Error saving image: {}", e))?;
                            log::debug!("Saved image!");
                        }
                    }
                    _ => {
                        log::trace!("Executing operator {:?} of {}", op, res);

                        // Ensure output images are allocated
                        for (socket, _) in op.outputs().iter() {
                            let socket_res = res.extend_fragment(&socket);
                            debug_assert!(self.output_sockets.get(&socket_res).is_some());
                            self.output_sockets
                                .get_mut(&socket_res)
                                .unwrap()
                                .ensure_alloc(&self.gpu)?;
                        }

                        // In debug builds, ensure that all input images exist and are backed
                        debug_assert!(op.inputs().iter().all(|(socket, _)| {
                            let socket_res = res.extend_fragment(&socket);
                            let output_res = self.input_sockets.get(&socket_res).unwrap();
                            let output = self.output_sockets.get(output_res);
                            output.is_some() && output.unwrap().is_backed()
                        }));

                        let mut inputs = HashMap::new();
                        for socket in op.inputs().keys() {
                            let socket_res = res.extend_fragment(&socket);
                            let output_res = self.input_sockets.get(&socket_res).unwrap();
                            inputs.insert(
                                socket.clone(),
                                self.output_sockets.get(output_res).unwrap(),
                            );
                        }

                        let mut outputs = HashMap::new();
                        for socket in op.outputs().keys() {
                            let socket_res = res.extend_fragment(&socket);
                            outputs.insert(
                                socket.clone(),
                                self.output_sockets.get(&socket_res).unwrap(),
                            );
                        }

                        // fill uniforms and execute shader
                        let pipeline = self.shader_library.pipeline_for(&op);
                        let desc_set = self.shader_library.descriptor_set_for(&op);
                        let uniforms = op.uniforms();
                        let descriptors = shaders::operator_write_desc(
                            op,
                            desc_set,
                            self.gpu.uniform_buffer(),
                            self.gpu.sampler(),
                            &inputs,
                            &outputs,
                        );

                        self.gpu.fill_uniforms(uniforms)?;
                        self.gpu.write_descriptor_sets(descriptors);
                        self.gpu.run_pipeline(
                            IMG_SIZE,
                            inputs.values().copied().collect(),
                            outputs.values().copied().collect(),
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

fn convert_image(raw: &[u8], ty: ImageType) -> Vec<u8> {
    fn to_16bit(x: f32) -> u16 {
        (x.clamp(0., 1.) * 65535.) as u16
    }

    match ty {
        // Underlying memory is formatted as rgba16f, expected to be Rgb16
        ImageType::Rgb => unsafe {
            let u16s: Vec<[u16; 3]> =
                std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                    .chunks(4)
                    .map(|chunk| {
                        [
                            to_16bit(chunk[0].to_f32()),
                            to_16bit(chunk[1].to_f32()),
                            to_16bit(chunk[2].to_f32()),
                        ]
                    })
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u8, u16s.len() * 2 * 3).to_owned()
        },
        // Underlying memory is formatted as r32f, expected to be L16
        ImageType::Grayscale => unsafe {
            let u16s: Vec<u16> =
                std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                    .iter()
                    .map(|x| to_16bit(*x))
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u8, u16s.len() * 2).to_owned()
        },
    }
}

fn load_rgba16f_image<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<u16>, String> {
    use image::GenericImageView;

    let img = image::open(path).map_err(|e| format!("Failed to read image: {}", e))?;

    fn sample8(sample: u8) -> u16 {
        half::f16::from_f32(sample as f32 / 256.0).to_bits()
    }

    fn sample16(sample: u16) -> u16 {
        half::f16::from_f32(sample as f32 / 65536.0).to_bits()
    }

    let mut loaded: Vec<u16> = Vec::with_capacity(img.width() as usize * img.height() as usize * 4);

    match img {
        image::DynamicImage::ImageLuma8(buf) => {
            for image::Luma([l]) in buf.pixels() {
                let x = sample8(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(255);
            }
        }
        image::DynamicImage::ImageLumaA8(buf) => {
            for image::LumaA([l, a]) in buf.pixels() {
                let x = sample8(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(sample8(*a));
            }
        }
        image::DynamicImage::ImageRgb8(buf) => {
            for image::Rgb([r, g, b]) in buf.pixels() {
                loaded.push(sample8(*r));
                loaded.push(sample8(*g));
                loaded.push(sample8(*b));
                loaded.push(sample8(255));
            }
        }
        image::DynamicImage::ImageRgba8(buf) => {
            for sample in buf.as_flat_samples().as_slice().iter() {
                loaded.push(sample8(*sample))
            }
        }
        image::DynamicImage::ImageBgr8(buf) => {
            for image::Bgr([b, g, r]) in buf.pixels() {
                loaded.push(sample8(*r));
                loaded.push(sample8(*g));
                loaded.push(sample8(*b));
                loaded.push(sample8(255));
            }
        }
        image::DynamicImage::ImageBgra8(buf) => {
            for image::Bgra([b, g, r, a]) in buf.pixels() {
                loaded.push(sample8(*r));
                loaded.push(sample8(*g));
                loaded.push(sample8(*b));
                loaded.push(sample8(*a));
            }
        }
        image::DynamicImage::ImageLuma16(buf) => {
            for image::Luma([l]) in buf.pixels() {
                let x = sample16(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(255);
            }
        }
        image::DynamicImage::ImageLumaA16(buf) => {
            for image::LumaA([l, a]) in buf.pixels() {
                let x = sample16(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(sample16(*a));
            }
        }
        image::DynamicImage::ImageRgb16(buf) => {
            for image::Rgb([r, g, b]) in buf.pixels() {
                loaded.push(sample16(*r));
                loaded.push(sample16(*g));
                loaded.push(sample16(*b));
                loaded.push(sample16(255));
            }
        }
        image::DynamicImage::ImageRgba16(buf) => {
            for sample in buf.as_flat_samples().as_slice().iter() {
                loaded.push(sample16(*sample))
            }
        }
    }

    Ok(loaded)
}
