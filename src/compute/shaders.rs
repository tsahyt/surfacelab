/// Tools for defining shaders for atomic operators.
use crate::{
    gpu,
    lang::{self, Socketed},
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
    /// The sampler to use on input images
    Sampler,
    /// Uniform buffer
    Uniforms,
}

/// Simplified description of a descriptor for use in operators
pub struct OperatorDescriptor {
    /// Binding of the descriptor. Needs to match with shader code!
    pub binding: u32,
    pub descriptor: OperatorDescriptorUse,
}

/// Describes an operator shader. Typically there is one shader per operator.
pub struct OperatorShader {
    pub spirv: &'static [u8],
    pub descriptors: &'static [OperatorDescriptor],
    pub specialization: Specialization<'static>,
}

impl OperatorShader {
    /// Return an iterator describing the descriptor set layout of this shader
    pub fn layout(&self) -> impl Iterator<Item = gpu::DescriptorSetLayoutBinding> {
        self.descriptors.iter().map(|desc| match desc.descriptor {
            OperatorDescriptorUse::OutputImage(..) => gpu::DescriptorSetLayoutBinding {
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
            OperatorDescriptorUse::Sampler => gpu::DescriptorSetLayoutBinding {
                binding: desc.binding,
                ty: gpu::DescriptorType::Sampler,
                count: 1,
                stage_flags: gpu::ShaderStageFlags::COMPUTE,
                immutable_samplers: false,
            },
            OperatorDescriptorUse::Uniforms => gpu::DescriptorSetLayoutBinding {
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
            },
        })
    }

    /// Return descriptor set write operators for this shader, given a
    /// descriptor set to write to, uniform buffer, sampler, as well as input
    /// and output images.
    pub fn writers<'a, B: gpu::Backend>(
        &self,
        desc_set: &'a B::DescriptorSet,
        uniforms: &'a B::Buffer,
        sampler: &'a B::Sampler,
        inputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
        outputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
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
                        inputs.get(socket).unwrap().get_view().unwrap(),
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
            })
    }
}

/// Executing an operator on the GPU is done by running one or more passes.
/// There is a special pass to synchronize resources between other passes, in
/// case there is a data dependency.
///
/// Note that the Uniform descriptor passed into passes refers to the *same*
/// uniform struct across *all* passes!
pub enum OperatorPassDescription {
    RunShader(OperatorShader),
    Synchronize,
}

/// A "compiled" operator pass holding the required GPU structures for execution.
pub enum OperatorPass<B: gpu::Backend> {
    RunShader {
        operator_shader: OperatorShader,
        pipeline: gpu::compute::ComputePipeline<B>,
        descriptors: B::DescriptorSet,
    },
    Synchronize,
}

impl<B> OperatorPass<B>
where
    B: gpu::Backend,
{
    /// Fill the given command buffer with commands to execute this operator pass.
    pub fn build_commands(&self, image_size: u32, cmd_buffer: &mut B::CommandBuffer) {
        use gfx_hal::prelude::*;
        match self {
            Self::RunShader {
                pipeline,
                descriptors,
                ..
            } => unsafe {
                cmd_buffer.bind_compute_pipeline(pipeline.pipeline());
                cmd_buffer.bind_compute_descriptor_sets(
                    pipeline.pipeline_layout(),
                    0,
                    Some(descriptors),
                    &[],
                );
                cmd_buffer.dispatch([image_size / 8, image_size / 8, 1]);
            },
            Self::Synchronize => {
                todo!()
            }
        }
    }

    /// Obtain descriptor set writers for this operator pass.
    pub fn descriptor_writers<'a>(
        &'a self,
        uniforms: &'a B::Buffer,
        sampler: &'a B::Sampler,
        inputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
        outputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
    ) -> Vec<gpu::DescriptorSetWrite<'a, B, Vec<gpu::Descriptor<'a, B>>>> {
        match self {
            OperatorPass::RunShader {
                operator_shader,
                descriptors,
                ..
            } => operator_shader
                .writers(descriptors, uniforms, sampler, inputs, outputs)
                .collect(),
            OperatorPass::Synchronize => Vec::new(),
        }
    }

    /// Create an `OperatorPass` from a description. This will convert the
    /// description to GPU structures.
    pub fn from_description(
        description: OperatorPassDescription,
        gpu: &mut gpu::compute::GPUCompute<B>,
    ) -> Result<Self, gpu::InitializationError> {
        match description {
            OperatorPassDescription::RunShader(operator_shader) => {
                let shader: gpu::Shader<B> = gpu.create_shader(operator_shader.spirv)?;
                let pipeline: gpu::compute::ComputePipeline<B> =
                    gpu.create_pipeline(&shader, operator_shader.layout())?;
                let desc_set = gpu.allocate_descriptor_set(pipeline.set_layout())?;
                Ok(Self::RunShader {
                    operator_shader,
                    pipeline,
                    descriptors: desc_set,
                })
            }
            OperatorPassDescription::Synchronize => Ok(Self::Synchronize),
        }
    }
}

/// A Shader is anything that can return a list of operator passes for itself. This
/// trait is used to attach a GPU side implementation to an operator.
#[enum_dispatch]
pub trait Shader {
    fn operator_passes(&self) -> Vec<OperatorPassDescription>;
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

/// The shader library holds relevant data for all (operator) shaders.
pub struct ShaderLibrary<B: gpu::Backend> {
    shaders: HashMap<String, Vec<OperatorPass<B>>>,
}

impl<B> ShaderLibrary<B>
where
    B: gpu::Backend,
{
    /// Initialize the shader library
    pub fn new(gpu: &mut gpu::compute::GPUCompute<B>) -> Result<Self, gpu::InitializationError> {
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
            shaders.insert(op.default_name().to_string(), passes);
        }

        log::info!("Shader Library initialized!");

        Ok(ShaderLibrary { shaders })
    }

    /// Obtain the operator passes for the given atomic operator
    pub fn passes_for(&self, op: &lang::AtomicOperator) -> Option<&[OperatorPass<B>]> {
        self.shaders.get(op.default_name()).map(|x| x.as_ref())
    }
}
