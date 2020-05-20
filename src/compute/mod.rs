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

struct SocketData<B: gpu::Backend> {
    /// Output sockets always map to an image, which may or may not be
    /// allocated, and a flag determining whether the image was recently
    /// updated.
    typed_outputs: HashMap<String, (bool, gpu::compute::Image<B>)>,

    /// Required to keep track of polymorphic outputs. Kept separately to keep
    /// output_sockets ownership structure simple.
    known_outputs: HashSet<String>,

    /// Input sockets only map to the output sockets they are connected to
    inputs: HashMap<String, Resource>,
}

struct Sockets<B: gpu::Backend>(HashMap<Resource, SocketData<B>>);

impl<B> Sockets<B>
where
    B: gpu::Backend,
{
    pub fn new() -> Self {
        Sockets(HashMap::new())
    }

    /// Remove all sockets for the given node.
    ///
    /// The resource *must* point to a node!
    pub fn remove_all_for_node(&mut self, res: &Resource) {
        debug_assert!(res.fragment().is_none());
        self.0.remove(res);
    }

    pub fn add_output_socket(&mut self, res: &Resource, image: Option<gpu::compute::Image<B>>) {
        let sockets = self.0.entry(res.drop_fragment()).or_insert(SocketData {
            typed_outputs: HashMap::new(),
            known_outputs: HashSet::new(),
            inputs: HashMap::new(),
        });
        let socket_name = res.fragment().unwrap().to_string();
        if let Some(img) = image {
            sockets
                .typed_outputs
                .insert(socket_name.clone(), (true, img));
        }
        sockets.known_outputs.insert(socket_name);
    }

    /// Determine whether the given resource points to a known output socket.
    pub fn is_known_output(&self, res: &Resource) -> bool {
        debug_assert!(res.fragment().is_some());
        self.0
            .get(&res.drop_fragment())
            .map(|s| s.known_outputs.contains(res.fragment().unwrap()))
            .unwrap_or(false)
    }

    /// Drop the underlying image from an output socket
    pub fn remove_image(&mut self, res: &Resource) {
        let sockets = self
            .0
            .get_mut(&res.drop_fragment())
            .expect("Trying to remove image from unknown resource");
        sockets.typed_outputs.remove(res.fragment().unwrap());
    }

    /// Obtain the output image given a socket resource
    pub fn get_output_image(&self, res: &Resource) -> Option<&gpu::compute::Image<B>> {
        self.0
            .get(&res.drop_fragment())?
            .typed_outputs
            .get(res.fragment().unwrap())
            .map(|x| &x.1)
    }

    /// Obtain the output image given a socket resource, mutably
    pub fn get_output_image_mut(&mut self, res: &Resource) -> Option<&mut gpu::compute::Image<B>> {
        self.0
            .get_mut(&res.drop_fragment())?
            .typed_outputs
            .get_mut(res.fragment().unwrap())
            .map(|x| &mut x.1)
    }

    /// Obtain the input image given a socket resource, mutably
    pub fn get_input_image(&self, res: &Resource) -> Option<&gpu::compute::Image<B>> {
        let sockets = self.0.get(&res.drop_fragment())?;
        let output_res = sockets.inputs.get(res.fragment()?)?;
        self.0
            .get(&output_res.drop_fragment())?
            .typed_outputs
            .get((&output_res).fragment()?)
            .map(|x| &x.1)
    }

    pub fn get_input_image_updated(&self, res: &Resource) -> Option<bool> {
        let sockets = self.0.get(&res.drop_fragment())?;
        let output_res = sockets.inputs.get(res.fragment()?)?;
        self.0
            .get(&output_res.drop_fragment())?
            .typed_outputs
            .get((&output_res).fragment()?)
            .map(|x| x.0)
    }

    pub fn output_image_set_updated(&mut self, node: &Resource, updated: bool) {
        for img in self.0.get_mut(&node).unwrap().typed_outputs.values_mut() {
            img.0 = updated;
        }
    }

    /// connect an output to an input
    pub fn connect_input(&mut self, from: &Resource, to: &Resource) {
        self.0
            .get_mut(&to.drop_fragment())
            .unwrap()
            .inputs
            .insert(to.fragment().unwrap().to_string(), from.to_owned());
    }
}

struct ComputeManager<B: gpu::Backend> {
    gpu: gpu::compute::GPUCompute<B>,

    sockets: Sockets<B>,
    shader_library: shaders::ShaderLibrary<B>,
    external_images: HashMap<std::path::PathBuf, ExternalImage>,

    /// The Compute Manager remembers the hash of the last executed set of
    /// uniforms for each resource. On the next execution this is checked, and
    /// if no changes happen, execution can be skipped entirely.
    last_known: HashMap<Resource, u64>,
}

impl<B> ComputeManager<B>
where
    B: gpu::Backend,
{
    pub fn new(mut gpu: gpu::compute::GPUCompute<B>) -> Self {
        let shader_library = shaders::ShaderLibrary::new(&mut gpu).unwrap();

        ComputeManager {
            gpu,
            sockets: Sockets::new(),
            shader_library,
            external_images: HashMap::new(),
            last_known: HashMap::new(),
        }
    }

    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();
        match &*event {
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, op) => {
                    for (socket, imgtype) in op.inputs().iter().chain(op.outputs().iter()) {
                        let socket_res = res.extend_fragment(&socket);

                        if let OperatorType::Monomorphic(ty) = imgtype {
                            // If the type is monomorphic, we can create the image
                            // right away, otherwise creation needs to be delayed
                            // until the type is known.
                            log::trace!("Adding monomorphic socket {}", socket_res);
                            let img = self
                                .gpu
                                .create_compute_image(IMG_SIZE, *ty, op.external_data())
                                .unwrap();
                            self.sockets.add_output_socket(&socket_res, Some(img));
                        } else {
                            self.sockets.add_output_socket(&socket_res, None);
                        }
                    }
                }
                GraphEvent::NodeRemoved(res) => self.sockets.remove_all_for_node(res),
                GraphEvent::Recomputed(instrs) => {
                    for i in instrs.iter() {
                        match self.interpret(i) {
                            Err(e) => {
                                log::error!("Error during compute interpretation: {}", e);
                                log::error!("Aborting compute!");
                                break;
                            }
                            Ok(r) => {
                                for ev in r {
                                    response.push(Lang::ComputeEvent(ev))
                                }
                            }
                        }
                    }
                }
                GraphEvent::SocketMonomorphized(res, ty) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Adding monomorphized socket {}", res);
                        let img = self.gpu.create_compute_image(IMG_SIZE, *ty, false).unwrap();
                        self.sockets.add_output_socket(res, Some(img));
                    }
                }
                GraphEvent::SocketDemonomorphized(res) => {
                    if self.sockets.is_known_output(res) {
                        self.sockets.remove_image(res);
                    }
                }
                _ => {}
            },
            Lang::UserEvent(UserEvent::Quit) => return None,
            _ => {}
        }

        Some(response)
    }

    fn interpret(&mut self, instr: &Instruction) -> Result<Vec<ComputeEvent>, String> {
        let mut response = Vec::new();

        match instr {
            Instruction::Move(from, to) => {
                log::trace!("Moving texture from {} to {}", from, to);
                debug_assert!(self.sockets.get_output_image(from).is_some());

                self.sockets.connect_input(from, to);
            }
            Instruction::Execute(res, op) => match op {
                Operator::Image { path } => {
                    if let Some(res) = self.execute_image(res, path)? {
                        response.push(res);
                    }
                }
                Operator::Output { .. } => {
                    for res in self.execute_output(op, res)? {
                        response.push(res);
                    }
                }
                _ => {
                    if let Some(res) = self.execute_operator(op, res)? {
                        response.push(res);
                    }
                }
            },
        }

        Ok(response)
    }

    fn execute_image(
        &mut self,
        res: &Resource,
        path: &std::path::PathBuf,
    ) -> Result<Option<ComputeEvent>, String> {
        log::trace!("Processing Image operator {}", res);

        let uploaded = {
            let image = self
                .sockets
                .get_output_image_mut(&res.extend_fragment("image"))
                .expect("Trying to process missing socket");

            let external_image = self.external_images.entry(path.clone()).or_insert_with(|| {
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
                    self.sockets.output_image_set_updated(res, true);
                    external_image.state = ExternalImageState::Uploaded;
                    true
                }
                ExternalImageState::Uploaded => {
                    self.sockets.output_image_set_updated(res, false);
                    log::trace!("Reusing uploaded image");
                    false
                }
            }
        };

        if uploaded {
            let image = self
                .sockets
                .get_output_image(&res.extend_fragment("image"))
                .unwrap();
            let thumbnail = self.gpu.generate_thumbnail(image)?;
            Ok(Some(ComputeEvent::ThumbnailGenerated(
                res.clone(),
                thumbnail,
            )))
        } else {
            Ok(None)
        }
    }

    // HACK: Images sent as OutputReady could technically get dropped before the renderer is done copying them.
    fn execute_output(
        &mut self,
        op: &Operator,
        res: &Resource,
    ) -> Result<Vec<ComputeEvent>, String> {
        let socket = "data";
        let output_type = match op {
            Operator::Output { output_type } => output_type,
            _ => panic!("Output execution on non-output"),
        };

        log::trace!("Processing Output operator {} socket {}", res, socket);

        let socket_res = res.extend_fragment(&socket);

        // Ensure socket exists and is backed in debug builds
        debug_assert!(self
            .sockets
            .get_input_image(&socket_res)
            .unwrap()
            .is_backed());

        let image = self.sockets.get_input_image(&socket_res).unwrap();

        // let raw = self.gpu.download_image(image).unwrap();
        // let ty = op.inputs()[socket]
        //     .monomorphic()
        //     .expect("Output Type must always be monomorphic!");
        //
        let thumbnail = self.gpu.generate_thumbnail(image)?;

        Ok(vec![
            ComputeEvent::OutputReady(
                res.clone(),
                gpu::BrokerImage::from::<B>(image.get_raw()),
                image.get_layout(),
                image.get_access(),
                *output_type,
            ),
            ComputeEvent::ThumbnailGenerated(res.clone(), thumbnail),
        ])
    }

    fn store_image(
        raw: Vec<u8>,
        path: std::path::PathBuf,
        ty: ImageType,
        size: u32,
    ) -> Result<(), String> {
        log::debug!("Downloaded image size {:?}", raw.len());

        thread::spawn(move || {
            let converted = convert_image(&raw, ty);

            log::debug!("Saving converted image");

            let r = image::save_buffer(
                path,
                &converted,
                size,
                size,
                match ty {
                    ImageType::Grayscale => image::ColorType::L16,
                    ImageType::Rgb => image::ColorType::Rgb16,
                },
            );
            match r {
                Err(e) => log::error!("Error saving image: {}", e),
                Ok(_) => log::debug!("Saved image!"),
            };
        });

        Ok(())
    }

    fn execute_operator(
        &mut self,
        op: &Operator,
        res: &Resource,
    ) -> Result<Option<ComputeEvent>, String> {
        use shaders::Uniforms;

        log::trace!("Executing operator {:?} of {}", op, res);

        // Ensure output images are allocated
        for (socket, _) in op.outputs().iter() {
            let socket_res = res.extend_fragment(&socket);
            self.sockets
                .get_output_image_mut(&socket_res)
                .unwrap_or_else(|| panic!("Missing output image for operator {}", res))
                .ensure_alloc(&self.gpu)?;
        }

        // In debug builds, ensure that all input images exist and are backed
        debug_assert!(op.inputs().iter().all(|(socket, _)| {
            let socket_res = res.extend_fragment(&socket);
            let output = self.sockets.get_input_image(&socket_res);
            output.is_some() && output.unwrap().is_backed()
        }));

        // skip execution if neither uniforms nor input changed
        let uniform_hash = op.uniform_hash();
        let inputs_updated = op.inputs().iter().any(|(socket, _)| {
            let socket_res = res.extend_fragment(&socket);
            self.sockets
                .get_input_image_updated(&socket_res)
                .unwrap_or(true)
        });
        match self.last_known.get(res) {
            Some(hash) if *hash == uniform_hash && !inputs_updated => {
                log::trace!("Reusing known image");
                self.sockets.output_image_set_updated(res, false);
                return Ok(None);
            }
            _ => {}
        };

        let mut inputs = HashMap::new();
        for socket in op.inputs().keys() {
            let socket_res = res.extend_fragment(&socket);
            inputs.insert(
                socket.clone(),
                self.sockets.get_input_image(&socket_res).unwrap(),
            );
        }

        let mut outputs = HashMap::new();
        for socket in op.outputs().keys() {
            let socket_res = res.extend_fragment(&socket);
            outputs.insert(
                socket.clone(),
                self.sockets.get_output_image(&socket_res).unwrap(),
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

        // generate thumbnail
        // FIXME: the thumbnail doesn't seem to represent colours correctly. this might be a colourspace issue
        let thumbnail = self.gpu.generate_thumbnail(
            outputs
                .values()
                .next()
                .expect("Cannot generate thumbnail for operator without outputs"),
        )?;

        self.last_known.insert(res.clone(), uniform_hash);
        self.sockets.output_image_set_updated(res, true);

        Ok(Some(ComputeEvent::ThumbnailGenerated(
            res.clone(),
            thumbnail,
        )))
    }
}

fn convert_image(raw: &[u8], ty: ImageType) -> Vec<u8> {
    fn to_16bit(x: f32) -> u16 {
        (x.clamp(0., 1.) * 65535.) as u16
    }

    match ty {
        // Underlying memory is formatted as rgba16f, expected to be Rgb16
        ImageType::Rgb => unsafe {
            #[allow(clippy::cast_ptr_alignment)]
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
            #[allow(clippy::cast_ptr_alignment)]
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
