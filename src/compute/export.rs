use std::path::Path;

use image::Rgb;
use thiserror::Error;

use crate::{
    gpu,
    lang::{ColorSpace, ExportFormat, ImageType},
};

#[derive(Error, Debug)]
pub enum ExportError {
    #[error("Export IO failed with {0}")]
    IOError(#[from] std::io::Error),
    #[error("Unsupport export format for given image")]
    UnsupportedExportFormat,
    #[error("Image encoding failed with {0}")]
    EncodingError(#[from] image::ImageError),
    #[error("Unsupported bit depth or colorspace for image type")]
    UnsupportedBitDepthOrColorSpace,
    #[error("Unable to find compute image for export")]
    UnknownImage,
}

pub enum ConvertedImage {
    R8(u32, Vec<u8>),
    Rgb8(u32, Vec<u8>),
    R16(u32, Vec<u16>),
    Rgb16(u32, Vec<u16>),
    Rgb32(u32, Vec<Rgb<f32>>),
}

impl ConvertedImage {
    /// Converts an image from the GPU. If the input image type is Rgb, a reverse
    /// gamma curve will be applied such that the output image matches what is
    /// displayed in the renderers.
    pub fn new(
        raw: &[u8],
        size: u32,
        color_space: ColorSpace,
        bit_depth: u8,
        ty: ImageType,
    ) -> Result<Self, ExportError> {
        fn to_16bit(x: f32) -> u16 {
            (x.clamp(0., 1.) * 65535.) as u16
        }

        fn to_16bit_gamma(x: f32) -> u16 {
            (x.powf(1.0 / 2.2).clamp(0., 1.) * 65535.) as u16
        }

        fn to_8bit(x: f32) -> u8 {
            (x.clamp(0., 1.) * 256.) as u8
        }

        fn to_8bit_gamma(x: f32) -> u8 {
            (x.powf(1.0 / 2.2).clamp(0., 1.) * 256.) as u8
        }

        match (bit_depth, color_space, ty) {
            (8, ColorSpace::Linear, ImageType::Grayscale) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u8s: Vec<u8> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                        .iter()
                        .map(|x| to_8bit(*x))
                        .collect()
                };
                Ok(ConvertedImage::R8(size, u8s))
            }
            (8, ColorSpace::Srgb, ImageType::Grayscale) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u8s: Vec<u8> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                        .iter()
                        .map(|x| to_8bit_gamma(*x))
                        .collect()
                };
                Ok(ConvertedImage::R8(size, u8s))
            }
            (8, ColorSpace::Linear, ImageType::Rgb) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u8s: Vec<u8> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                        .chunks(4)
                        .map(|chunk| {
                            vec![
                                to_8bit(chunk[0].to_f32()),
                                to_8bit(chunk[1].to_f32()),
                                to_8bit(chunk[2].to_f32()),
                            ]
                        })
                        .flatten()
                        .collect()
                };
                Ok(ConvertedImage::Rgb8(size, u8s))
            }
            (8, ColorSpace::Srgb, ImageType::Rgb) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u8s: Vec<u8> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                        .chunks(4)
                        .map(|chunk| {
                            vec![
                                to_8bit_gamma(chunk[0].to_f32()),
                                to_8bit_gamma(chunk[1].to_f32()),
                                to_8bit_gamma(chunk[2].to_f32()),
                            ]
                        })
                        .flatten()
                        .collect()
                };
                Ok(ConvertedImage::Rgb8(size, u8s))
            }
            (16, ColorSpace::Linear, ImageType::Grayscale) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u16s: Vec<u16> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                        .iter()
                        .map(|x| to_16bit(*x).to_be())
                        .collect()
                };
                Ok(ConvertedImage::R16(size, u16s))
            }
            (16, ColorSpace::Srgb, ImageType::Grayscale) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u16s: Vec<u16> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                        .iter()
                        .map(|x| to_16bit_gamma(*x).to_be())
                        .collect()
                };
                Ok(ConvertedImage::R16(size, u16s))
            }
            (16, ColorSpace::Linear, ImageType::Rgb) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u16s: Vec<u16> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                        .chunks(4)
                        .map(|chunk| {
                            vec![
                                to_16bit(chunk[0].to_f32()).to_be(),
                                to_16bit(chunk[1].to_f32()).to_be(),
                                to_16bit(chunk[2].to_f32()).to_be(),
                            ]
                        })
                        .flatten()
                        .collect()
                };
                Ok(ConvertedImage::Rgb16(size, u16s))
            }
            (16, ColorSpace::Srgb, ImageType::Rgb) => {
                #[allow(clippy::cast_ptr_alignment)]
                let u16s: Vec<u16> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                        .chunks(4)
                        .map(|chunk| {
                            vec![
                                to_16bit_gamma(chunk[0].to_f32()).to_be(),
                                to_16bit_gamma(chunk[1].to_f32()).to_be(),
                                to_16bit_gamma(chunk[2].to_f32()).to_be(),
                            ]
                        })
                        .flatten()
                        .collect()
                };
                Ok(ConvertedImage::Rgb16(size, u16s))
            }
            (32, ColorSpace::Linear, ImageType::Grayscale) => {
                #[allow(clippy::cast_ptr_alignment)]
                let f32s: Vec<Rgb<f32>> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                        .iter()
                        .map(|v| Rgb([*v, *v, *v]))
                        .collect()
                };
                Ok(ConvertedImage::Rgb32(size, f32s))
            }
            (32, ColorSpace::Linear, ImageType::Rgb) => {
                #[allow(clippy::cast_ptr_alignment)]
                let f32s: Vec<Rgb<f32>> = unsafe {
                    std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                        .chunks(4)
                        .map(|chunk| Rgb([chunk[0].to_f32(), chunk[1].to_f32(), chunk[2].to_f32()]))
                        .collect()
                };
                Ok(ConvertedImage::Rgb32(size, f32s))
            }
            _ => Err(ExportError::UnsupportedBitDepthOrColorSpace),
        }
    }

    /// Save this image to a file, using a given format. The format is *not*
    /// inferred from the path!
    pub fn save_to_file<P: AsRef<Path>>(
        &self,
        format: ExportFormat,
        path: P,
    ) -> Result<(), ExportError> {
        let mut writer = std::fs::File::create(path)?;
        match (self, format) {
            (ConvertedImage::R8(size, data), ExportFormat::Png) => {
                use image::codecs::png;
                let enc = png::PngEncoder::new(writer);
                enc.encode(data, *size, *size, image::ColorType::L8)?;
            }
            (ConvertedImage::R8(size, data), ExportFormat::Jpeg) => {
                use image::codecs::jpeg;
                let mut enc = jpeg::JpegEncoder::new(&mut writer);
                enc.encode(data, *size, *size, image::ColorType::L8)?;
            }
            (ConvertedImage::R8(size, data), ExportFormat::Tiff) => {
                use image::codecs::tiff;
                let enc = tiff::TiffEncoder::new(writer);
                enc.encode(data, *size, *size, image::ColorType::L8)?;
            }
            (ConvertedImage::R8(size, data), ExportFormat::Tga) => {
                use image::codecs::tga;
                let enc = tga::TgaEncoder::new(writer);
                enc.encode(data, *size, *size, image::ColorType::L8)?;
            }
            (ConvertedImage::Rgb8(size, data), ExportFormat::Png) => {
                use image::codecs::png;
                let enc = png::PngEncoder::new(writer);
                enc.encode(data, *size, *size, image::ColorType::Rgb8)?;
            }
            (ConvertedImage::Rgb8(size, data), ExportFormat::Jpeg) => {
                use image::codecs::jpeg;
                let mut enc = jpeg::JpegEncoder::new(&mut writer);
                enc.encode(data, *size, *size, image::ColorType::Rgb8)?;
            }
            (ConvertedImage::Rgb8(size, data), ExportFormat::Tiff) => {
                use image::codecs::tiff;
                let enc = tiff::TiffEncoder::new(writer);
                enc.encode(data, *size, *size, image::ColorType::Rgb8)?;
            }
            (ConvertedImage::Rgb8(size, data), ExportFormat::Tga) => {
                use image::codecs::tga;
                let enc = tga::TgaEncoder::new(writer);
                enc.encode(data, *size, *size, image::ColorType::Rgb8)?;
            }
            (ConvertedImage::R16(size, data), ExportFormat::Png) => {
                use image::codecs::png;
                let enc = png::PngEncoder::new(writer);
                let u8data = unsafe {
                    std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2)
                };
                enc.encode(u8data, *size, *size, image::ColorType::L16)?;
            }
            (ConvertedImage::Rgb16(size, data), ExportFormat::Png) => {
                use image::codecs::png;
                let enc = png::PngEncoder::new(writer);
                let u8data = unsafe {
                    std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2)
                };
                enc.encode(u8data, *size, *size, image::ColorType::Rgb16)?;
            }
            (ConvertedImage::Rgb32(size, data), ExportFormat::Hdr) => {
                use image::codecs::hdr;
                let enc = hdr::HdrEncoder::new(writer);
                enc.encode(data, *size as _, *size as _)?;
            }
            _ => return Err(ExportError::UnsupportedExportFormat),
        }

        Ok(())
    }
}
