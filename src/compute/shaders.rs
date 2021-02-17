/// Tools for defining shaders for atomic operators.
use crate::{
    gpu,
    lang::{self, Socketed},
};
use enum_dispatch::*;
use std::collections::HashMap;
use zerocopy::AsBytes;

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

/// A Shader is anything that can return an operator shader for itself.
#[enum_dispatch]
pub trait Shader {
    fn operator_shader(&self) -> Option<OperatorShader>;
}

/// Uniforms are structs that can be converted into plain buffers for GPU use,
/// and can be hashed.
#[enum_dispatch]
pub trait Uniforms {
    fn uniforms(&self) -> &[u8];
    fn uniform_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;

        let mut hasher = DefaultHasher::new();
        hasher.write(self.uniforms());
        hasher.finish()
    }
}

impl<T> Uniforms for T
where
    T: AsBytes,
{
    fn uniforms(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// The shader library holds relevant data for all (operator) shaders.
pub struct ShaderLibrary<B: gpu::Backend> {
    pipelines: HashMap<String, gpu::compute::ComputePipeline<B>>,
    descriptor_sets: HashMap<String, B::DescriptorSet>,
}

impl<B> ShaderLibrary<B>
where
    B: gpu::Backend,
{
    /// Initialize the shader library
    pub fn new(gpu: &mut gpu::compute::GPUCompute<B>) -> Result<Self, gpu::InitializationError> {
        log::info!("Initializing Shader Library");
        let mut pipelines = HashMap::new();
        let mut descriptor_sets = HashMap::new();

        for op in lang::AtomicOperator::all_default() {
            log::trace!("Initializing operator {}", op.title());
            if let Some(operator_shader) = op.operator_shader() {
                let shader: gpu::Shader<B> = gpu.create_shader(operator_shader.spirv)?;
                let pipeline: gpu::compute::ComputePipeline<B> =
                    gpu.create_pipeline(&shader, operator_shader.layout())?;
                let desc_set = gpu.allocate_descriptor_set(pipeline.set_layout())?;

                pipelines.insert(op.default_name().to_string(), pipeline);
                descriptor_sets.insert(op.default_name().to_string(), desc_set);
            }
        }

        log::info!("Shader Library initialized!");

        Ok(ShaderLibrary {
            pipelines,
            descriptor_sets,
        })
    }

    /// Obtain a compute pipeline for the given operator
    pub fn pipeline_for(&self, op: &lang::AtomicOperator) -> &gpu::compute::ComputePipeline<B> {
        debug_assert!(op.default_name() != "image" && op.default_name() != "output");
        self.pipelines.get(op.default_name()).unwrap()
    }

    /// Obtain the descriptor set for the given operator
    pub fn descriptor_set_for(&self, op: &lang::AtomicOperator) -> &B::DescriptorSet {
        debug_assert!(op.default_name() != "image" && op.default_name() != "output");
        self.descriptor_sets.get(op.default_name()).unwrap()
    }

    /// Create descriptor set writes for given operator with its inputs and outputs.
    /// This assumes that all given inputs and outputs are already bound!
    pub fn write_desc<'a>(
        op: &lang::AtomicOperator,
        desc_set: &'a B::DescriptorSet,
        uniforms: &'a B::Buffer,
        sampler: &'a B::Sampler,
        inputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
        outputs: &'a HashMap<String, &'a gpu::compute::Image<B>>,
    ) -> Vec<gpu::DescriptorSetWrite<'a, B, Vec<gpu::Descriptor<'a, B>>>> {
        match op.operator_shader() {
            Some(operator_shader) => operator_shader
                .writers(desc_set, uniforms, sampler, inputs, outputs)
                .collect(),
            None => vec![],
        }
    }
}
