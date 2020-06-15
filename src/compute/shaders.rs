use crate::{
    gpu,
    lang::{self, Socketed},
};
use enum_dispatch::*;
use std::collections::HashMap;
use zerocopy::AsBytes;

pub enum OperatorDescriptorUse {
    InputImage(&'static str),
    OutputImage(&'static str),
    Sampler,
    Uniforms,
}

pub struct OperatorDescriptor {
    pub binding: u32,
    pub descriptor: OperatorDescriptorUse,
}

pub struct OperatorShader {
    pub spirv: &'static [u8],
    pub descriptors: &'static [OperatorDescriptor],
}

impl OperatorShader {
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

#[enum_dispatch]
pub trait Shader {
    fn operator_shader(&self) -> Option<OperatorShader>;
}

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

/// Image is special and doesn't have uniforms. Therefore the output is empty
impl Uniforms for lang::Image {
    fn uniforms(&self) -> &[u8] {
        &[]
    }
}

/// Output is special and doesn't have uniforms. Therefore the output is empty
impl Uniforms for lang::Output {
    fn uniforms(&self) -> &[u8] {
        &[]
    }
}

pub struct ShaderLibrary<B: gpu::Backend> {
    _shaders: HashMap<&'static str, gpu::Shader<B>>,
    pipelines: HashMap<&'static str, gpu::compute::ComputePipeline<B>>,
    descriptor_sets: HashMap<&'static str, B::DescriptorSet>,
}

impl<B> ShaderLibrary<B>
where
    B: gpu::Backend,
{
    pub fn new(gpu: &mut gpu::compute::GPUCompute<B>) -> Result<Self, String> {
        log::info!("Initializing Shader Library");
        let mut shaders = HashMap::new();
        let mut pipelines = HashMap::new();
        let mut descriptor_sets = HashMap::new();
        for op in lang::Operator::all_default() {
            log::trace!("Initializing operator {}", op.title());
            if let Some(operator_shader) = op.operator_shader() {
                let shader: gpu::Shader<B> = gpu.create_shader(operator_shader.spirv)?;
                let pipeline: gpu::compute::ComputePipeline<B> =
                    gpu.create_pipeline(&shader, operator_shader.layout())?;
                let desc_set = gpu.allocate_descriptor_set(pipeline.set_layout())?;

                shaders.insert(op.default_name(), shader);
                pipelines.insert(op.default_name(), pipeline);
                descriptor_sets.insert(op.default_name(), desc_set);
            }
        }

        log::info!("Shader Library initialized!");

        Ok(ShaderLibrary {
            _shaders: shaders,
            pipelines,
            descriptor_sets,
        })
    }

    pub fn pipeline_for(&self, op: &lang::Operator) -> &gpu::compute::ComputePipeline<B> {
        debug_assert!(op.default_name() != "image" && op.default_name() != "output");
        self.pipelines.get(op.default_name()).unwrap()
    }

    pub fn descriptor_set_for(&self, op: &lang::Operator) -> &B::DescriptorSet {
        debug_assert!(op.default_name() != "image" && op.default_name() != "output");
        self.descriptor_sets.get(op.default_name()).unwrap()
    }

    /// Create descriptor set writes for given operator with its inputs and outputs.
    /// This assumes that all given inputs and outputs are already bound!
    pub fn write_desc<'a>(
        op: &lang::Operator,
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
