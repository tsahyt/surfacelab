use super::shaders::{ShaderLibrary, Uniforms};
use super::sockets::*;
use super::Linearization;
use crate::{gpu, lang::*, util::*};
use std::collections::{HashMap, HashSet, VecDeque};
use std::iter::FromIterator;
use std::rc::Rc;
use std::time::Instant;

pub struct ExternalImage {
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
    /// Call to unknown graph
    UnknownCall,
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
struct StackFrame {
    step: usize,
    instructions: VecDeque<Instruction>,
    linearization: Rc<Linearization>,
    substitutions_map: Rc<HashMap<Resource<Node>, Vec<ParamSubstitution>>>,
}

impl StackFrame {
    pub fn new<'a, I>(linearization: Rc<Linearization>, substitutions: I) -> Self
    where
        I: Iterator<Item = &'a ParamSubstitution>,
    {
        let mut substitutions_map: HashMap<Resource<Node>, Vec<ParamSubstitution>> = HashMap::new();
        for s in substitutions {
            substitutions_map
                .entry(s.resource().parameter_node())
                .and_modify(|x| x.push(s.clone()))
                .or_insert_with(|| vec![s.clone()]);
        }

        Self {
            step: 0,
            instructions: VecDeque::from_iter(linearization.instructions.iter().cloned()),
            linearization: linearization.clone(),
            substitutions_map: Rc::new(substitutions_map),
        }
    }

    /// Find the retention set for this stack frame, i.e. the set of images that
    ///
    /// 1. Has a last use point >= current step AND
    /// 2. Has been processed <= current step
    /// 3. Is part of this frame
    pub fn retention_set(&self) -> impl Iterator<Item = &Resource<Node>> {
        self.linearization.retention_set_at(self.step)
    }
}

/// An interpreter takes a view into the current compute manager state, and runs
/// one interpretation, yielding events through the Iterator instance step by step.
pub struct Interpreter<'a, B: gpu::Backend> {
    /// GPU to run the interpreter on
    gpu: &'a mut gpu::compute::GPUCompute<B>,

    /// Reference to socket data
    sockets: &'a mut Sockets<B>,

    /// Last known uniform hashes
    last_known: &'a mut HashMap<Resource<Node>, u64>,

    /// Storage for external images
    external_images: &'a mut HashMap<(std::path::PathBuf, ColorSpace), Option<ExternalImage>>,

    /// Reference to shader library
    shader_library: &'a ShaderLibrary<B>,

    /// Reference to known linearizations
    linearizations: &'a HashMap<Resource<Graph>, Rc<Linearization>>,

    /// Number of executions, kept for cache invalidation
    seq: u64,

    /// Callstack for complex operator calls
    execution_stack: Vec<StackFrame>,
}

impl<'a, B: gpu::Backend> Interpreter<'a, B> {
    pub fn new(
        gpu: &'a mut gpu::compute::GPUCompute<B>,
        sockets: &'a mut Sockets<B>,
        last_known: &'a mut HashMap<Resource<Node>, u64>,
        external_images: &'a mut HashMap<(std::path::PathBuf, ColorSpace), Option<ExternalImage>>,
        shader_library: &'a ShaderLibrary<B>,
        linearizations: &'a HashMap<Resource<Graph>, Rc<Linearization>>,
        seq: u64,
        graph: &Resource<Graph>,
    ) -> Self {
        let linearization = linearizations.get(graph).expect("Unknown graph").clone();
        let execution_stack = vec![StackFrame::new(linearization, std::iter::empty())];

        Self {
            gpu,
            sockets,
            last_known,
            external_images,
            shader_library,
            linearizations,
            seq: seq + 1,
            execution_stack,
        }
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

        // Push call onto stack
        let frame = StackFrame::new(
            self.linearizations
                .get(&op.graph)
                .ok_or(InterpretationError::UnknownCall)?
                .clone(),
            op.parameters.values(),
        );
        self.execution_stack.push(frame);
        self.seq += 1;

        // Set up outputs for copy back
        for (socket, _) in op.outputs().iter() {
            let socket_res = res.node_socket(&socket);
            self.sockets
                .get_output_image_mut(&socket_res)
                .unwrap_or_else(|| panic!("Missing output image for operator {}", res))
                .ensure_alloc(&self.gpu)?;
        }

        // Write down uniforms and timing data
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
        let descriptors = ShaderLibrary::write_desc(
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
        substitutions: &HashMap<Resource<Node>, Vec<ParamSubstitution>>,
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
}

impl<'a, B> Iterator for Interpreter<'a, B>
where
    B: gpu::Backend,
{
    type Item = Result<(Vec<ComputeEvent>, u64), InterpretationError>;

    fn next(&mut self) -> Option<Self::Item> {
        let frame = self.execution_stack.last_mut()?;
        let instruction = frame.instructions.pop_front().unwrap();
        let substitutions = frame.substitutions_map.clone();
        frame.step += 1;

        let response = match self.interpret(&instruction, &substitutions) {
            Ok(r) => Some(Ok((r, self.seq))),

            // Handle OOM
            Err(InterpretationError::ImageError(gpu::compute::ImageError::OutOfMemory)) => {
                self.cleanup();
                match self.interpret(&instruction, &substitutions) {
                    Ok(r) => Some(Ok((r, self.seq))),
                    Err(InterpretationError::ImageError(gpu::compute::ImageError::OutOfMemory)) => {
                        Some(Err(InterpretationError::HardOOM))
                    }
                    Err(e) => Some(Err(e)),
                }
            }
            Err(e) => Some(Err(e)),
        };

        // Pop frame if we're done here
        if self.execution_stack.last()?.instructions.is_empty() {
            self.execution_stack.pop();
        }

        response
    }
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
