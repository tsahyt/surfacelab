use crate::{gpu, lang};
use std::collections::HashMap;
use zerocopy::AsBytes;

struct OperatorShader {
    spirv: &'static [u8],
    layout: &'static [gpu::DescriptorSetLayoutBinding],
}

impl OperatorShader {
    pub fn from_operator(op: &lang::Operator) -> Option<&'static Self> {
        use lang::Operator;

        match op {
            // Image and Output are special
            Operator::Image { .. } => None,
            Operator::Output { .. } => None,

            Operator::Blend(..) => Some(&BLEND),
            Operator::PerlinNoise(..) => Some(&PERLIN_NOISE),
            Operator::Rgb(..) => Some(&RGB),
            Operator::Grayscale(..) => Some(&GRAYSCALE),
            Operator::Ramp(..) => Some(&RAMP),
            Operator::NormalMap(..) => Some(&NORMAL_MAP),
        }
    }
}

static BLEND: OperatorShader = OperatorShader {
    spirv: include_bytes!("../../shaders/blend.spv"),
    layout: &[
        gpu::DescriptorSetLayoutBinding {
            binding: 0,
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
        gpu::DescriptorSetLayoutBinding {
            binding: 1,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Sampled {
                    with_sampler: false,
                },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 2,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Sampled {
                    with_sampler: false,
                },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 3,
            ty: gpu::DescriptorType::Sampler,
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 4,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Storage { read_only: false },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
    ],
};
static PERLIN_NOISE: OperatorShader = OperatorShader {
    spirv: include_bytes!("../../shaders/perlin.spv"),
    layout: &[
        gpu::DescriptorSetLayoutBinding {
            binding: 0,
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
        gpu::DescriptorSetLayoutBinding {
            binding: 1,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Storage { read_only: false },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
    ],
};
static RGB: OperatorShader = OperatorShader {
    spirv: include_bytes!("../../shaders/rgb.spv"),
    layout: &[
        gpu::DescriptorSetLayoutBinding {
            binding: 0,
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
        gpu::DescriptorSetLayoutBinding {
            binding: 1,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Storage { read_only: false },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
    ],
};
static GRAYSCALE: OperatorShader = OperatorShader {
    spirv: include_bytes!("../../shaders/grayscale.spv"),
    layout: &[
        gpu::DescriptorSetLayoutBinding {
            binding: 0,
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
        gpu::DescriptorSetLayoutBinding {
            binding: 1,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Sampled {
                    with_sampler: false,
                },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 2,
            ty: gpu::DescriptorType::Sampler,
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 3,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Storage { read_only: false },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
    ],
};
static RAMP: OperatorShader = OperatorShader {
    spirv: include_bytes!("../../shaders/ramp.spv"),
    layout: &[
        gpu::DescriptorSetLayoutBinding {
            binding: 0,
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
        gpu::DescriptorSetLayoutBinding {
            binding: 1,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Sampled {
                    with_sampler: false,
                },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 2,
            ty: gpu::DescriptorType::Sampler,
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 3,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Storage { read_only: false },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
    ],
};
static NORMAL_MAP: OperatorShader = OperatorShader {
    spirv: include_bytes!("../../shaders/normal.spv"),
    layout: &[
        gpu::DescriptorSetLayoutBinding {
            binding: 0,
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
        gpu::DescriptorSetLayoutBinding {
            binding: 1,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Sampled {
                    with_sampler: false,
                },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 2,
            ty: gpu::DescriptorType::Sampler,
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
        gpu::DescriptorSetLayoutBinding {
            binding: 3,
            ty: gpu::DescriptorType::Image {
                ty: gpu::ImageDescriptorType::Storage { read_only: false },
            },
            count: 1,
            stage_flags: gpu::ShaderStageFlags::COMPUTE,
            immutable_samplers: false,
        },
    ],
};

/// Create descriptor set writes for given operator with its inputs and outputs.
/// This assumes that all given inputs and outputs are already bound!
pub fn operator_write_desc<'a, B: gpu::Backend, S: std::hash::BuildHasher>(
    op: &lang::Operator,
    desc_set: &'a B::DescriptorSet,
    uniforms: &'a B::Buffer,
    sampler: &'a B::Sampler,
    inputs: &HashMap<String, &'a gpu::compute::Image<B>, S>,
    outputs: &HashMap<String, &'a gpu::compute::Image<B>, S>,
) -> Vec<gpu::DescriptorSetWrite<'a, B, Vec<gpu::Descriptor<'a, B>>>> {
    use lang::Operator;

    debug_assert!(inputs.values().all(|i| i.get_view().is_some()));
    debug_assert!(outputs.values().all(|i| i.get_view().is_some()));

    match op {
        Operator::Image { .. } => vec![],
        Operator::Output { .. } => vec![],

        Operator::Blend(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, gpu::SubRange::WHOLE)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    inputs.get("background").unwrap().get_view().unwrap(),
                    gpu::Layout::ShaderReadOnlyOptimal,
                )],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 2,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    inputs.get("foreground").unwrap().get_view().unwrap(),
                    gpu::Layout::ShaderReadOnlyOptimal,
                )],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 3,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Sampler(sampler)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 4,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("color").unwrap().get_view().unwrap(),
                    gpu::Layout::General,
                )],
            },
        ],
        Operator::PerlinNoise(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, gpu::SubRange::WHOLE)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("noise").unwrap().get_view().unwrap(),
                    gpu::Layout::General,
                )],
            },
        ],
        Operator::Rgb(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, gpu::SubRange::WHOLE)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("color").unwrap().get_view().unwrap(),
                    gpu::Layout::General,
                )],
            },
        ],
        Operator::Grayscale(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, gpu::SubRange::WHOLE)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    inputs.get("color").unwrap().get_view().unwrap(),
                    gpu::Layout::ShaderReadOnlyOptimal,
                )],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 2,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Sampler(sampler)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 3,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("value").unwrap().get_view().unwrap(),
                    gpu::Layout::General,
                )],
            },
        ],
        Operator::Ramp(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, gpu::SubRange::WHOLE)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    inputs.get("factor").unwrap().get_view().unwrap(),
                    gpu::Layout::ShaderReadOnlyOptimal,
                )],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 2,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Sampler(sampler)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 3,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("color").unwrap().get_view().unwrap(),
                    gpu::Layout::General,
                )],
            },
        ],
        Operator::NormalMap(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, gpu::SubRange::WHOLE)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    inputs.get("height").unwrap().get_view().unwrap(),
                    gpu::Layout::ShaderReadOnlyOptimal,
                )],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 2,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Sampler(sampler)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 3,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("normal").unwrap().get_view().unwrap(),
                    gpu::Layout::General,
                )],
            },
        ],
    }
}

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

impl Uniforms for lang::Operator {
    fn uniforms(&self) -> &[u8] {
        use lang::Operator;

        match self {
            // Image and Output are special and don't have uniforms
            Operator::Image { .. } => &[],
            Operator::Output { .. } => &[],

            // Operators
            Operator::Blend(p) => p.as_bytes(),
            Operator::PerlinNoise(p) => p.as_bytes(),
            Operator::Rgb(p) => p.as_bytes(),
            Operator::Grayscale(p) => p.as_bytes(),
            Operator::Ramp(p) => p.as_bytes(),
            Operator::NormalMap(p) => p.as_bytes(),
        }
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
            if let Some(operator_shader) = OperatorShader::from_operator(&op) {
                let shader: gpu::Shader<B> = gpu.create_shader(operator_shader.spirv)?;
                let pipeline: gpu::compute::ComputePipeline<B> =
                    gpu.create_pipeline(&shader, operator_shader.layout)?;
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
}
