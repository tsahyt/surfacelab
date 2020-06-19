use enum_dispatch::*;
use std::path::{Path, PathBuf};

#[enum_dispatch]
pub trait Parameters {
    fn set_parameter(&mut self, field: &'static str, data: &[u8]);
}

pub type ParameterBool = u32;

pub trait ParameterField {
    fn from_data(data: &[u8]) -> Self;
    fn to_data(&self) -> Vec<u8>;
}

impl ParameterField for f32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        f32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for u32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        u32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for i32 {
    fn from_data(data: &[u8]) -> Self {
        let mut arr: [u8; 4] = Default::default();
        arr.copy_from_slice(data);
        i32::from_be_bytes(arr)
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl ParameterField for [f32; 3] {
    fn from_data(data: &[u8]) -> Self {
        let cols: Vec<f32> = data
            .chunks(4)
            .map(|z| {
                let mut arr: [u8; 4] = Default::default();
                arr.copy_from_slice(z);
                f32::from_be_bytes(arr)
            })
            .collect();
        [cols[0], cols[1], cols[2]]
    }

    fn to_data(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(self[0] as f32).to_be_bytes());
        buf.extend_from_slice(&(self[1] as f32).to_be_bytes());
        buf.extend_from_slice(&(self[2] as f32).to_be_bytes());
        buf.extend_from_slice(&(1.0 as f32).to_be_bytes());
        buf
    }
}

impl ParameterField for PathBuf {
    fn from_data(data: &[u8]) -> Self {
        let path_str = unsafe { std::str::from_utf8_unchecked(&data) };
        Path::new(path_str).to_path_buf()
    }

    fn to_data(&self) -> Vec<u8> {
        self.to_str().unwrap().as_bytes().to_vec()
    }
}
