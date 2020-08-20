use crate::lang::*;

pub struct FillLayer {

}

pub struct FxLayer {

}

pub enum Layer {
    FillLayer(FillLayer),
    FxLayer(FxLayer),
}
