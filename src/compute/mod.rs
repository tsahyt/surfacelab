use crate::{broker, gpu, lang::*};
use image::{imageops, ImageBuffer, Luma, Rgb, Rgba};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod shaders;

pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (sender, receiver, disconnector) = broker.subscribe();
    match gpu::compute::GPUCompute::new(gpu) {
        Err(e) => {
            log::error!("Failed to initialize GPU Compute: {}", e);
            panic!("Critical Error");
        }
        Ok(gpu) => thread::Builder::new()
            .name("compute".to_string())
            .spawn(move || {
                let mut compute_mgr = ComputeManager::new(gpu);
                for event in receiver {
                    match compute_mgr.process_event(event) {
                        None => break,
                        Some(response) => {
                            for ev in response {
                                if let Err(e) = sender.send(ev) {
                                    log::error!(
                                        "Compute lost connection to application bus! {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                }

                log::info!("GPU Compute Handler terminating");
                disconnector.disconnect();
            })
            .expect("Failed to spawn compute thread!"),
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

struct TypedOutput<B: gpu::Backend> {
    seq: u64,
    image: gpu::compute::Image<B>,
    ty: ImageType,
    force: bool,
}

impl<B> TypedOutput<B>
where
    B: gpu::Backend,
{
    /// Reinitialize the GPU image with a (possibly new) size. This will also
    /// force the image on the next evaluation.
    fn reinit_image(&mut self, gpu: &gpu::compute::GPUCompute<B>, size: u32) {
        self.image = gpu.create_compute_image(size, self.ty, false).unwrap();
        self.force = true;
    }
}

/// Per "node" socket data. Note that we don't really have a notion of node here
/// in the compute component, but this still very closely corresponds to that.
struct SocketData<B: gpu::Backend> {
    /// Output sockets always map to an image, which may or may not be
    /// allocated, and a counter determining in which execution the image was
    /// most recently updated. Additionally the image type is stored such that
    /// we know it at export time.
    typed_outputs: HashMap<String, TypedOutput<B>>,

    /// Required to keep track of polymorphic outputs. Kept separately to keep
    /// output_sockets ownership structure simple.
    known_outputs: HashSet<String>,

    /// The image size of output images for sockets managed here.
    output_size: u32,

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

    pub fn ensure_node_exists(&mut self, res: &Resource, size: u32) -> &mut SocketData<B> {
        self.0.entry(res.drop_fragment()).or_insert(SocketData {
            typed_outputs: HashMap::new(),
            known_outputs: HashSet::new(),
            output_size: size,
            inputs: HashMap::new(),
        })
    }

    pub fn add_output_socket(
        &mut self,
        res: &Resource,
        image: Option<(gpu::compute::Image<B>, ImageType)>,
        size: u32,
    ) {
        let sockets = self.ensure_node_exists(res, size);
        let socket_name = res.fragment().unwrap().to_string();
        if let Some((img, ty)) = image {
            sockets.typed_outputs.insert(
                socket_name.clone(),
                TypedOutput {
                    seq: 0,
                    image: img,
                    ty,
                    force: false,
                },
            );
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

    pub fn reinit_output_images(
        &mut self,
        res: &Resource,
        gpu: &gpu::compute::GPUCompute<B>,
        size: u32,
    ) {
        if let Some(socket_data) = self.0.get_mut(&res) {
            for out in socket_data.typed_outputs.values_mut() {
                out.reinit_image(gpu, size);
            }
        }
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
            .map(|x| (&x.image, x.ty))
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
            .map(|x| (&mut x.image, x.ty))
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
            .map(|x| (&x.image, x.ty))
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
            .map(|x| x.seq)
    }

    pub fn get_output_image_updated(&mut self, node: &Resource) -> Option<u64> {
        self.0
            .get(&node)
            .unwrap()
            .typed_outputs
            .values()
            .map(|x| x.seq)
            .max()
    }

    pub fn get_force(&self, node: &Resource) -> bool {
        self.0
            .get(&node)
            .unwrap()
            .typed_outputs
            .values()
            .any(|x| x.force)
    }

    pub fn set_output_image_updated(&mut self, node: &Resource, updated: u64) {
        for img in self.0.get_mut(&node).unwrap().typed_outputs.values_mut() {
            img.seq = updated;
            img.force = false;
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

    pub fn get_image_size(&self, res: &Resource) -> u32 {
        self.0.get(&res.drop_fragment()).unwrap().output_size
    }

    pub fn get_input_resource(&self, res: &Resource) -> Option<&Resource> {
        let sockets = self.0.get(&res.drop_fragment())?;
        sockets.inputs.get(res.fragment()?)
    }

    /// Rename a resource, moving all its sockets to the new name
    pub fn rename(&mut self, from: &Resource, to: &Resource) {
        if let Some(x) = self.0.remove(from) {
            self.0.insert(to.clone(), x);
        }
    }

    /// Resize outputs
    pub fn resize(&mut self, res: &Resource, new_size: u32) {
        if let Some(x) = self.0.get_mut(res) {
            x.output_size = new_size;
        }
    }
}

struct ComputeManager<B: gpu::Backend> {
    gpu: gpu::compute::GPUCompute<B>,

    /// Sockets contain all the relevant information for individual node outputs
    /// and inputs.
    sockets: Sockets<B>,

    shader_library: shaders::ShaderLibrary<B>,
    external_images: HashMap<std::path::PathBuf, ExternalImage>,

    /// Last known linearization of a graph
    linearizations: HashMap<Resource, Vec<Instruction>>,

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
            linearizations: HashMap::new(),
            seq: 0,
            last_known: HashMap::new(),
        }
    }

    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();
        match &*event {
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, op, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_node_exists(res, *size);

                    // Create (unallocated) compute images if possible for all outputs
                    for (socket, imgtype) in op.outputs().iter() {
                        let socket_res = res.extend_fragment(&socket);

                        if let OperatorType::Monomorphic(ty) = imgtype {
                            // If the type is monomorphic, we can create the image
                            // right away, otherwise creation needs to be delayed
                            // until the type is known.
                            log::trace!("Adding monomorphic socket {}", socket_res);
                            let img = self
                                .gpu
                                .create_compute_image(
                                    self.sockets.get_image_size(res),
                                    *ty,
                                    op.external_data(),
                                )
                                .unwrap();
                            self.sockets
                                .add_output_socket(&socket_res, Some((img, *ty)), *size);
                        } else {
                            self.sockets.add_output_socket(&socket_res, None, *size);
                        }
                    }
                }
                GraphEvent::NodeRemoved(res) => self.sockets.remove_all_for_node(res),
                GraphEvent::NodeRenamed(from, to) => self.rename(from, to),
                GraphEvent::NodeResized(res, new_size) => {
                    self.sockets.resize(res, *new_size as u32);
                    self.sockets
                        .reinit_output_images(res, &mut self.gpu, *new_size as u32);
                }
                GraphEvent::Relinearized(graph, instrs) => {
                    self.linearizations.insert(graph.clone(), instrs.clone());
                }
                GraphEvent::Recompute(graph) => match self.interpret_linearization(graph, &[]) {
                    Err(e) => {
                        log::error!("Error during compute interpretation: {}", e);
                        log::error!("Aborting compute!");
                    }
                    Ok(r) => {
                        for ev in r {
                            response.push(Lang::ComputeEvent(ev))
                        }
                    }
                },
                GraphEvent::SocketMonomorphized(res, ty) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Adding monomorphized socket {}", res);
                        let img = self
                            .gpu
                            .create_compute_image(self.sockets.get_image_size(res), *ty, false)
                            .unwrap();
                        // The socket is a known output, and thus the actual
                        // size should also already be known!
                        // TODO: Rework add_output_socket API
                        self.sockets.add_output_socket(res, Some((img, *ty)), 1024);
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
            Lang::UserIOEvent(UserIOEvent::ExportImage(export, size, path)) => match export {
                ExportSpec::RGBA(rgba_spec) => self.export_to_rgba(rgba_spec.clone(), *size, path),
                ExportSpec::RGB(rgb_spec) => self.export_to_rgb(rgb_spec.clone(), *size, path),
                ExportSpec::Grayscale(gray_spec) => {
                    self.export_to_grayscale(gray_spec.clone(), *size, path)
                }
            },
            _ => {}
        }

        Some(response)
    }

    fn rename(&mut self, from: &Resource, to: &Resource) {
        // Move last known hash so we can save on a recomputation
        if let Some(h) = self.last_known.remove(from) {
            self.last_known.insert(to.clone(), h);
        }

        // Move sockets
        self.sockets.rename(from, to);
    }

    pub fn reset(&mut self) {
        self.sockets.clear();
        self.external_images.clear();
        self.last_known.clear();
    }

    fn interpret_linearization(
        &mut self,
        graph: &Resource,
        substitutions: &[ParamSubstitution],
    ) -> Result<Vec<ComputeEvent>, String> {
        self.seq += 1;
        let instrs = self
            .linearizations
            .get(graph)
            .ok_or_else(|| "Unknown graph")?
            .clone();
        let mut response = Vec::new();

        let mut substitutions_map: HashMap<Resource, Vec<&ParamSubstitution>> = HashMap::new();
        for s in substitutions {
            substitutions_map
                .entry(s.resource.clone())
                .and_modify(|x| x.push(s))
                .or_insert(vec![s]);
        }

        for i in instrs.iter() {
            let mut r = self.interpret(i, &substitutions_map)?;
            response.append(&mut r);
        }

        Ok(response)
    }

    fn interpret(
        &mut self,
        instr: &Instruction,
        substitutions: &HashMap<Resource, Vec<&ParamSubstitution>>,
    ) -> Result<Vec<ComputeEvent>, String> {
        let mut response = Vec::new();

        match instr {
            Instruction::Move(from, to) => {
                log::trace!("Moving texture from {} to {}", from, to);
                debug_assert!(self.sockets.get_output_image(from).is_some());

                self.sockets.connect_input(from, to);
            }
            Instruction::Execute(res, op) => {
                let mut op = op.clone();
                if let Some(subs) = substitutions.get(res) {
                    for s in subs {
                        s.substitute(&mut op);
                    }
                }

                match op {
                    AtomicOperator::Image(Image { path }) => {
                        self.execute_image(res, &path)?;
                    }
                    AtomicOperator::Output(..) => {
                        for res in self.execute_output(&op, res)? {
                            response.push(res);
                        }
                    }
                    _ => {
                        self.execute_operator(&op, res)?;
                    }
                }
            }
            Instruction::Call(res, op) => {
                self.execute_call(res, op)?;
            }
            Instruction::Copy(from, to) => {
                self.execute_copy(from, to)?;
            }
            Instruction::Thumbnail(socket) => {
                if let Some(r) = self.execute_thumbnail(socket)? {
                    response.push(r)
                }
            }
        }

        Ok(response)
    }

    /// Execute a thumbnail instruction.
    ///
    /// This will generate a thumbnail for the given output socket. This assumes the
    /// socket to be valid, allocated, and containing proper data!
    ///
    /// The thumbnail generation will only happen if the output socket has been
    /// updated in the current seq step.
    fn execute_thumbnail(&mut self, socket: &Resource) -> Result<Option<ComputeEvent>, String> {
        let updated = self
            .sockets
            .get_output_image_updated(&socket.drop_fragment())
            .expect("Missing sequence for socket");
        if updated == self.seq {
            log::trace!("Generating thumbnail for {}", socket);
            let image = self
                .sockets
                .get_output_image(socket)
                .expect("Missing output image for socket");
            let thumbnail = self.gpu.generate_thumbnail(image)?;
            Ok(Some(ComputeEvent::ThumbnailGenerated(
                socket.drop_fragment(),
                thumbnail,
            )))
        } else {
            log::trace!("Skipping thumbnail generation");
            Ok(None)
        }
    }

    /// Execute a call instruction.
    ///
    /// This will recurse into the subgraph, interpret its entire linearization,
    /// and then ensure that all output sockets of the complex operator are
    /// backed by GPU images to make them ready for copying.
    fn execute_call(&mut self, res: &Resource, op: &ComplexOperator) -> Result<(), String> {
        log::trace!("Calling complex operator of {}", res);
        self.interpret_linearization(&op.graph, &op.substitutions)?;

        for (socket, _) in op.outputs().iter() {
            let socket_res = res.extend_fragment(&socket);
            self.sockets
                .get_output_image_mut(&socket_res)
                .unwrap_or_else(|| panic!("Missing output image for operator {}", res))
                .ensure_alloc(&self.gpu)?;
        }

        Ok(())
    }

    /// Execute a copy instructions.
    ///
    /// *Note*: Both images have to exist. This is *not* checked and may result
    /// in segfaults or all sorts of nasty behaviour.
    fn execute_copy(&mut self, from: &Resource, to: &Resource) -> Result<(), String> {
        log::trace!("Executing copy from {} to {}", from, to);

        let from_image = self
            .sockets
            .get_output_image(from)
            .or(self.sockets.get_input_image(from))
            .expect("Unable to find source image for copy");
        let to_image = self
            .sockets
            .get_output_image(to)
            .expect("Unable to find source image for copy");

        self.gpu.copy_image(from_image, to_image)?;

        Ok(())
    }

    fn execute_image(&mut self, res: &Resource, path: &std::path::PathBuf) -> Result<(), String> {
        log::trace!("Processing Image operator {}", res);

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
            }
            ExternalImageState::Uploaded => {
                self.sockets.set_output_image_updated(res, self.seq);
                log::trace!("Reusing uploaded image");
            }
        }

        Ok(())
    }

    // NOTE: Images sent as OutputReady could technically get dropped before the
    // renderer is done copying them.
    fn execute_output(
        &mut self,
        op: &AtomicOperator,
        res: &Resource,
    ) -> Result<Vec<ComputeEvent>, String> {
        let socket = "data";
        let output_type = match op {
            AtomicOperator::Output(Output { output_type }) => output_type,
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
                self.sockets
                    .get_image_size(self.sockets.get_input_resource(&socket_res).unwrap()),
                *output_type,
            ),
            ComputeEvent::ThumbnailGenerated(res.clone(), thumbnail),
        ])
    }

    fn export_to_rgba<P: AsRef<Path>>(&mut self, spec: [ChannelSpec; 4], size: u32, path: P) {
        let mut images = HashMap::new();

        for s in &spec {
            let entry = images.entry(s.0.clone());
            entry.or_insert_with(|| {
                #[allow(clippy::or_fun_call)]
                let (image, ty) = self
                    .sockets
                    .get_input_image_typed(&s.0)
                    .or(self.sockets.get_output_image_typed(&s.0))
                    .expect("Trying to export non-existent socket");
                let img_size = image.get_size();
                imageops::resize(
                    &convert_image(&self.gpu.download_image(image).unwrap(), img_size, ty)
                        .expect("Image conversion failed"),
                    size,
                    size,
                    imageops::Triangle,
                )
            });
        }

        let final_image = ImageBuffer::from_fn(size, size, |x, y| {
            Rgba([
                images.get(&spec[0].0).unwrap().get_pixel(x, y)[spec[0].1.channel_index()],
                images.get(&spec[1].0).unwrap().get_pixel(x, y)[spec[1].1.channel_index()],
                images.get(&spec[2].0).unwrap().get_pixel(x, y)[spec[2].1.channel_index()],
                images.get(&spec[3].0).unwrap().get_pixel(x, y)[spec[3].1.channel_index()],
            ])
        });

        final_image.save(path).unwrap();
    }

    fn export_to_rgb<P: AsRef<Path>>(&mut self, spec: [ChannelSpec; 3], size: u32, path: P) {
        let mut images = HashMap::new();

        for s in &spec {
            let entry = images.entry(s.0.clone());
            entry.or_insert_with(|| {
                #[allow(clippy::or_fun_call)]
                let (image, ty) = self
                    .sockets
                    .get_input_image_typed(&s.0)
                    .or(self.sockets.get_output_image_typed(&s.0))
                    .expect("Trying to export non-existent socket");
                let img_size = image.get_size();
                imageops::resize(
                    &convert_image(&self.gpu.download_image(image).unwrap(), img_size, ty)
                        .expect("Image conversion failed"),
                    size,
                    size,
                    imageops::Triangle,
                )
            });
        }

        let final_image = ImageBuffer::from_fn(size, size, |x, y| {
            Rgb([
                images.get(&spec[0].0).unwrap().get_pixel(x, y)[spec[0].1.channel_index()],
                images.get(&spec[1].0).unwrap().get_pixel(x, y)[spec[1].1.channel_index()],
                images.get(&spec[2].0).unwrap().get_pixel(x, y)[spec[2].1.channel_index()],
            ])
        });

        final_image.save(path).unwrap();
    }

    fn export_to_grayscale<P: AsRef<Path>>(&mut self, spec: ChannelSpec, size: u32, path: P) {
        #[allow(clippy::or_fun_call)]
        let (image, ty) = self
            .sockets
            .get_input_image_typed(&spec.0)
            .or(self.sockets.get_output_image_typed(&spec.0))
            .expect("Trying to export non-existent socket {}");
        let img_size = image.get_size();

        let downloaded = imageops::resize(
            &convert_image(&self.gpu.download_image(image).unwrap(), img_size, ty).unwrap(),
            size,
            size,
            imageops::Triangle,
        );
        let final_image = ImageBuffer::from_fn(size, size, |x, y| {
            Luma([downloaded.get_pixel(x, y)[spec.1.channel_index()]])
        });

        final_image.save(path).unwrap();
    }

    fn execute_operator(&mut self, op: &AtomicOperator, res: &Resource) -> Result<(), String> {
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
        let op_seq = self
            .sockets
            .get_output_image_updated(res)
            .expect("Missing sequence for operator");
        let inputs_updated = op.inputs().iter().any(|(socket, _)| {
            let socket_res = res.extend_fragment(&socket);
            self.sockets
                .get_input_image_updated(&socket_res)
                .expect("Missing input image")
                > op_seq
        });
        match self.last_known.get(res) {
            Some(hash)
                if *hash == uniform_hash && !inputs_updated && !self.sockets.get_force(&res) =>
            {
                log::trace!("Reusing cached image");
                return Ok(());
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
            self.sockets.get_image_size(res),
            inputs.values().copied().collect(),
            outputs.values().copied().collect(),
            pipeline,
            desc_set,
        );

        self.last_known.insert(res.clone(), uniform_hash);
        self.sockets.set_output_image_updated(res, self.seq);

        Ok(())
    }
}

/// Converts an image from the GPU into a standardized rgba16 image. If the
/// input image type is Rgb, a reverse gamma curve will be applied such that the
/// output image matches what is displayed in the renderers.
fn convert_image(
    raw: &[u8],
    size: u32,
    ty: ImageType,
) -> Result<ImageBuffer<Rgba<u16>, Vec<u16>>, String> {
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

    ImageBuffer::from_raw(size, size, converted)
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
