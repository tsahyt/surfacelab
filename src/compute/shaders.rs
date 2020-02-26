use crate::{gpu, lang};
use std::collections::HashMap;
use zerocopy::AsBytes;

fn operator_shader_src<'a>(op: &'a lang::Operator) -> Option<&'static [u8]> {
    use lang::Operator;

    let src = match op {
        // Image and Output are special
        Operator::Image { .. } => return None,
        Operator::Output { .. } => return None,

        // Operators
        Operator::Blend(..) => include_bytes!("../../shaders/blend.spv"),
        Operator::PerlinNoise(..) => include_bytes!("../../shaders/perlin.spv"),
    };

    Some(src)
}

pub fn operator_uniforms<'a>(op: &'a lang::Operator) -> &'a [u8] {
    use lang::Operator;

    match op {
        // Image and Output are special and don't have uniforms
        Operator::Image { .. } => &[],
        Operator::Output { .. } => &[],

        // Operators
        Operator::Blend(p) => p.as_bytes(),
        Operator::PerlinNoise(p) => p.as_bytes(),
    }
}

pub struct ShaderLibrary<B: gpu::Backend> {
    shaders: HashMap<&'static str, gpu::Shader<B>>,
}

impl<B> ShaderLibrary<B>
where
    B: gpu::Backend,
{
    pub fn new(gpu: &gpu::compute::GPUCompute<B>) -> Result<Self, String> {
        let mut hm = HashMap::new();
        for op in lang::Operator::all_default() {
            if let Some(shader_src) = operator_shader_src(&op) {
                let shader: gpu::Shader<B> = gpu.create_shader(shader_src)?;
                hm.insert(op.default_name(), shader);
            }
        }

        Ok(ShaderLibrary { shaders: hm })
    }

    pub fn shader_for(&self, op: &lang::Operator) -> &gpu::Shader<B> {
        debug_assert!(op.default_name() != "image" && op.default_name() != "output");
        self.shaders.get(op.default_name()).unwrap()
    }
}
