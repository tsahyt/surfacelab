/// Tools for defining shaders for atomic operators.
use crate::{
    gpu,
    lang::{self, ImageType, Socketed},
};
use enum_dispatch::*;
use std::borrow::Cow;
use std::collections::HashMap;
use zerocopy::AsBytes;

pub use gpu::Specialization;

/// Usage of a descriptor for an operator
pub enum OperatorDescriptorUse {
    /// Input images are passed into the shader
    InputImage(&'static str),
    /// Input images are compute results of the shader
    OutputImage(&'static str),
    /// Intermediate images are used for temporary storage and persist between
    /// operator passes. The bool describes whether space for MIP levels must be
    /// reserved
    IntermediateImage(&'static str),
    /// Intermediate buffers are used for temporary storage of non-image data
    IntermediateBuffer(&'static str),
    /// The sampler to use on input images
    Sampler,
    /// Uniform buffer
    Uniforms,
    /// Occupancy buffer
    Occupancy,
}

/// Simplified description of a descriptor for use in operators
pub struct OperatorDescriptor {
    /// Binding of the descriptor. Needs to match with shader code!
    pub binding: u32,
    pub descriptor: OperatorDescriptorUse,
}

/// Workgroup size of the operator pass.
pub enum OperatorShape {
    /// Execute shader per pixel, using given local work group sizes
    PerPixel { local_x: u8, local_y: u8 },
    /// Execute shader per row or column, using the given work group size for
    /// number of rows in a local workgroup
    PerRowOrColumn { local_size: u8 },
}

/// Describes an operator shader. Typically there is one shader per operator.
pub struct OperatorShader {
    pub spirv: &'static [u8],
    pub descriptors: &'static [OperatorDescriptor],
    pub specialization: Specialization<'static>,
    pub shape: OperatorShape,
}

impl OperatorShader {
    /// Return an iterator describing the descriptor set layout of this shader
    pub fn layout(&self) -> impl Iterator<Item = gpu::DescriptorSetLayoutBinding> {
        self.descriptors.iter().map(|desc| match desc.descriptor {
            OperatorDescriptorUse::OutputImage(..)
            | OperatorDescriptorUse::IntermediateImage(..) => gpu::DescriptorSetLayoutBinding {
                binding: desc.binding,
                ty: gpu::DescriptorType::Image {
                    ty: gpu::ImageDescriptorType::Storage { read_only: false },
                },
                count: 1,
                stage_flags: gpu::ShaderStageFlags::COMPUTE,
                immutable_samplers: false,
            },
            OperatorDescriptorUse::InputImage(..) => gpu::DescriptorSetLayoutBinding {
                binding: desc.binding,
                ty: gpu::DescriptorType::Image {
                    ty: gpu::ImageDescriptorType::Sampled {
                        with_sampler: false,
                    },
                },
                count: 1,
                stage_flags: gpu::ShaderStageFlags::COMPUTE,
                immutable_samplers: false,
            },
            OperatorDescriptorUse::IntermediateBuffer(..) => gpu::DescriptorSetLayoutBinding {
                binding: desc.binding,
                ty: gpu::DescriptorType::Buffer {
                    ty: gpu::BufferDescriptorType::Storage { read_only: false },
                    format: gpu::BufferDescriptorFormat::Structured {
                        dynamic_offset: false,
                    },
                },
                count: 1,
                stage_flags: gpu::ShaderStageFlags::COMPUTE,
                immutable_samplers: false,
            },
            OperatorDescriptorUse::Sampler => gpu::DescriptorSetLayoutBinding {
                binding: desc.binding,
                ty: gpu::DescriptorType::Sampler,
                count: 1,
                stage_flags: gpu::ShaderStageFlags::COMPUTE,
                immutable_samplers: false,
            },
            OperatorDescriptorUse::Uniforms | OperatorDescriptorUse::Occupancy => {
                gpu::DescriptorSetLayoutBinding {
                    binding: desc.binding,
                    ty: gpu::DescriptorType::Buffer {
                        ty: gpu::BufferDescriptorType::Uniform,
                        format: gpu::BufferDescriptorFormat::Structured {
                            dynamic_offset: false,
                        },
                    },
                    count: 1,
                    stage_flags: gpu::ShaderStageFlags::COMPUTE,
                    immutable_samplers: false,
                }
            }
        })
    }

    /// Return descriptor set write operators for this shader, given a
    /// descriptor set to write to, uniform buffer, sampler, as well as input
    /// and output images.
    pub fn writers<'a, B: gpu::Backend>(
        &self,
        desc_set: &'a B::DescriptorSet,
        uniforms: &'a B::Buffer,
        occupancy: &'a B::Buffer,
        sampler: &'a B::Sampler,
        inputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
        outputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
        intermediate_images: &'a HashMap<String, gpu::compute::Image<B>>,
        intermediate_buffers: &'a HashMap<String, gpu::compute::TempBuffer<B>>,
    ) -> impl Iterator<Item = gpu::DescriptorSetWrite<'a, B, Vec<gpu::Descriptor<'a, B>>>> {
        self.descriptors
            .iter()
            .map(move |desc| match desc.descriptor {
                OperatorDescriptorUse::Uniforms => gpu::DescriptorSetWrite {
                    set: desc_set,
                    binding: desc.binding,
                    array_offset: 0,
                    descriptors: vec![gpu::Descriptor::Buffer(uniforms, gpu::SubRange::WHOLE)],
                },
                OperatorDescriptorUse::Occupancy => gpu::DescriptorSetWrite {
                    set: desc_set,
                    binding: desc.binding,
                    array_offset: 0,
                    descriptors: vec![gpu::Descriptor::Buffer(occupancy, gpu::SubRange::WHOLE)],
                },
                OperatorDescriptorUse::Sampler => gpu::DescriptorSetWrite {
                    set: desc_set,
                    binding: desc.binding,
                    array_offset: 0,
                    descriptors: vec![gpu::Descriptor::Sampler(sampler)],
                },
                OperatorDescriptorUse::InputImage(socket) => gpu::DescriptorSetWrite {
                    set: desc_set,
                    binding: desc.binding,
                    array_offset: 0,
                    descriptors: vec![gpu::Descriptor::Image(
                        // This workaround requires that operators have at least
                        // one non-optional input!
                        inputs
                            .get(socket)
                            .or_else(|| inputs.values().next())
                            .and_then(|i| i.get_view())
                            .unwrap(),
                        gpu::Layout::ShaderReadOnlyOptimal,
                    )],
                },
                OperatorDescriptorUse::OutputImage(socket) => gpu::DescriptorSetWrite {
                    set: desc_set,
                    binding: desc.binding,
                    array_offset: 0,
                    descriptors: vec![gpu::Descriptor::Image(
                        outputs.get(socket).unwrap().get_view().unwrap(),
                        gpu::Layout::General,
                    )],
                },
                OperatorDescriptorUse::IntermediateImage(name) => gpu::DescriptorSetWrite {
                    set: desc_set,
                    binding: desc.binding,
                    array_offset: 0,
                    descriptors: vec![gpu::Descriptor::Image(
                        intermediate_images.get(name).unwrap().get_view().unwrap(),
                        gpu::Layout::General,
                    )],
                },
                OperatorDescriptorUse::IntermediateBuffer(name) => gpu::DescriptorSetWrite {
                    set: desc_set,
                    binding: desc.binding,
                    array_offset: 0,
                    descriptors: vec![gpu::Descriptor::Buffer(
                        intermediate_buffers.get(name).unwrap().get_raw(),
                        gpu::SubRange::WHOLE,
                    )],
                },
            })
    }
}

/// Description of synchronization to be performed. The string refers to an
/// intermediate image by name.
pub enum SynchronizeDescription {
    ToWrite(&'static str),
    ToRead(&'static str),
    ToReadWrite(&'static str),
}

/// Executing an operator on the GPU is done by running one or more passes.
/// There is a special pass to synchronize resources between other passes, in
/// case there is a data dependency.
///
/// Note that the Uniform descriptor passed into passes refers to the *same*
/// uniform struct across *all* passes!
pub enum OperatorPassDescription {
    /// Run a shader as an operator pass
    RunShader(OperatorShader),
    /// Generate MIP levels for an image in a specified intermediate image,
    GenerateMips(&'static str, &'static str),
    /// Synchronize image according to description
    SynchronizeImage(&'static [SynchronizeDescription]),
    /// Synchronize buffer according to description
    SynchronizeBuffer(&'static [SynchronizeDescription]),
}

/// A "compiled" operator pass holding the required GPU structures for execution.
pub enum OperatorPass<B: gpu::Backend> {
    RunShader {
        operator_shader: OperatorShader,
        pipeline: gpu::compute::ComputePipeline<B>,
        descriptors: B::DescriptorSet,
    },
    GenerateMips(&'static str, &'static str),
    SynchronizeImage(&'static [SynchronizeDescription]),
    SynchronizeBuffer(&'static [SynchronizeDescription]),
}

impl<B> OperatorPass<B>
where
    B: gpu::Backend,
{
    /// Fill the given command buffer with commands to execute this operator pass.
    pub fn build_commands<'a, L>(
        &self,
        image_size: u32,
        intermediate_images: &'a HashMap<String, gpu::compute::Image<B>>,
        intermediate_images_locks: &'a HashMap<String, L>,
        intermediate_buffers: &'a HashMap<String, gpu::compute::TempBuffer<B>>,
        cmd_buffer: &mut B::CommandBuffer,
    ) where
        L: std::ops::Deref<Target = B::Image>,
    {
        use gfx_hal::prelude::*;
        match self {
            Self::RunShader {
                pipeline,
                descriptors,
                operator_shader: OperatorShader { shape, .. },
            } => unsafe {
                cmd_buffer.bind_compute_pipeline(pipeline.pipeline());
                cmd_buffer.bind_compute_descriptor_sets(
                    pipeline.pipeline_layout(),
                    0,
                    Some(descriptors),
                    &[],
                );
                cmd_buffer.dispatch(match shape {
                    OperatorShape::PerPixel { local_x, local_y } => [
                        image_size / *local_x as u32,
                        image_size / *local_y as u32,
                        1,
                    ],
                    OperatorShape::PerRowOrColumn { local_size } => {
                        [image_size / *local_size as u32, 1, 1]
                    }
                });
            },
            Self::GenerateMips(source, target) => unsafe {
                let source_size = todo!();
                let target_size = intermediate_images[*target].get_size() as i32;

                cmd_buffer.blit_image(
                    todo!("source image lock"),
                    todo!("source layout"),
                    &intermediate_images_locks[*target],
                    intermediate_images[*target].get_layout(),
                    gfx_hal::image::Filter::Linear,
                    (0..3).map(|level| gfx_hal::command::ImageBlit {
                        src_subresource: gfx_hal::image::SubresourceLayers {
                            aspects: gfx_hal::format::Aspects::COLOR,
                            level: 0,
                            layers: 0..1,
                        },
                        src_bounds: gfx_hal::image::Offset { x: 0, y: 0, z: 0 }..gfx_hal::image::Offset {
                            x: source_size,
                            y: source_size,
                            z: 1,
                        },
                        dst_subresource: gfx_hal::image::SubresourceLayers {
                            aspects: gfx_hal::format::Aspects::COLOR,
                            level,
                            layers: 0..1,
                        },
                        dst_bounds: gfx_hal::image::Offset { x: 0, y: 0, z: 0 }..gfx_hal::image::Offset {
                            x: target_size >> level,
                            y: target_size >> level,
                            z: 1,
                        },
                    }),
                );
            },
            Self::SynchronizeImage(descs) => unsafe {
                cmd_buffer.pipeline_barrier(
                    gfx_hal::pso::PipelineStage::COMPUTE_SHADER
                        ..gfx_hal::pso::PipelineStage::COMPUTE_SHADER,
                    gfx_hal::memory::Dependencies::empty(),
                    descs.iter().map(|desc| match desc {
                        SynchronizeDescription::ToWrite(name) => {
                            let image = intermediate_images
                                .get(*name)
                                .expect("Illegal intermediate image");

                            image.barrier_to(
                                &intermediate_images_locks[*name],
                                gfx_hal::image::Access::SHADER_WRITE,
                                gfx_hal::image::Layout::General,
                            )
                        }
                        SynchronizeDescription::ToRead(name) => {
                            let image = intermediate_images
                                .get(*name)
                                .expect("Illegal intermediate image");

                            image.barrier_to(
                                &intermediate_images_locks[*name],
                                gfx_hal::image::Access::SHADER_READ,
                                gfx_hal::image::Layout::General,
                            )
                        }
                        SynchronizeDescription::ToReadWrite(name) => {
                            let image = intermediate_images
                                .get(*name)
                                .expect("Illegal intermediate image");

                            image.barrier_to(
                                &intermediate_images_locks[*name],
                                gfx_hal::image::Access::SHADER_READ
                                    | gfx_hal::image::Access::SHADER_WRITE,
                                gfx_hal::image::Layout::General,
                            )
                        }
                    }),
                );
            },
            Self::SynchronizeBuffer(descs) => unsafe {
                cmd_buffer.pipeline_barrier(
                    gfx_hal::pso::PipelineStage::COMPUTE_SHADER
                        ..gfx_hal::pso::PipelineStage::COMPUTE_SHADER,
                    gfx_hal::memory::Dependencies::empty(),
                    descs.iter().map(|desc| match desc {
                        SynchronizeDescription::ToWrite(name) => {
                            let buffer = intermediate_buffers
                                .get(*name)
                                .expect("Illegal intermediate buffer");

                            buffer.barrier_to(gfx_hal::buffer::Access::SHADER_WRITE)
                        }
                        SynchronizeDescription::ToRead(name) => {
                            let buffer = intermediate_buffers
                                .get(*name)
                                .expect("Illegal intermediate buffer");

                            buffer.barrier_to(gfx_hal::buffer::Access::SHADER_READ)
                        }
                        SynchronizeDescription::ToReadWrite(name) => {
                            let buffer = intermediate_buffers
                                .get(*name)
                                .expect("Illegal intermediate buffer");

                            buffer.barrier_to(
                                gfx_hal::buffer::Access::SHADER_WRITE
                                    | gfx_hal::buffer::Access::SHADER_READ,
                            )
                        }
                    }),
                );
            },
        }
    }

    /// Obtain descriptor set writers for this operator pass.
    pub fn descriptor_writers<'a>(
        &'a self,
        uniforms: &'a B::Buffer,
        occupancy: &'a B::Buffer,
        sampler: &'a B::Sampler,
        inputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
        outputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
        intermediate_images: &'a HashMap<String, gpu::compute::Image<B>>,
        intermediate_buffers: &'a HashMap<String, gpu::compute::TempBuffer<B>>,
    ) -> Vec<gpu::DescriptorSetWrite<'a, B, Vec<gpu::Descriptor<'a, B>>>> {
        match self {
            OperatorPass::RunShader {
                operator_shader,
                descriptors,
                ..
            } => operator_shader
                .writers(
                    descriptors,
                    uniforms,
                    occupancy,
                    sampler,
                    inputs,
                    outputs,
                    intermediate_images,
                    intermediate_buffers,
                )
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Create an `OperatorPass` from a description. This will convert the
    /// description to GPU structures.
    pub fn from_description(
        description: OperatorPassDescription,
        gpu: &mut gpu::compute::GPUCompute<B>,
    ) -> Result<Self, gpu::compute::InitializationError> {
        match description {
            OperatorPassDescription::RunShader(operator_shader) => {
                let shader: gpu::Shader<B> = gpu.create_shader(operator_shader.spirv)?;
                let pipeline: gpu::compute::ComputePipeline<B> = gpu.create_pipeline(
                    &shader,
                    &operator_shader.specialization,
                    operator_shader.layout(),
                )?;
                let desc_set = gpu.allocate_descriptor_set(pipeline.set_layout())?;
                Ok(Self::RunShader {
                    operator_shader,
                    pipeline,
                    descriptors: desc_set,
                })
            }
            OperatorPassDescription::GenerateMips(source, img) => {
                Ok(Self::GenerateMips(source, img))
            }
            OperatorPassDescription::SynchronizeImage(desc) => Ok(Self::SynchronizeImage(desc)),
            OperatorPassDescription::SynchronizeBuffer(desc) => Ok(Self::SynchronizeBuffer(desc)),
        }
    }
}

/// Defines the dimensions of a buffer, counted in number of elements.
pub enum BufferDim {
    Square(FromSocketOr<usize>),
    Vector(FromSocketOr<usize>),
}

/// Description element to take data from an existing socket or use an
/// independent definition.
pub enum FromSocketOr<T> {
    FromSocket(&'static str),
    Independent(T),
}

/// Description of intermediate data in an Operator. References sockets are
/// assumed to be *outputs*.
pub enum IntermediateDataDescription {
    Image {
        /// Image size to use for the intermediate image. Since all outputs have the
        /// same size, the exact choice is irrelevant.
        size: FromSocketOr<u32>,
        /// Type of the intermediate image.
        ty: FromSocketOr<ImageType>,
        /// Whether the image supports MIP levels
        mips: bool,
    },
    Buffer {
        /// Dimensions of the buffer
        dim: BufferDim,
        /// Element width in bytes
        element_width: usize,
    },
}

/// A Shader is anything that can return a list of operator passes for itself. This
/// trait is used to attach a GPU side implementation to an operator.
#[enum_dispatch]
pub trait Shader {
    /// Return a list of operator passes
    fn operator_passes(&self) -> Vec<OperatorPassDescription>;

    /// Return a hashmap describing all intermediate data by name. Defaults to empty.
    fn intermediate_data(&self) -> HashMap<String, IntermediateDataDescription> {
        HashMap::new()
    }
}

/// Uniforms are structs that can be converted into plain buffers for GPU use,
/// and can be hashed.
#[enum_dispatch]
pub trait Uniforms {
    fn uniforms(&self) -> Cow<[u8]>;
    fn uniform_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;

        let mut hasher = DefaultHasher::new();
        hasher.write(&self.uniforms());
        hasher.finish()
    }
}

impl<T> Uniforms for T
where
    T: AsBytes,
{
    fn uniforms(&self) -> Cow<[u8]> {
        Cow::Borrowed(self.as_bytes())
    }
}

struct ShaderData<B: gpu::Backend> {
    passes: Vec<OperatorPass<B>>,
    intermediate_data: Vec<(String, IntermediateDataDescription)>,
}

/// The shader library holds relevant data for all (operator) shaders.
pub struct ShaderLibrary<B: gpu::Backend> {
    shaders: HashMap<String, ShaderData<B>>,
}

impl<B> ShaderLibrary<B>
where
    B: gpu::Backend,
{
    /// Initialize the shader library
    pub fn new(
        gpu: &mut gpu::compute::GPUCompute<B>,
    ) -> Result<Self, gpu::compute::InitializationError> {
        log::info!("Initializing Shader Library");
        let mut shaders = HashMap::new();

        for op in lang::AtomicOperator::all_default() {
            log::trace!("Initializing operator {}", op.title());
            let passes = op
                .operator_passes()
                .drain(0..)
                .map(|pass| OperatorPass::from_description(pass, gpu))
                .flatten()
                .collect();
            let intermediate_data = op.intermediate_data().drain().collect();

            shaders.insert(
                op.default_name().to_string(),
                ShaderData {
                    passes,
                    intermediate_data,
                },
            );
        }

        log::info!("Shader Library initialized!");

        Ok(ShaderLibrary { shaders })
    }

    /// Obtain the operator passes for the given atomic operator
    pub fn passes_for(&self, op: &lang::AtomicOperator) -> Option<&[OperatorPass<B>]> {
        self.shaders
            .get(op.default_name())
            .map(|x| x.passes.as_ref())
    }

    pub fn intermediate_data_for(
        &self,
        op: &lang::AtomicOperator,
    ) -> Option<&[(String, IntermediateDataDescription)]> {
        self.shaders
            .get(op.default_name())
            .map(|x| x.intermediate_data.as_ref())
    }
}
