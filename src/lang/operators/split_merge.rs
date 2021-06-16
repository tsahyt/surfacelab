use super::super::parameters::*;
use super::super::socketed::*;
use crate::compute::shaders::*;
use crate::shader;

use maplit::hashmap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use surfacelab_derive::*;
use zerocopy::AsBytes;

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Split {}

impl Default for Split {
    fn default() -> Self {
        Self {}
    }
}

impl Socketed for Split {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "color".to_string() => (OperatorType::Monomorphic(ImageType::Rgb), false),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "x/r".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
            "y/g".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
            "z/b".to_string() => OperatorType::Monomorphic(ImageType::Grayscale),
        }
    }

    fn default_name(&self) -> &str {
        "split"
    }

    fn title(&self) -> &str {
        "Split"
    }
}

impl Shader for Split {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("split"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::InputImage("color"),
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::OutputImage("x/r"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::OutputImage("y/g"),
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::OutputImage("z/b"),
                },
            ],
            specialization: Specialization::default(),
            shape: OperatorShape::PerPixel {
                local_x: 8,
                local_y: 8,
            },
        })]
    }
}

impl OperatorParamBox for Split {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("split".to_string()),
            categories: vec![],
        }
    }
}

#[repr(C)]
#[derive(AsBytes, Clone, Copy, Debug, Serialize, Deserialize, Parameters, PartialEq)]
pub struct Merge {}

impl Default for Merge {
    fn default() -> Self {
        Self {}
    }
}

impl Socketed for Merge {
    fn inputs(&self) -> HashMap<String, (OperatorType, bool)> {
        hashmap! {
            "x/r".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), false),
            "y/g".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), false),
            "z/b".to_string() => (OperatorType::Monomorphic(ImageType::Grayscale), false),
        }
    }

    fn outputs(&self) -> HashMap<String, OperatorType> {
        hashmap! {
            "color".to_string() => OperatorType::Monomorphic(ImageType::Rgb)
        }
    }

    fn default_name(&self) -> &str {
        "merge"
    }

    fn title(&self) -> &str {
        "Merge"
    }
}

impl Shader for Merge {
    fn operator_passes(&self) -> Vec<OperatorPassDescription> {
        vec![OperatorPassDescription::RunShader(OperatorShader {
            spirv: shader!("merge"),
            descriptors: &[
                OperatorDescriptor {
                    binding: 0,
                    descriptor: OperatorDescriptorUse::InputImage("x/r"),
                },
                OperatorDescriptor {
                    binding: 1,
                    descriptor: OperatorDescriptorUse::InputImage("y/g"),
                },
                OperatorDescriptor {
                    binding: 2,
                    descriptor: OperatorDescriptorUse::InputImage("z/b"),
                },
                OperatorDescriptor {
                    binding: 3,
                    descriptor: OperatorDescriptorUse::Sampler,
                },
                OperatorDescriptor {
                    binding: 4,
                    descriptor: OperatorDescriptorUse::OutputImage("color"),
                },
            ],
            specialization: Specialization::default(),
            shape: OperatorShape::PerPixel {
                local_x: 8,
                local_y: 8,
            },
        })]
    }
}

impl OperatorParamBox for Merge {
    fn param_box_description(&self) -> ParamBoxDescription<Field> {
        ParamBoxDescription {
            box_title: self.title().to_string(),
            preset_tag: Some("merge".to_string()),
            categories: vec![],
        }
    }
}
