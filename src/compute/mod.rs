use crate::{broker, gpu, lang::*};
use image::{ImageBuffer, Luma, Rgb, Rgba};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod shaders;

// TODO: Image sizes should not be hardcoded! For now they're 1024 everywhere. Probably.
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
    /// allocated, and a counter determining in which execution the image was
    /// most recently updated. Additionally the image type is stored such that
    /// we know it at export time.
    typed_outputs: HashMap<String, (u64, gpu::compute::Image<B>, ImageType)>,

    /// Required to keep track of polymorphic outputs. Kept separately to keep
    /// output_sockets ownership structure simple.
    known_outputs: HashSet<String>,

    /// Input sockets only map to the output sockets they are connected to
    inputs: HashMap<String, Resource>,
    // TODO: look for a way to reference inputs instead of using resources and two lookups
}

struct Sockets<B: gpu::Backend>(HashMap<Resource, SocketData<B>>);

impl<B> Sockets<B>
where
    B: gpu::Backend,
{
    pub fn new() -> Self {
        Sockets(HashMap::new())
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Remove all sockets for the given node.
    ///
    /// The resource *must* point to a node!
    pub fn remove_all_for_node(&mut self, res: &Resource) {
        debug_assert!(res.fragment().is_none());
        self.0.remove(res);
    }

    pub fn add_output_socket(
        &mut self,
        res: &Resource,
        image: Option<(gpu::compute::Image<B>, ImageType)>,
    ) {
        let sockets = self.0.entry(res.drop_fragment()).or_insert(SocketData {
            typed_outputs: HashMap::new(),
            known_outputs: HashSet::new(),
            inputs: HashMap::new(),
        });
        let socket_name = res.fragment().unwrap().to_string();
        if let Some((img, ty)) = image {
            sockets
                .typed_outputs
                .insert(socket_name.clone(), (0, img, ty));
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

    /// Obtain the output image given a socket resource along with its type
    pub fn get_output_image_typed(
        &self,
        res: &Resource,
    ) -> Option<(&gpu::compute::Image<B>, ImageType)> {
        self.0
            .get(&res.drop_fragment())?
            .typed_outputs
            .get(res.fragment().unwrap())
            .map(|x| (&x.1, x.2))
    }

    /// Obtain the output image given a socket resource
    pub fn get_output_image(&self, res: &Resource) -> Option<&gpu::compute::Image<B>> {
        self.get_output_image_typed(res).map(|x| x.0)
    }

    /// Obtain the output image given a socket resource, mutably, along with its type
    pub fn get_output_image_typed_mut(
        &mut self,
        res: &Resource,
    ) -> Option<(&mut gpu::compute::Image<B>, ImageType)> {
        self.0
            .get_mut(&res.drop_fragment())?
            .typed_outputs
            .get_mut(res.fragment().unwrap())
            .map(|x| (&mut x.1, x.2))
    }

    /// Obtain the output image given a socket resource, mutably
    pub fn get_output_image_mut(&mut self, res: &Resource) -> Option<&mut gpu::compute::Image<B>> {
        self.get_output_image_typed_mut(res).map(|x| x.0)
    }

    /// Obtain the input image given a socket resource along with its type
    pub fn get_input_image_typed(
        &self,
        res: &Resource,
    ) -> Option<(&gpu::compute::Image<B>, ImageType)> {
        let sockets = self.0.get(&res.drop_fragment())?;
        let output_res = sockets.inputs.get(res.fragment()?)?;
        self.0
            .get(&output_res.drop_fragment())?
            .typed_outputs
            .get((&output_res).fragment()?)
            .map(|x| (&x.1, x.2))
    }

    /// Obtain the input image given a socket resource
    pub fn get_input_image(&self, res: &Resource) -> Option<&gpu::compute::Image<B>> {
        self.get_input_image_typed(res).map(|x| x.0)
    }

    pub fn get_input_image_updated(&self, res: &Resource) -> Option<u64> {
        let sockets = self.0.get(&res.drop_fragment())?;
        let output_res = sockets.inputs.get(res.fragment()?)?;
        self.0
            .get(&output_res.drop_fragment())?
            .typed_outputs
            .get((&output_res).fragment()?)
            .map(|x| x.0)
    }

    pub fn get_output_image_updated(&mut self, node: &Resource) -> Option<u64> {
        self.0.get(&node).unwrap().typed_outputs.values().map(|x| x.0).max()
    }

    pub fn set_output_image_updated(&mut self, node: &Resource, updated: u64) {
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

    /// Number of executions, kept for cache invalidation
    seq: u64,

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
            seq: 0,
            last_known: HashMap::new(),
        }
    }

    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();
        match &*event {
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, op, _) => {
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
                            self.sockets
                                .add_output_socket(&socket_res, Some((img, *ty)));
                        } else {
                            self.sockets.add_output_socket(&socket_res, None);
                        }
                    }
                }
                GraphEvent::NodeRemoved(res) => self.sockets.remove_all_for_node(res),
                GraphEvent::Recomputed(instrs) => {
                    self.seq += 1;
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
                        self.sockets.add_output_socket(res, Some((img, *ty)));
                    }
                }
                GraphEvent::SocketDemonomorphized(res) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Removing monomorphized socket {}", res);
                        self.sockets.remove_image(res);
                    }
                }
                _ => {}
            },
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::UserIOEvent(UserIOEvent::OpenSurface(..)) => self.reset(),
            Lang::UserIOEvent(UserIOEvent::NewSurface) => self.reset(),
            Lang::UserIOEvent(UserIOEvent::ExportImage(export, path)) => {
                let res = match export {
                    ExportSpec::RGBA(rgba_spec) => self.export_to_rgba(rgba_spec.clone(), path),
                    ExportSpec::RGB(rgb_spec) => self.export_to_rgb(rgb_spec.clone(), path),
                    ExportSpec::Grayscale(gray_spec) => {
                        self.export_to_grayscale(gray_spec.clone(), path)
                    }
                };
                if let Err(e) = res {
                    log::error!("Export failed: {}", e);
                }
            }
            _ => {}
        }

        Some(response)
    }

    pub fn reset(&mut self) {
        self.sockets.clear();
        self.external_images.clear();
        self.last_known.clear();
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
                Operator::Image(Image { path }) => {
                    if let Some(res) = self.execute_image(res, path)? {
                        response.push(res);
                    }
                }
                Operator::Output(..) => {
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
                    self.sockets.set_output_image_updated(res, self.seq);
                    external_image.state = ExternalImageState::Uploaded;
                    true
                }
                ExternalImageState::Uploaded => {
                    self.sockets.set_output_image_updated(res, self.seq);
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
            Operator::Output(Output { output_type }) => output_type,
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

    fn export_to_rgba<P: AsRef<Path>>(
        &mut self,
        spec: [ChannelSpec; 4],
        path: P,
    ) -> Result<(), String> {
        let mut images = HashMap::new();

        for s in &spec {
            #[allow(clippy::or_fun_call)]
            let (image, ty) = self
                .sockets
                .get_input_image_typed(&s.0)
                .or(self.sockets.get_output_image_typed(&s.0))
                .ok_or(format!("Error loading image from resource {}", s.0))?;
            let downloaded = convert_image(&self.gpu.download_image(image)?, ty)?;
            images.insert(s.0.clone(), downloaded);
        }

        let final_image = ImageBuffer::from_fn(IMG_SIZE, IMG_SIZE, |x, y| {
            Rgba([
                images.get(&spec[0].0).unwrap().get_pixel(x, y)[spec[0].1.channel_index()],
                images.get(&spec[1].0).unwrap().get_pixel(x, y)[spec[1].1.channel_index()],
                images.get(&spec[2].0).unwrap().get_pixel(x, y)[spec[2].1.channel_index()],
                images.get(&spec[3].0).unwrap().get_pixel(x, y)[spec[3].1.channel_index()],
            ])
        });

        final_image.save(path).unwrap();

        Ok(())
    }

    fn export_to_rgb<P: AsRef<Path>>(
        &mut self,
        spec: [ChannelSpec; 3],
        path: P,
    ) -> Result<(), String> {
        let mut images = HashMap::new();

        for s in &spec {
            #[allow(clippy::or_fun_call)]
            let (image, ty) = self
                .sockets
                .get_input_image_typed(&s.0)
                .or(self.sockets.get_output_image_typed(&s.0))
                .ok_or(format!("Error loading image from resource {}", s.0))?;
            let downloaded = convert_image(&self.gpu.download_image(image)?, ty)?;
            images.insert(s.0.clone(), downloaded);
        }

        let final_image = ImageBuffer::from_fn(IMG_SIZE, IMG_SIZE, |x, y| {
            Rgb([
                images.get(&spec[0].0).unwrap().get_pixel(x, y)[spec[0].1.channel_index()],
                images.get(&spec[1].0).unwrap().get_pixel(x, y)[spec[1].1.channel_index()],
                images.get(&spec[2].0).unwrap().get_pixel(x, y)[spec[2].1.channel_index()],
            ])
        });

        final_image.save(path).unwrap();

        Ok(())
    }

    fn export_to_grayscale<P: AsRef<Path>>(
        &mut self,
        spec: ChannelSpec,
        path: P,
    ) -> Result<(), String> {
        #[allow(clippy::or_fun_call)]
        let (image, ty) = self
            .sockets
            .get_input_image_typed(&spec.0)
            .or(self.sockets.get_output_image_typed(&spec.0))
            .ok_or(format!("Trying to export non-existent socket {}", spec.0))?;

        let downloaded = convert_image(&self.gpu.download_image(image)?, ty)?;
        let final_image = ImageBuffer::from_fn(IMG_SIZE, IMG_SIZE, |x, y| {
            Luma([downloaded.get_pixel(x, y)[spec.1.channel_index()]])
        });

        final_image.save(path).unwrap();

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
        let op_seq = self.sockets.get_output_image_updated(res).expect("Missing sequence for operator");
        let inputs_updated = op.inputs().iter().any(|(socket, _)| {
            let socket_res = res.extend_fragment(&socket);
            self.sockets
                .get_input_image_updated(&socket_res)
                .expect("Missing input image") > op_seq
        });
        match self.last_known.get(res) {
            Some(hash) if *hash == uniform_hash && !inputs_updated => {
                log::trace!("Reusing cached image");
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
        let descriptors = shaders::ShaderLibrary::write_desc(
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
        let thumbnail = self.gpu.generate_thumbnail(
            outputs
                .values()
                .next()
                .expect("Cannot generate thumbnail for operator without outputs"),
        )?;

        self.last_known.insert(res.clone(), uniform_hash);
        self.sockets.set_output_image_updated(res, self.seq);

        Ok(Some(ComputeEvent::ThumbnailGenerated(
            res.clone(),
            thumbnail,
        )))
    }
}

/// Converts an image from the GPU into a standardized rgba16 image. If the
/// input image type is Rgb, a reverse gamma curve will be applied such that the
/// output image matches what is displayed in the renderers.
fn convert_image(raw: &[u8], ty: ImageType) -> Result<ImageBuffer<Rgba<u16>, Vec<u16>>, String> {
    fn to_16bit(x: f32) -> u16 {
        (x.clamp(0., 1.) * 65535.) as u16
    }

    fn to_16bit_gamma(x: f32) -> u16 {
        (x.powf(1.0 / 2.2).clamp(0., 1.) * 65535.) as u16
    }

    let converted: Vec<u16> = match ty {
        // Underlying memory is formatted as rgba16f
        ImageType::Rgb => unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let u16s: Vec<[u16; 4]> =
                std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                    .chunks(4)
                    .map(|chunk| {
                        [
                            to_16bit_gamma(chunk[0].to_f32()),
                            to_16bit_gamma(chunk[1].to_f32()),
                            to_16bit_gamma(chunk[2].to_f32()),
                            to_16bit(chunk[3].to_f32()),
                        ]
                    })
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u16, u16s.len() * 4).to_owned()
        },
        // Underlying memory is formatted as r32f, using this value for all channels
        ImageType::Grayscale => unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let u16s: Vec<[u16; 4]> =
                std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                    .iter()
                    .map(|x| [to_16bit(*x); 4])
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u16, u16s.len() * 4).to_owned()
        },
    };

    ImageBuffer::from_raw(IMG_SIZE, IMG_SIZE, converted)
        .ok_or_else(|| "Error while creating image buffer".to_string())
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
