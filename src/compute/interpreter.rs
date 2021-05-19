use super::{
    export::*,
    external::*,
    shaders::{BufferDim, IntermediateDataDescription, ShaderLibrary, Uniforms},
    sockets::*,
    Linearization,
};
use crate::{gpu, lang::*};
use itertools::Itertools;
use std::collections::{HashMap, HashSet, VecDeque};
use std::iter::FromIterator;
use std::rc::Rc;
use std::time::Instant;
use thiserror::Error;

const STACK_LIMIT: usize = 256;

#[derive(Debug, Error)]
pub enum InterpretationError {
    /// An error occurred regarding GPU compute memory
    #[error("An error occurred regarding GPU compute memory")]
    AllocatorError(#[from] gpu::compute::AllocatorError),
    /// An error occurred during uploading of an image
    #[error("An error occurred during uploading of an image")]
    UploadError(#[from] gpu::basic_mem::BasicBufferBuilderError),
    /// An error occured during pipeline execution,
    #[error("An error occured during pipeline execution")]
    PipelineError(#[from] gpu::PipelineError),
    /// An error occured relating to external data,
    #[error("Failed to read external data")]
    ExternalDataError(#[from] ExternalError),
    /// External data not found
    #[error("External data not found")]
    ExternalDataNotFound,
    /// Hard OOM, i.e. OOM after cleanup
    #[error("Hard out of memory condition encountered")]
    HardOOM,
    /// Call to unknown graph
    #[error("Call to unknown graph attempted")]
    UnknownCall,
    /// Failed to find shader for operator
    #[error("Missing shader in shader library")]
    MissingShader,
    /// Stack size limit reached during execution, likely the result of recursion
    #[error("Stack limit reached")]
    StackLimitReached,
    /// Recursion detected during execution
    #[error("Recursion detected during execution")]
    RecursionDetected,
    /// Error during image download while exporting
    #[error("Image download failed. {0}")]
    DownloadError(#[from] gpu::DownloadError),
    /// Error occurred during export
    #[error("Error during export: {0}")]
    ExportError(#[from] ExportError),
}

#[derive(Debug)]
struct StackFrame {
    step: usize,
    graph: Resource<Graph>,
    instructions: VecDeque<Instruction>,
    linearization: Rc<Linearization>,
    substitutions_map: Rc<HashMap<Resource<Node>, Vec<ParamSubstitution>>>,
    start_time: Instant,
    caller: Option<Resource<Node>>,
    frame_size: u32,
}

impl StackFrame {
    pub fn new<'a, I>(
        graph: Resource<Graph>,
        linearization: Rc<Linearization>,
        substitutions: I,
        caller: Option<Resource<Node>>,
        frame_size: u32,
    ) -> Option<Self>
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

        let instructions: VecDeque<_> = linearization
            .instructions
            .iter()
            .filter(|i| caller.is_none() || !i.is_call_skippable())
            .cloned()
            .collect();
        if instructions.is_empty() {
            return None;
        }

        Some(Self {
            step: 0,
            graph,
            instructions,
            linearization,
            substitutions_map: Rc::new(substitutions_map),
            start_time: Instant::now(),
            caller,
            frame_size,
        })
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

    /// Storage for external data
    external_data: &'a mut Externals,

    /// Reference to shader library
    shader_library: &'a ShaderLibrary<B>,

    /// Reference to known linearizations
    linearizations: &'a HashMap<Resource<Graph>, Rc<Linearization>>,

    /// Number of executions, kept for cache invalidation
    seq: u64,

    /// Callstack for complex operator calls
    execution_stack: Vec<StackFrame>,

    /// Parent size
    parent_size: u32,

    /// View socket
    view_socket: &'a mut Option<(Resource<Socket>, u64)>,

    /// Export specs relevant to the interpreter
    export_specs: &'a HashMap<Resource<Node>, &'a (ExportSpec, std::path::PathBuf)>,
}

impl<'a, B: gpu::Backend> Interpreter<'a, B> {
    pub fn new(
        gpu: &'a mut gpu::compute::GPUCompute<B>,
        sockets: &'a mut Sockets<B>,
        external_data: &'a mut Externals,
        shader_library: &'a ShaderLibrary<B>,
        linearizations: &'a HashMap<Resource<Graph>, Rc<Linearization>>,
        seq: u64,
        graph: &Resource<Graph>,
        parent_size: u32,
        view_socket: &'a mut Option<(Resource<Socket>, u64)>,
        export_specs: &'a HashMap<Resource<Node>, &'a (ExportSpec, std::path::PathBuf)>,
    ) -> Result<Self, InterpretationError> {
        let linearization = linearizations
            .get(graph)
            .ok_or(InterpretationError::UnknownCall)?
            .clone();
        let execution_stack = std::iter::once(StackFrame::new(
            graph.clone(),
            linearization,
            std::iter::empty(),
            None,
            parent_size,
        ))
        .flatten()
        .collect();

        Ok(Self {
            gpu,
            sockets,
            external_data,
            shader_library,
            linearizations,
            seq: seq + 1,
            execution_stack,
            parent_size,
            view_socket,
            export_specs,
        })
    }

    /// Execute a thumbnail instruction.
    ///
    /// This will generate a thumbnail for the given output socket. This assumes the
    /// socket to be valid, allocated, and containing proper data!
    ///
    /// The thumbnail generation will only happen if the output socket has been
    /// updated after the last recorded thumbnail update.
    fn execute_thumbnail(&mut self, socket: &Resource<Socket>) -> Vec<ComputeEvent> {
        debug_assert!(self.sockets.get_output_image(&socket).unwrap().is_backed());

        let node = socket.socket_node();
        let mut response = Vec::new();

        let thumbnail_updated = self
            .sockets
            .get_thumbnail_updated(&node)
            .expect("Missing sequence for thumbnail");
        let socket_updated = self
            .sockets
            .get_output_images_updated(&socket.socket_node())
            .expect("Missing sequence for socket");
        if thumbnail_updated <= socket_updated {
            log::trace!("Generating thumbnail for {}", socket);
            let ty = self
                .sockets
                .get_output_image_type(socket)
                .expect("Missing output image for socket");
            let new = self
                .sockets
                .ensure_group_thumbnail_exists(&node, ty, &mut self.gpu);
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

            self.sockets.set_thumbnail_updated(&node, self.seq);
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
        frame_size: u32,
        res: &Resource<Node>,
        op: &ComplexOperator,
    ) -> Result<(), InterpretationError> {
        log::trace!("Calling complex operator of {}", res);

        let uniform_hash = op.parameter_hash();
        if !self.sockets.group_requires_recompute(res, uniform_hash) {
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
                .set_output_images_updated(res, self.seq.min(inner_seq));

            return Ok(());
        }

        // Ensure socket group is well sized
        let op_size = self.sockets.get_image_size_mut(res);
        if op_size.ensure_allocation_size(self.parent_size, frame_size) {
            let new_size = op_size.allocation_size();
            self.sockets.reinit_output_images(res, self.gpu, new_size);
        }

        let size = self.sockets.get_image_size(res).allocation_size();

        // Set up outputs for copy back
        for (socket, _) in op.outputs().iter() {
            let socket_res = res.node_socket(&socket);
            self.sockets
                .get_output_image_mut(&socket_res)
                .unwrap_or_else(|| panic!("Missing output image for operator {}", res))
                .ensure_alloc()?;
        }

        // Push call onto stack
        if let Some(frame) = StackFrame::new(
            op.graph.clone(),
            self.linearizations
                .get(&op.graph)
                .ok_or(InterpretationError::UnknownCall)?
                .clone(),
            op.parameters.values(),
            Some(res.clone()),
            size,
        ) {
            if self
                .execution_stack
                .iter()
                .find(|frame| frame.graph == op.graph)
                .is_some()
            {
                return Err(InterpretationError::RecursionDetected);
            }

            self.execution_stack.push(frame);
            self.seq += 1;
        }

        // Write down uniforms and timing data. Output update happens on copy later.
        self.sockets.set_last_hash(&res, uniform_hash);

        Ok(())
    }

    /// Execute a copy instruction.
    ///
    /// *Note*: The source image has to be backed. The target image will be
    /// allocated if not already.
    fn execute_copy(
        &mut self,
        from: &Resource<Socket>,
        to: &Resource<Socket>,
    ) -> Result<(), InterpretationError> {
        // Potentially too restrictive, because it always runs even when the
        // copy will be skipped anyway.
        debug_assert!(self
            .sockets
            .get_output_image(&from)
            .or_else(|| self.sockets.get_input_image(&from))
            .unwrap()
            .is_backed());

        let to_seq = self.sockets.get_output_images_updated(&to.socket_node());
        let from_seq = self
            .sockets
            .get_input_image_updated(from)
            .or_else(|| self.sockets.get_output_images_updated(&from.socket_node()));

        if to_seq > from_seq && !self.sockets.get_force(&to.socket_node()) {
            log::trace!("Skipping copy");
        } else {
            log::trace!("Executing copy from {} to {}", from, to);

            self.sockets
                .get_output_image_mut(to)
                .expect("Unable to find source image for copy")
                .ensure_alloc()?;

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
                .set_output_images_updated(&to.socket_node(), self.seq);
        }

        Ok(())
    }

    /// Execute an Image operator.
    fn execute_image(
        &mut self,
        res: &Resource<Node>,
        image_res: &Resource<Img>,
    ) -> Result<(), InterpretationError> {
        log::trace!("Processing Image operator {}", res);

        let parameter_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            image_res.hash(&mut hasher);
            hasher.finish()
        };

        if !self.sockets.group_requires_recompute(res, parameter_hash)
            && !self
                .external_data
                .get_image(image_res)
                .map(|i| i.needs_loading())
                .unwrap_or(false)
        {
            log::trace!("Reusing cached image");
            return Ok(());
        }

        let start_time = Instant::now();
        if let Some(external_image) = self.external_data.get_image_mut(image_res) {
            log::trace!("Uploading image to GPU");
            let (buf, size) = external_image.ensure_loaded()?;
            let socket_res = res.node_socket("image");

            // Resize socket if necessary
            if self.sockets.resize(&res, size, false) {
                self.sockets.reinit_output_images(&res, self.gpu, size);
            }

            let image = self
                .sockets
                .get_output_image_mut(&socket_res)
                .expect("Trying to process missing socket");
            image.ensure_alloc()?;

            self.gpu.upload_image(&image, buf)?;
            self.sockets.set_last_hash(res, parameter_hash);
            self.sockets.set_output_images_updated(res, self.seq);
            self.sockets
                .update_timing_data(res, start_time.elapsed().as_secs_f64());

            Ok(())
        } else {
            self.sockets
                .update_timing_data(res, start_time.elapsed().as_secs_f64());
            Err(InterpretationError::ExternalDataNotFound)
        }
    }

    /// Execute an SVG operator.
    fn execute_svg(
        &mut self,
        res: &Resource<Node>,
        svg_res: &Resource<resource::Svg>,
    ) -> Result<(), InterpretationError> {
        log::trace!("Processing SVG operator {}", res);

        let parameter_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            svg_res.hash(&mut hasher);
            hasher.finish()
        };

        if !self.sockets.group_requires_recompute(res, parameter_hash)
            && !self
                .external_data
                .get_svg(svg_res)
                .map(|i| i.needs_loading())
                .unwrap_or(false)
        {
            log::trace!("Reusing cached image");
            return Ok(());
        }

        let start_time = Instant::now();
        if let Some(external_image) = self.external_data.get_svg_mut(svg_res) {
            log::trace!("Rasterising and uploading image to GPU");
            let (buf, size) = external_image.ensure_loaded()?;
            let socket_res = res.node_socket("image");

            // Resize socket if necessary
            if self.sockets.resize(&res, size, false) {
                self.sockets.reinit_output_images(&res, self.gpu, size);
            }

            let image = self
                .sockets
                .get_output_image_mut(&socket_res)
                .expect("Trying to process missing socket");
            image.ensure_alloc()?;

            self.gpu.upload_image(&image, buf)?;
            self.sockets.set_last_hash(res, parameter_hash);
            self.sockets.set_output_images_updated(res, self.seq);
            self.sockets
                .update_timing_data(res, start_time.elapsed().as_secs_f64());

            Ok(())
        } else {
            self.sockets
                .update_timing_data(res, start_time.elapsed().as_secs_f64());
            Err(InterpretationError::ExternalDataNotFound)
        }
    }

    /// Execute an Input operator
    fn execute_input(&mut self, res: &Resource<Node>) -> Result<(), InterpretationError> {
        log::trace!("Processing Input operator at {}", res);

        let start_time = Instant::now();
        let socket_res = res.node_socket("data");
        self.sockets
            .get_output_image_mut(&socket_res)
            .expect("Missing output image on input socket")
            .ensure_alloc()?;
        self.sockets
            .update_timing_data(res, start_time.elapsed().as_secs_f64());

        Ok(())
    }

    /// Execute an Output operator
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

        // Potentially skip output processing
        let uniform_hash = op.uniform_hash();
        if !self.sockets.group_requires_recompute(res, uniform_hash) {
            log::trace!("Skipping output processing");
            return Vec::new();
        }

        self.sockets.set_last_hash(res, uniform_hash);
        self.sockets.set_output_images_updated(res, self.seq);

        let ty = op.output_type.into();
        let new = self
            .sockets
            .ensure_group_thumbnail_exists(&res, ty, &mut self.gpu);
        let image = self.sockets.get_input_image(&socket_res).unwrap();

        let thumbnail = self.sockets.get_thumbnail(&res).unwrap();
        self.gpu.generate_thumbnail(image, thumbnail);

        let mut result = vec![ComputeEvent::OutputReady(
            res.clone(),
            gpu::BrokerImage::from::<B>(image.get_raw()),
            image.get_layout(),
            image.get_access(),
            self.sockets
                .get_image_size(
                    &self
                        .sockets
                        .get_input_resource(&socket_res)
                        .unwrap()
                        .socket_node(),
                )
                .allocation_size(),
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
        frame_size: u32,
        op: &AtomicOperator,
        res: &Resource<Node>,
    ) -> Result<(), InterpretationError> {
        log::trace!("Executing operator {:?} of {}", op, res);

        // Ensure socket group is well sized
        let op_size = self.sockets.get_image_size_mut(res);
        if op_size.ensure_allocation_size(self.parent_size, frame_size) {
            let new_size = op_size.allocation_size();
            self.sockets.reinit_output_images(res, self.gpu, new_size);
        }

        // Ensure output images are allocated
        for (socket, _) in op.outputs().iter() {
            let socket_res = res.node_socket(&socket);
            debug_assert!(self.sockets.get_output_image(&socket_res).is_some());
            self.sockets
                .get_output_image_mut(&socket_res)
                .unwrap()
                .ensure_alloc()?;
        }

        // In debug builds, ensure that all mandatory input images exist and are backed
        debug_assert!(op.inputs().iter().all(|(socket, (_, optional))| {
            let socket_res = res.node_socket(&socket);
            let output = self.sockets.get_input_image(&socket_res);
            *optional || (output.is_some() && output.unwrap().is_backed())
        }));

        // Potentially skip execution if group recompute is not required
        let uniform_hash = op.uniform_hash();
        if !self.sockets.group_requires_recompute(res, uniform_hash) {
            log::trace!("Reusing cached image");
            return Ok(());
        }

        let start_time = Instant::now();

        // Get inputs and outputs
        let sockets = &self.sockets;
        let inputs: HashMap<_, _> = op
            .inputs()
            .keys()
            .filter_map(|socket| {
                Some((
                    socket.clone(),
                    sockets.get_input_image(&res.node_socket(&socket))?,
                ))
            })
            .collect();
        let outputs: HashMap<_, _> = op
            .outputs()
            .keys()
            .map(|socket| {
                (
                    socket.clone(),
                    sockets.get_output_image(&res.node_socket(&socket)).unwrap(),
                )
            })
            .collect();

        // Build structures for intermediate data
        let mut intermediate_images = HashMap::new();
        let mut intermediate_buffers = HashMap::new();
        for (name, descr) in self
            .shader_library
            .intermediate_data_for(&op)
            .ok_or(InterpretationError::MissingShader)?
        {
            use super::shaders::FromSocketOr;

            match descr {
                IntermediateDataDescription::Image { size, ty } => {
                    let size = match size {
                        FromSocketOr::FromSocket(_) => {
                            sockets.get_image_size(res).allocation_size()
                        }
                        FromSocketOr::Independent(s) => *s,
                    };
                    let ty = match ty {
                        FromSocketOr::FromSocket(s) => self
                            .sockets
                            .get_output_image_type(&res.node_socket(s))
                            .expect("Invalid output socket"),
                        FromSocketOr::Independent(t) => *t,
                    };
                    let mut img = self.gpu.create_compute_image(size, ty, false)?;
                    img.ensure_alloc()?;
                    intermediate_images.insert(name.clone(), img);
                }
                IntermediateDataDescription::Buffer { dim, element_width } => {
                    let length = match dim {
                        BufferDim::Square(square) => match square {
                            FromSocketOr::FromSocket(_) => {
                                sockets.get_image_size(res).allocation_size().pow(2)
                            }
                            FromSocketOr::Independent(s) => s.pow(2) as u32,
                        },
                        BufferDim::Vector(vector) => match vector {
                            FromSocketOr::FromSocket(_) => {
                                sockets.get_image_size(res).allocation_size()
                            }
                            FromSocketOr::Independent(s) => *s as u32,
                        },
                    };

                    let bytes = length as u64 * *element_width as u64;
                    let buffer = self.gpu.create_compute_temp_buffer(bytes)?;

                    intermediate_buffers.insert(name.clone(), buffer);
                }
            }
        }

        // Build input occupancy vector
        let occupancy: Vec<_> = op
            .inputs()
            .keys()
            .sorted()
            .map(
                |socket| match sockets.get_input_image(&res.node_socket(&socket)) {
                    Some(img) => match img.get_image_type() {
                        ImageType::Grayscale => gpu::compute::InputOccupancy::OccupiedGrayscale,
                        ImageType::Rgb => gpu::compute::InputOccupancy::OccupiedRgb,
                    },
                    None => gpu::compute::InputOccupancy::Unoccupied,
                },
            )
            .collect();

        // Fill uniforms and execute operator passes
        let passes = self
            .shader_library
            .passes_for(&op)
            .ok_or(InterpretationError::MissingShader)?;

        self.gpu.fill_uniforms(&op.uniforms())?;
        self.gpu.fill_occupancy(&occupancy)?;

        for pass in passes {
            let writers = pass.descriptor_writers(
                self.gpu.uniform_buffer(),
                self.gpu.occupancy_buffer(),
                self.gpu.sampler(),
                &inputs,
                &outputs,
                &intermediate_images,
                &intermediate_buffers,
            );
            self.gpu.write_descriptor_sets(writers);
        }

        self.gpu.run_compute(
            sockets.get_image_size(res).allocation_size(),
            inputs.values().unique().copied(),
            outputs.values().copied(),
            intermediate_images.iter(),
            |img_size, intermediates_locks, cmd_buffer| {
                for pass in passes {
                    pass.build_commands(
                        img_size,
                        &intermediate_images,
                        intermediates_locks,
                        &intermediate_buffers,
                        cmd_buffer,
                    );
                }
            },
        );

        self.sockets.set_last_hash(res, uniform_hash);
        self.sockets.set_output_images_updated(res, self.seq);
        self.sockets
            .update_timing_data(res, start_time.elapsed().as_secs_f64());

        Ok(())
    }

    /// Process view socket handling, returning an event if appropriate. It will
    /// check against the given socket and node resources to determine whether
    /// either match against the view socket before proceeding.
    fn process_view_socket(
        &mut self,
        socket: Option<&Resource<Socket>>,
        node: Option<&Resource<Node>>,
    ) -> Result<Option<ComputeEvent>, InterpretationError> {
        if (socket.is_some() && socket != self.view_socket.as_ref().map(|x| x.0.clone()).as_ref())
            || (node.is_some()
                && self
                    .view_socket
                    .as_ref()
                    .map(|s| s.0.socket_node())
                    .as_ref()
                    != node)
        {
            return Ok(None);
        }
        if let Some((socket, vs_seq)) = &mut self.view_socket {
            if self
                .sockets
                .get_output_images_updated(&socket.socket_node())
                .unwrap_or(u64::MAX)
                < *vs_seq
            {
                return Ok(None);
            }

            let (image, ty) = self.sockets.get_output_image_typed(&socket).unwrap();
            *vs_seq = self.seq;

            Ok(Some(ComputeEvent::SocketViewReady(
                gpu::BrokerImage::from::<B>(image.get_raw()),
                image.get_layout(),
                image.get_access(),
                self.sockets
                    .get_image_size(&socket.socket_node())
                    .allocation_size(),
                ty,
            )))
        } else {
            Ok(None)
        }
    }

    /// Export an image as given by the export specifications to a certain path.
    fn export(
        &mut self,
        spec: &ExportSpec,
        path: std::path::PathBuf,
    ) -> Result<(), InterpretationError> {
        log::trace!("Exporting {} to {:?}", spec.node, path);

        let (img, ty) = self
            .sockets
            .get_input_image_typed(&spec.node.node_socket("data"))
            .ok_or(ExportError::UnknownImage)?;
        let img_size = img.get_size();
        let raw_data = self.gpu.download_image(img)?;

        let format = spec.format;
        let color_space = spec.color_space;
        let bit_depth = spec.bit_depth;

        std::thread::spawn(move || {
            log::trace!("Encoding image to {:?} in thread", format);
            match ConvertedImage::new(&raw_data, img_size, color_space, bit_depth, ty)
                .and_then(|img| img.save_to_file(format, path))
            {
                Err(e) => log::error!("Failed encoding with {}", e),
                _ => {}
            }
        });

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
            HashSet::from_iter(self.sockets.known_groups().cloned());

        let execution_stack = &self.execution_stack;

        for n in execution_stack
            .iter()
            .map(|frame| frame.retention_set())
            .flatten()
        {
            cleanable.remove(n);
        }

        for node in cleanable {
            self.sockets.free_images_for_group(&node, &mut self.gpu);
        }
    }

    /// Interpret a single instruction, given a substitution map
    fn interpret(
        &mut self,
        frame_size: u32,
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
                    AtomicOperator::Image(Image { resource }) => {
                        self.execute_image(
                            res,
                            &resource.ok_or(InterpretationError::ExternalDataNotFound)?,
                        )?;
                    }
                    AtomicOperator::Svg(Svg { resource }) => {
                        self.execute_svg(
                            res,
                            &resource.ok_or(InterpretationError::ExternalDataNotFound)?,
                        )?;
                    }
                    AtomicOperator::Input(..) => {
                        self.execute_input(res)?;
                    }
                    AtomicOperator::Output(output) => {
                        for res in self.execute_output(&output, res) {
                            response.push(res);
                        }
                        if let Some((spec, path)) = self.export_specs.get(res) {
                            self.export(spec, path.clone())?;
                        }
                    }
                    _ => {
                        self.execute_atomic_operator(frame_size, &op, res)?;
                    }
                }

                if let Some(ev) = self.process_view_socket(None, Some(res))? {
                    response.push(ev);
                }
            }
            Instruction::Call(res, op) => {
                self.execute_call(frame_size, res, op)?;
            }
            Instruction::Copy(from, to) => {
                self.execute_copy(from, to)?;

                if let Some(ev) = self.process_view_socket(Some(to), None)? {
                    response.push(ev);
                }
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
        if self.execution_stack.len() > STACK_LIMIT {
            return Some(Err(InterpretationError::StackLimitReached));
        }

        let frame = self.execution_stack.last_mut()?;
        let frame_size = frame.frame_size;
        let instruction = frame
            .instructions
            .pop_front()
            .expect("Found empty stack frame");
        let substitutions = frame.substitutions_map.clone();

        let response = match self.interpret(frame_size, &instruction, &substitutions) {
            Ok(r) => Some(Ok((r, self.seq))),

            // Handle OOM
            Err(InterpretationError::AllocatorError(gpu::compute::AllocatorError::OutOfMemory)) => {
                self.cleanup();
                match self.interpret(frame_size, &instruction, &substitutions) {
                    Ok(r) => Some(Ok((r, self.seq))),
                    Err(InterpretationError::AllocatorError(
                        gpu::compute::AllocatorError::OutOfMemory,
                    )) => Some(Err(InterpretationError::HardOOM)),
                    Err(e) => Some(Err(e)),
                }
            }
            Err(e) => Some(Err(e)),
        };

        if instruction.is_execution_step() {
            self.execution_stack.last_mut()?.step += 1;
        }

        // Pop frame if we're done here
        if self.execution_stack.last()?.instructions.is_empty() {
            let frame = self.execution_stack.pop().unwrap();
            if let Some(caller) = frame.caller {
                let elapsed = frame.start_time.elapsed();
                self.sockets
                    .update_timing_data(&caller, elapsed.as_secs_f64());
            }
        }

        response
    }
}
