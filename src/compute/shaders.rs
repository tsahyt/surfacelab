use crate::{gpu, lang};
use std::collections::HashMap;
use zerocopy::AsBytes;

// Blend
static BLEND_SHADER: &[u8] = include_bytes!("../../shaders/blend.spv");
static BLEND_LAYOUT: &[gpu::DescriptorSetLayoutBinding] = &[
    gpu::DescriptorSetLayoutBinding {
        binding: 0,
        ty: gpu::DescriptorType::UniformBuffer,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
    gpu::DescriptorSetLayoutBinding {
        binding: 1,
        ty: gpu::DescriptorType::SampledImage,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
    gpu::DescriptorSetLayoutBinding {
        binding: 2,
        ty: gpu::DescriptorType::SampledImage,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
    gpu::DescriptorSetLayoutBinding {
        binding: 3,
        ty: gpu::DescriptorType::StorageImage,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
];

// Perlin Noise
static PERLIN_NOISE_SHADER: &[u8] = include_bytes!("../../shaders/perlin.spv");
static PERLIN_NOISE_LAYOUT: &[gpu::DescriptorSetLayoutBinding] = &[
    gpu::DescriptorSetLayoutBinding {
        binding: 0,
        ty: gpu::DescriptorType::UniformBuffer,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
    gpu::DescriptorSetLayoutBinding {
        binding: 1,
        ty: gpu::DescriptorType::StorageImage,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
];

// Rgb
static RGB_SHADER: &[u8] = include_bytes!("../../shaders/rgb.spv");
static RGB_LAYOUT: &[gpu::DescriptorSetLayoutBinding] = &[
    gpu::DescriptorSetLayoutBinding {
        binding: 0,
        ty: gpu::DescriptorType::UniformBuffer,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
    gpu::DescriptorSetLayoutBinding {
        binding: 1,
        ty: gpu::DescriptorType::StorageImage,
        count: 1,
        stage_flags: gpu::ShaderStageFlags::COMPUTE,
        immutable_samplers: false,
    },
];

fn operator_shader_src<'a>(op: &'a lang::Operator) -> Option<&'static [u8]> {
    use lang::Operator;

    let src = match op {
        // Image and Output are special
        Operator::Image { .. } => return None,
        Operator::Output { .. } => return None,

        // Operators
        Operator::Blend(..) => BLEND_SHADER,
        Operator::PerlinNoise(..) => PERLIN_NOISE_SHADER,
        Operator::Rgb(..) => RGB_SHADER,
    };

    Some(src)
}

fn operator_layout<'a>(
    op: &'a lang::Operator,
) -> Option<&'static [gpu::DescriptorSetLayoutBinding]> {
    use lang::Operator;

    let bindings = match op {
        // Image and Output are special
        Operator::Image { .. } => return None,
        Operator::Output { .. } => return None,

        Operator::Blend(..) => BLEND_LAYOUT,
        Operator::PerlinNoise(..) => PERLIN_NOISE_LAYOUT,
        Operator::Rgb(..) => RGB_LAYOUT,
    };

    Some(bindings)
}

pub fn operator_write_desc<'a, B: gpu::Backend>(
    op: &lang::Operator,
    desc_set: &'a B::DescriptorSet,
    uniforms: &'a B::Buffer,
    inputs: &HashMap<String, &'a gpu::compute::Image<B>>,
    outputs: &HashMap<String, &'a gpu::compute::Image<B>>,
) -> Vec<gpu::DescriptorSetWrite<'a, B, Vec<gpu::Descriptor<'a, B>>>> {
    use lang::Operator;

    match op {
        Operator::Image { .. } => vec![],
        Operator::Output { .. } => vec![],

        Operator::Blend(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, None..None)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    inputs.get("color1").unwrap().get_view(),
                    gpu::Layout::ShaderReadOnlyOptimal,
                )],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    inputs.get("color2").unwrap().get_view(),
                    gpu::Layout::ShaderReadOnlyOptimal,
                )],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("color").unwrap().get_view(),
                    gpu::Layout::General,
                )],
            },
        ],
        Operator::PerlinNoise(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, None..None)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("noise").unwrap().get_view(),
                    gpu::Layout::General,
                )],
            },
        ],
        Operator::Rgb(..) => vec![
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Buffer(uniforms, None..None)],
            },
            gpu::DescriptorSetWrite {
                set: desc_set,
                binding: 1,
                array_offset: 0,
                descriptors: vec![gpu::Descriptor::Image(
                    outputs.get("color").unwrap().get_view(),
                    gpu::Layout::General,
                )],
            },
        ]
    }
}

pub trait Uniforms {
    fn uniforms<'a>(&'a self) -> &'a [u8];
}

impl Uniforms for lang::Operator {
    fn uniforms<'a>(&'a self) -> &'a [u8] {
        use lang::Operator;

        match self {
            // Image and Output are special and don't have uniforms
            Operator::Image { .. } => &[],
            Operator::Output { .. } => &[],

            // Operators
            Operator::Blend(p) => p.as_bytes(),
            Operator::PerlinNoise(p) => p.as_bytes(),
            Operator::Rgb(p) => p.as_bytes()
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
            if let Some(shader_src) = operator_shader_src(&op) {
                let shader: gpu::Shader<B> = gpu.create_shader(shader_src)?;
                let layout = operator_layout(&op).ok_or("Failed to fetch Operator layout")?;
                let pipeline: gpu::compute::ComputePipeline<B> =
                    gpu.create_pipeline(&shader, layout)?;
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
