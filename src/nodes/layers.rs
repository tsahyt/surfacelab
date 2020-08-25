use crate::lang::*;
use enumset::EnumSet;
use serde_derive::{Deserialize, Serialize};

pub struct FillLayer {
    mask: MaskStack,
    channels: EnumSet<MaterialChannel>,
    factor: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FxLayer {
    operator: Operator,
    channels: EnumSet<MaterialChannel>,
}

pub struct MaskStack {}

pub enum Layer {
    FillLayer(FillLayer),
    FxLayer(FxLayer),
}
