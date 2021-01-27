use crate::{broker, gpu, lang::*};

use image::{imageops, ImageBuffer, Luma, Rgb, Rgba};
use strum::IntoEnumIterator;

use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

pub mod shaders;
pub mod sockets;

use sockets::*;

/// Start the compute manager in a thread. There should only be one such thread.
pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (sender, receiver, disconnector) = broker.subscribe();
    match gpu::compute::GPUCompute::new(gpu) {
        Err(e) => {
            log::error!("Failed to initialize GPU Compute: {:?}", e);
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

/// An ExternalImage is a wrapper around an aligned buffer containing image data
/// loaded from file or otherwise not coming from procedural computation.
struct ExternalImage {
    buffer: Vec<u16>,
}

#[derive(Debug)]
pub enum InterpretationError {
    /// An error occurred regarding GPU Compute Images
    ImageError(gpu::compute::ImageError),
    /// An error occurred during uploading of an image
    UploadError(gpu::UploadError),
    /// An error occured during pipeline execution,
    PipelineError(gpu::PipelineError),
    /// Failed to read external image
    ExternalImageRead,
    /// Hard OOM, i.e. OOM after cleanup
    HardOOM,
}

impl From<gpu::compute::ImageError> for InterpretationError {
    fn from(e: gpu::compute::ImageError) -> Self {
        InterpretationError::ImageError(e)
    }
}

impl From<gpu::UploadError> for InterpretationError {
    fn from(e: gpu::UploadError) -> Self {
        InterpretationError::UploadError(e)
    }
}

impl From<gpu::PipelineError> for InterpretationError {
    fn from(e: gpu::PipelineError) -> Self {
        InterpretationError::PipelineError(e)
    }
}

#[derive(Debug)]
struct Linearization {
    instructions: Vec<Instruction>,
    use_points: Vec<(Resource<Node>, UsePoint)>,
}

impl Linearization {
    pub fn retention_set_at(&self, step: usize) -> impl Iterator<Item = &Resource<Node>> {
        self.use_points.iter().filter_map(move |(r, up)| {
            if up.last >= step && up.creation <= step {
                Some(r)
            } else {
                None
            }
        })
    }
}

#[derive(Debug)]
struct StackFrame {
    step: usize,
    linearization: Rc<Linearization>,
}

impl StackFrame {
    /// Find the retention set for this stack frame, i.e. the set of images that
    ///
    /// 1. Has a last use point >= current step AND
    /// 2. Has been processed <= current step
    /// 3. Is part of this frame
    pub fn retention_set(&self) -> impl Iterator<Item = &Resource<Node>> {
        self.linearization.retention_set_at(self.step)
    }
}

/// The compute manager is responsible for managing the compute component and
/// processing events from the bus relating to that.
struct ComputeManager<B: gpu::Backend> {
    gpu: gpu::compute::GPUCompute<B>,

    /// Sockets contain all the relevant information for individual node outputs
    /// and inputs.
    sockets: Sockets<B>,

    /// Shader library containing all known shaders
    shader_library: shaders::ShaderLibrary<B>,

    /// Storage for external images
    external_images: HashMap<(std::path::PathBuf, ColorSpace), Option<ExternalImage>>,

    /// Last known linearization of a graph
    linearizations: HashMap<Resource<Graph>, Rc<Linearization>>,

    /// Number of executions, kept for cache invalidation
    seq: u64,

    /// The Compute Manager remembers the hash of the last executed set of
    /// uniforms for each resource. On the next execution this is checked, and
    /// if no changes happen, execution can be skipped entirely.
    last_known: HashMap<Resource<Node>, u64>,

    /// Callstack for complex operator calls
    execution_stack: Vec<StackFrame>,
}

impl<B> ComputeManager<B>
where
    B: gpu::Backend,
{
    /// Initialize a new compute manager.
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
            execution_stack: Vec::new(),
        }
    }

    /// Process one event from the application bus
    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();
        match &*event {
            Lang::LayersEvent(event) => match event {
                LayersEvent::LayerPushed(res, _, _, _, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_node_exists(res, *size);

                    // Blend nodes
                    for channel in MaterialChannel::iter() {
                        let mut blend_node = res.clone();
                        let new_name = format!(
                            "{}.blend.{}",
                            blend_node.file().unwrap(),
                            channel.short_name()
                        );
                        blend_node.rename_file(&new_name);
                        self.sockets.ensure_node_exists(&blend_node, *size);
                    }
                }
                LayersEvent::MaskPushed(_, res, _, _, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_node_exists(res, *size);

                    let mut blend_node = res.clone();
                    let new_name = format!("{}.blend", blend_node.file().unwrap(),);
                    blend_node.rename_file(&new_name);
                    self.sockets.ensure_node_exists(&blend_node, *size);
                }
                LayersEvent::LayersAdded(g, size) => {
                    for channel in MaterialChannel::iter() {
                        self.sockets.ensure_node_exists(
                            &g.graph_node(&format!("output.{}", channel.short_name())),
                            *size,
                        );
                    }
                }
                LayersEvent::LayerRemoved(res) => {
                    for socket in self
                        .sockets
                        .remove_all_for_node(res, &mut self.gpu)
                        .drain(0..)
                    {
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                    }

                    for channel in MaterialChannel::iter() {
                        let mut blend_node = res.clone();
                        let new_name = format!(
                            "{}.blend.{}",
                            blend_node.file().unwrap(),
                            channel.short_name()
                        );
                        blend_node.rename_file(&new_name);

                        for socket in self
                            .sockets
                            .remove_all_for_node(res, &mut self.gpu)
                            .drain(0..)
                        {
                            response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                        }
                    }
                }
                _ => {}
            },
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_node_exists(res, *size);
                }
                GraphEvent::OutputSocketAdded(res, ty, external_data, size) => {
                    match ty {
                        OperatorType::Monomorphic(ty) => {
                            // If the type is monomorphic, we can create the image
                            // right away, otherwise creation needs to be delayed
                            // until the type is known.
                            log::trace!(
                                "Adding monomorphic socket {}, {} external data",
                                res,
                                if *external_data { "with" } else { "without" }
                            );
                            let img = self
                                .gpu
                                .create_compute_image(
                                    self.sockets.get_image_size(&res.socket_node()),
                                    *ty,
                                    *external_data,
                                )
                                .unwrap();
                            self.sockets.add_output_socket(
                                res,
                                Some((img, *ty)),
                                *size,
                                *external_data,
                            );
                            response.push(Lang::ComputeEvent(ComputeEvent::SocketCreated(
                                res.clone(),
                                *ty,
                            )));
                        }
                        OperatorType::Polymorphic(_) => {
                            self.sockets
                                .add_output_socket(res, None, *size, *external_data);
                        }
                    }
                }
                GraphEvent::ComplexOperatorUpdated(node, _, _) => {
                    self.sockets.force(node);
                }
                GraphEvent::NodeRemoved(res) => {
                    for socket in self
                        .sockets
                        .remove_all_for_node(res, &mut self.gpu)
                        .drain(0..)
                    {
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                    }
                }
                GraphEvent::NodeRenamed(from, to) => self.rename(from, to),
                GraphEvent::NodeResized(res, new_size) => {
                    if self.sockets.resize(res, *new_size as u32) {
                        self.sockets
                            .reinit_output_images(res, &self.gpu, *new_size as u32);
                    }
                }
                GraphEvent::Relinearized(graph, instrs, use_points, force_points) => {
                    for fp in force_points {
                        self.sockets.force_all_for_node(fp);
                    }

                    self.linearizations.insert(
                        graph.clone(),
                        Rc::new(Linearization {
                            instructions: instrs.clone(),
                            use_points: use_points.clone(),
                        }),
                    );
                }
                GraphEvent::Recompute(graph) => {
                    debug_assert!(self.execution_stack.is_empty());

                    match self.interpret_linearization(graph, std::iter::empty()) {
                        Err(e) => {
                            self.execution_stack.clear();
                            log::error!("Error during compute interpretation: {:?}", e);
                            log::error!("Aborting compute!");
                        }
                        Ok(r) => {
                            for ev in r {
                                response.push(Lang::ComputeEvent(ev))
                            }
                        }
                    }
                }
                GraphEvent::SocketMonomorphized(res, ty) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Adding monomorphized socket {}", res);
                        // Polymorphic operators never have external data.
                        let img = self
                            .gpu
                            .create_compute_image(
                                self.sockets.get_image_size(&res.socket_node()),
                                *ty,
                                false,
                            )
                            .unwrap();
                        // The socket is a known output, and thus the actual
                        // size should also already be known!
                        self.sockets
                            .add_output_socket(res, Some((img, *ty)), 1024, false);
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketCreated(
                            res.clone(),
                            *ty,
                        )));
                    }
                }
                GraphEvent::SocketDemonomorphized(res) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Removing monomorphized socket {}", res);
                        self.sockets.remove_image(res);
                        let node = res.socket_node();
                        self.sockets.clear_thumbnail(&node, &mut self.gpu);
                        response.push(Lang::ComputeEvent(ComputeEvent::ThumbnailDestroyed(node)));
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(
                            res.clone(),
                        )));
                    }
                }
                GraphEvent::GraphRenamed(from, to) => {
                    self.sockets.rename_graph(from, to);
                    self.linearizations.remove(to);
                }
                _ => {}
            },
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::UserIOEvent(UserIOEvent::OpenSurface(..)) => self.reset(),
            Lang::UserIOEvent(UserIOEvent::NewSurface) => self.reset(),
            Lang::SurfaceEvent(SurfaceEvent::ExportImage(export, size, path)) => match export {
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

    /// Rename a node
    fn rename(&mut self, from: &Resource<Node>, to: &Resource<Node>) {
        // Move last known hash so we can save on a recomputation
        if let Some(h) = self.last_known.remove(from) {
            self.last_known.insert(to.clone(), h);
        }

        // Move sockets
        self.sockets.rename(from, to);
    }

    /// Reset the entire compute manager. This clears all socket data and external images.
    pub fn reset(&mut self) {
        self.sockets.clear(&mut self.gpu);
        self.external_images.clear();
        self.last_known.clear();
    }

    /// Interpret a linearization that is known by the graph name, given some
    /// set of substitutions.
    fn interpret_linearization<'a, I>(
        &mut self,
        graph: &Resource<Graph>,
        substitutions: I,
    ) -> Result<Vec<ComputeEvent>, InterpretationError>
    where
        I: Iterator<Item = &'a ParamSubstitution>,
    {
        self.seq += 1;

        let linearization = self
            .linearizations
            .get(graph)
            .expect("Unknown graph")
            .clone();
        let mut response = Vec::new();

        let mut substitutions_map: HashMap<Resource<Node>, Vec<&ParamSubstitution>> =
            HashMap::new();
        for s in substitutions {
            substitutions_map
                .entry(s.resource().parameter_node())
                .and_modify(|x| x.push(s))
                .or_insert_with(|| vec![s]);
        }

        let mut step: usize = 0;

        for i in linearization.instructions.iter() {
            // Push current step and linearization as frame
            self.execution_stack.push(StackFrame {
                step,
                linearization: linearization.clone(),
            });

            match self.interpret(i, &substitutions_map) {
                Ok(mut r) => response.append(&mut r),

                // Handle OOM
                Err(InterpretationError::ImageError(gpu::compute::ImageError::OutOfMemory)) => {
                    self.cleanup();
                    match self.interpret(i, &substitutions_map) {
                        Ok(mut r) => response.append(&mut r),
                        Err(InterpretationError::ImageError(
                            gpu::compute::ImageError::OutOfMemory,
                        )) => return Err(InterpretationError::HardOOM),
                        e => return e,
                    }
                }
                e => return e,
            }

            // Pop stack
            self.execution_stack.pop();

            if i.is_execution_step() {
                step += 1;
            }
        }

        Ok(response)
    }

    /// Clean up all image data, using the current execution stack to determine what can be cleaned up.
    ///
    /// We can safely clean up an image if
    /// 1. It will no longer be used after this point in a linearization OR
    /// 2. It has not yet been processed at this point in a linearization OR
    /// 3. It is not part of the current execution stack.
    ///
    /// Conversely, we must keep all images that satisfy all three of the following
    /// 1. Has a last use point >= current step AND
    /// 2. Has been processed <= current step AND
    /// 3. Is part of the current execution stack
    ///
    /// The set of images satisfying all these conditions is called the retention set.
    fn cleanup(&mut self) {
        log::debug!("Compute Image cleanup triggered");

        let mut cleanable: HashSet<Resource<Node>> =
            HashSet::from_iter(self.sockets.known_nodes().cloned());

        let execution_stack = &self.execution_stack;

        for n in execution_stack
            .iter()
            .map(|frame| frame.retention_set())
            .flatten()
        {
            cleanable.remove(n);
        }

        for node in cleanable {
            self.sockets.free_images_for_node(&node, &mut self.gpu);
        }
    }

    /// Interpret a single instruction, given a substitution map
    fn interpret(
        &mut self,
        instr: &Instruction,
        substitutions: &HashMap<Resource<Node>, Vec<&ParamSubstitution>>,
    ) -> Result<Vec<ComputeEvent>, InterpretationError> {
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
                    AtomicOperator::Image(Image { path, color_space }) => {
                        self.execute_image(res, &path, color_space)?;
                    }
                    AtomicOperator::Input(..) => {
                        self.execute_input(res)?;
                    }
                    AtomicOperator::Output(output) => {
                        for res in self.execute_output(&output, res) {
                            response.push(res);
                        }
                    }
                    _ => {
                        self.execute_atomic_operator(&op, res)?;
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
                let mut r = self.execute_thumbnail(socket);
                response.append(&mut r);
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
    fn execute_thumbnail(&mut self, socket: &Resource<Socket>) -> Vec<ComputeEvent> {
        let node = socket.socket_node();
        let mut response = Vec::new();
        let updated = self
            .sockets
            .get_output_image_updated(&node)
            .expect("Missing sequence for socket");
        if updated == self.seq {
            log::trace!("Generating thumbnail for {}", socket);
            let ty = self
                .sockets
                .get_output_image_type(socket)
                .expect("Missing output image for socket");
            let new = self
                .sockets
                .ensure_node_thumbnail_exists(&node, ty, &mut self.gpu);
            let thumbnail = self.sockets.get_thumbnail(&node).unwrap();
            let image = self
                .sockets
                .get_output_image(socket)
                .expect("Missing output image for socket");
            self.gpu.generate_thumbnail(image, thumbnail);
            if new {
                response.push(ComputeEvent::ThumbnailCreated(
                    node.clone(),
                    gpu::BrokerImageView::from::<B>(self.gpu.view_thumbnail(thumbnail)),
                ));
            }
            response.push(ComputeEvent::ThumbnailUpdated(node.clone()));
        } else {
            log::trace!("Skipping thumbnail generation");
        }
        response
    }

    /// Execute a call instruction.
    ///
    /// This will recurse into the subgraph, interpret its entire linearization,
    /// and then ensure that all output sockets of the complex operator are
    /// backed by GPU images to make them ready for copying.
    fn execute_call(
        &mut self,
        res: &Resource<Node>,
        op: &ComplexOperator,
    ) -> Result<(), InterpretationError> {
        log::trace!("Calling complex operator of {}", res);

        let uniform_hash = op.parameter_hash();
        let op_seq = self
            .sockets
            .get_output_image_updated(res)
            .expect("Missing sequence for operator");
        let inputs_updated = op.inputs().iter().any(|(socket, _)| {
            let socket_res = res.node_socket(&socket);
            self.sockets
                .get_input_image_updated(&socket_res)
                .expect("Missing input image")
                > op_seq
        });
        match self.last_known.get(res) {
            Some(hash)
                if *hash == uniform_hash && !inputs_updated && !self.sockets.get_force(&res) =>
            {
                log::trace!("Reusing cached images, skipping call");

                let inner_seq = op
                    .outputs
                    .iter()
                    .map(|(_, (_, x))| {
                        self.sockets
                            .get_input_image_updated(&x.node_socket("data"))
                            .unwrap_or(0)
                    })
                    .max()
                    .unwrap_or(0);

                self.sockets
                    .set_output_image_updated(res, self.seq.min(inner_seq));

                return Ok(());
            }
            _ => {}
        };

        let start_time = Instant::now();

        self.interpret_linearization(&op.graph, op.parameters.values())?;

        for (socket, _) in op.outputs().iter() {
            let socket_res = res.node_socket(&socket);
            self.sockets
                .get_output_image_mut(&socket_res)
                .unwrap_or_else(|| panic!("Missing output image for operator {}", res))
                .ensure_alloc(&self.gpu)?;
        }

        self.last_known.insert(res.clone(), uniform_hash);
        self.sockets
            .update_timing_data(res, start_time.elapsed().as_secs_f64());

        Ok(())
    }

    /// Execute a copy instruction.
    ///
    /// *Note*: The source image has to be backed. This is *not* checked and may result
    /// in segfaults or all sorts of nasty behaviour. The target image will be
    /// allocated if not already.
    fn execute_copy(
        &mut self,
        from: &Resource<Socket>,
        to: &Resource<Socket>,
    ) -> Result<(), InterpretationError> {
        let to_seq = self.sockets.get_output_image_updated(&to.socket_node());
        let from_seq = self
            .sockets
            .get_output_image_updated(&from.socket_node())
            .or_else(|| self.sockets.get_input_image_updated(from));

        if to_seq >= from_seq && !self.sockets.get_force(&to.socket_node()) {
            log::trace!("Skipping copy");
        } else {
            log::trace!("Executing copy from {} to {}", from, to);

            self.sockets
                .get_output_image_mut(to)
                .expect("Unable to find source image for copy")
                .ensure_alloc(&self.gpu)?;

            #[allow(clippy::or_fun_call)]
            let from_image = self
                .sockets
                .get_output_image(from)
                .or(self.sockets.get_input_image(from))
                .expect("Unable to find source image for copy");
            let to_image = self
                .sockets
                .get_output_image(to)
                .expect("Unable to find source image for copy");

            self.gpu.copy_image(from_image, to_image);
            self.sockets
                .set_output_image_updated(&to.socket_node(), self.seq);
        }

        Ok(())
    }

    /// Execute an Image operator.
    fn execute_image(
        &mut self,
        res: &Resource<Node>,
        path: &std::path::PathBuf,
        color_space: ColorSpace,
    ) -> Result<(), InterpretationError> {
        log::trace!("Processing Image operator {}", res);

        let parameter_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            color_space.hash(&mut hasher);
            hasher.finish()
        };
        match self.last_known.get(res) {
            Some(hash) if *hash == parameter_hash && !self.sockets.get_force(&res) => {
                log::trace!("Reusing cached image");
                return Ok(());
            }
            _ => {}
        };

        let start_time = Instant::now();
        let image = self
            .sockets
            .get_output_image_mut(&res.node_socket("image"))
            .expect("Trying to process missing socket");

        if let Some(external_image) = self
            .external_images
            .entry((path.clone(), color_space))
            .or_insert_with(|| {
                log::trace!("Loading external image {:?}", path);
                let buf = match color_space {
                    ColorSpace::Srgb => {
                        load_rgba16f_image(path, f16_from_u8_gamma, f16_from_u16_gamma).ok()?
                    }
                    ColorSpace::Linear => {
                        load_rgba16f_image(path, f16_from_u8, f16_from_u16).ok()?
                    }
                };
                Some(ExternalImage { buffer: buf })
            })
        {
            log::trace!("Uploading image to GPU");
            image.ensure_alloc(&self.gpu)?;
            self.gpu.upload_image(&image, &external_image.buffer)?;
            self.last_known.insert(res.clone(), parameter_hash);
            self.sockets.set_output_image_updated(res, self.seq);
            self.sockets
                .update_timing_data(res, start_time.elapsed().as_secs_f64());

            Ok(())
        } else {
            self.sockets
                .update_timing_data(res, start_time.elapsed().as_secs_f64());
            Err(InterpretationError::ExternalImageRead)
        }
    }

    /// Execute an Input operator
    fn execute_input(&mut self, res: &Resource<Node>) -> Result<(), InterpretationError> {
        let start_time = Instant::now();
        let socket_res = res.node_socket("data");
        self.sockets
            .get_output_image_mut(&socket_res)
            .expect("Missing output image on input socket")
            .ensure_alloc(&self.gpu)?;
        self.sockets
            .update_timing_data(res, start_time.elapsed().as_secs_f64());

        Ok(())
    }

    /// Execute an Output operatoro
    ///
    /// Requires the socket to exist and be backed.
    fn execute_output(&mut self, op: &Output, res: &Resource<Node>) -> Vec<ComputeEvent> {
        let output_type = op.output_type;
        let socket_res = res.node_socket("data");

        log::trace!("Processing Output operator {} socket {}", res, socket_res);

        // Ensure socket exists and is backed in debug builds
        debug_assert!(self
            .sockets
            .get_input_image(&socket_res)
            .unwrap()
            .is_backed());

        let ty = self
            .sockets
            .get_input_image_type(&socket_res)
            .expect("Missing image for input socket");
        let new = self
            .sockets
            .ensure_node_thumbnail_exists(&res, ty, &mut self.gpu);
        let image = self.sockets.get_input_image(&socket_res).unwrap();
        let thumbnail = self.sockets.get_thumbnail(&res).unwrap();
        self.gpu.generate_thumbnail(image, thumbnail);

        let mut result = vec![ComputeEvent::OutputReady(
            res.clone(),
            gpu::BrokerImage::from::<B>(image.get_raw()),
            image.get_layout(),
            image.get_access(),
            self.sockets.get_image_size(
                &self
                    .sockets
                    .get_input_resource(&socket_res)
                    .unwrap()
                    .socket_node(),
            ),
            output_type,
        )];

        if new {
            result.push(ComputeEvent::ThumbnailCreated(
                res.clone(),
                gpu::BrokerImageView::from::<B>(self.gpu.view_thumbnail(thumbnail)),
            ));
        }
        result.push(ComputeEvent::ThumbnailUpdated(res.clone()));

        result
    }

    /// Export an RGBA image as given by an array of channel specifications to a certain path.
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

    /// Export an RGB image as given by an array of channel specifications to a certain path.
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

    /// Export a grayscale image as given by an array of channel specifications to a certain path.
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

    /// Executes a single atomic operator.
    ///
    /// For all intents and purposes this is the main workhorse of the compute
    /// component. Requires that all output images are already present, i.e.
    /// exist and are backed.
    ///
    /// Will skip execution if not required.
    fn execute_atomic_operator(
        &mut self,
        op: &AtomicOperator,
        res: &Resource<Node>,
    ) -> Result<(), InterpretationError> {
        use shaders::Uniforms;

        log::trace!("Executing operator {:?} of {}", op, res);

        // Ensure output images are allocated
        for (socket, _) in op.outputs().iter() {
            let socket_res = res.node_socket(&socket);
            self.sockets
                .get_output_image_mut(&socket_res)
                .unwrap_or_else(|| panic!("Missing output image for operator {}", res))
                .ensure_alloc(&self.gpu)?;
        }

        // In debug builds, ensure that all input images exist and are backed
        debug_assert!(op.inputs().iter().all(|(socket, _)| {
            let socket_res = res.node_socket(&socket);
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
            let socket_res = res.node_socket(&socket);
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

        let start_time = Instant::now();

        let mut inputs = HashMap::new();
        for socket in op.inputs().keys() {
            let socket_res = res.node_socket(&socket);
            inputs.insert(
                socket.clone(),
                self.sockets.get_input_image(&socket_res).unwrap(),
            );
        }

        let mut outputs = HashMap::new();
        for socket in op.outputs().keys() {
            let socket_res = res.node_socket(&socket);
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
        self.sockets
            .update_timing_data(res, start_time.elapsed().as_secs_f64());

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

fn f16_from_u8(sample: u8) -> u16 {
    half::f16::from_f32(sample as f32 / 256.0).to_bits()
}

fn f16_from_u16(sample: u16) -> u16 {
    half::f16::from_f32(sample as f32 / 65536.0).to_bits()
}

fn f16_from_u8_gamma(sample: u8) -> u16 {
    half::f16::from_f32((sample as f32 / 256.0).powf(2.2)).to_bits()
}

fn f16_from_u16_gamma(sample: u16) -> u16 {
    half::f16::from_f32((sample as f32 / 65536.0).powf(2.2)).to_bits()
}

/// Load an image from a path into a u16 buffer with f16 encoding, using the
/// provided sampling functions. Those functions can be used to alter each
/// sample if necessary, e.g. to perform gamma correction.
fn load_rgba16f_image<P: AsRef<std::path::Path>, F: Fn(u8) -> u16, G: Fn(u16) -> u16>(
    path: P,
    sample8: F,
    sample16: G,
) -> Result<Vec<u16>, String> {
    use image::GenericImageView;

    let img = image::open(path).map_err(|e| format!("Failed to read image: {}", e))?;

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
