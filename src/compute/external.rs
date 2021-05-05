use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Debug,
    path::{Path, PathBuf},
};
use thiserror::Error;

use crate::{
    lang::{resource::Svg, ColorSpace, Img, Resource},
    util::*,
};

#[derive(Debug, Error)]
pub enum ExternalError {
    #[error("External data operation failed during IO: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Error working with external image: {0}")]
    ImageError(#[from] image::ImageError),
}

pub enum Source {
    Packed(Vec<u8>),
    Disk(std::path::PathBuf),
}

impl Source {
    /// Pack the source data. If the source is on disk, it will be loaded and
    /// stored internally. This operation is idempotent.
    pub fn pack(&mut self) -> Result<(), ExternalError> {
        match self {
            Self::Packed(..) => Ok(()),
            Self::Disk(path) => {
                *self = Self::Packed(std::fs::read(path)?);
                Ok(())
            }
        }
    }

    /// Get the data from this source. If the data is packed, it will simply be
    /// borrowed, otherwise IO is performed to get the data from disk.
    pub fn get_data(&self) -> Result<Cow<Vec<u8>>, ExternalError> {
        match self {
            Self::Packed(data) => Ok(Cow::Borrowed(data)),
            Self::Disk(path) => Ok(Cow::Owned(std::fs::read(path)?)),
        }
    }
}

pub trait External {
    type Extra;

    fn fill_buffer(&mut self, raw: &[u8]) -> Result<Vec<u16>, ExternalError>;
    fn get_extra(&self) -> Self::Extra;
}

pub struct ExternalData<T: External> {
    /// Aligned buffer
    buffer: Option<Vec<u16>>,

    /// Source of the data
    source: Source,

    /// Satellite data depending on the type of data being held, e.g. colorspace information
    satellite: T,
}

impl<T> ExternalData<T>
where
    T: External,
{
    /// Create new external data from disk with the given satellite data.
    pub fn new(path: std::path::PathBuf, satellite: T) -> Self {
        Self {
            buffer: None,
            source: Source::Disk(path),
            satellite,
        }
    }

    /// Like `new` but for packed data.
    pub fn new_packed(data: Vec<u8>, satellite: T) -> Self {
        Self {
            buffer: None,
            source: Source::Packed(data),
            satellite,
        }
    }

    /// Determines whether this data requires (re)loading.
    pub fn needs_loading(&self) -> bool {
        self.buffer.is_none()
    }

    /// Pack the external data.
    pub fn pack(&mut self) -> Result<(), ExternalError> {
        self.source.pack()
    }

    /// Get a reference to the external data's satellite data.
    pub fn satellite(&self) -> &T {
        &self.satellite
    }

    /// Update the satellite data. This will reset the internal buffer and
    /// require reloading.
    pub fn update_satellite<F: FnMut(&mut T)>(&mut self, mut update: F) {
        update(&mut self.satellite);
        self.buffer = None;
    }

    /// Ensure that the internal buffer is filled, according to the satellite
    /// data. Returns a reference to the buffer on success.
    pub fn ensure_loaded(&mut self) -> Result<(&[u16], T::Extra), ExternalError> {
        if self.buffer.is_none() {
            let raw = self.source.get_data()?;
            let buf = self.satellite.fill_buffer(&raw)?;
            self.buffer = Some(buf);
        }

        Ok((self.buffer.as_ref().unwrap(), self.satellite.get_extra()))
    }

    /// Get a reference to the external data's source.
    pub fn source(&self) -> &Source {
        &self.source
    }
}

pub struct ImageData {
    color_space: ColorSpace,
    dimensions: u32,
}

impl ImageData {
    /// Get a reference to the image data's dimensions.
    pub fn dimensions(&self) -> u32 {
        self.dimensions
    }

    /// Get a reference to the image data's color space.
    pub fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    /// Set the image data's color space.
    pub fn set_color_space(&mut self, color_space: ColorSpace) {
        self.color_space = color_space;
    }
}

impl External for ImageData {
    type Extra = u32;

    fn fill_buffer(&mut self, raw: &[u8]) -> Result<Vec<u16>, ExternalError> {
        use image::GenericImageView;

        let img = image::load_from_memory(raw)?;
        self.dimensions = img.width().max(img.height());

        Ok(match self.color_space {
            ColorSpace::Srgb => load_rgba16f_image(&img, f16_from_u8_gamma, f16_from_u16_gamma),
            ColorSpace::Linear => load_rgba16f_image(&img, f16_from_u8, f16_from_u16),
        })
    }

    fn get_extra(&self) -> Self::Extra {
        self.dimensions
    }
}

impl Default for ImageData {
    fn default() -> Self {
        Self {
            color_space: ColorSpace::Srgb,
            dimensions: 1024,
        }
    }
}

pub struct SvgData {}

impl External for SvgData {
    type Extra = ();

    fn fill_buffer(&mut self, _raw: &[u8]) -> Result<Vec<u16>, ExternalError> {
        todo!()
    }

    fn get_extra(&self) -> Self::Extra {}
}

pub struct Externals {
    images: HashMap<Resource<Img>, ExternalData<ImageData>>,
    svgs: HashMap<Resource<Svg>, ExternalData<SvgData>>,
}

impl Externals {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.images.clear();
        self.svgs.clear();
    }

    /// Get a reference to an external image if it exists.
    pub fn get_image(&self, resource: &Resource<Img>) -> Option<&ExternalData<ImageData>> {
        self.images.get(resource)
    }

    /// Get a mutable reference to an external image if it exists.
    pub fn get_image_mut(
        &mut self,
        resource: &Resource<Img>,
    ) -> Option<&mut ExternalData<ImageData>> {
        self.images.get_mut(resource)
    }

    /// Insert a new image
    pub fn insert_image<P: AsRef<Path> + Debug>(
        &mut self,
        resource: Resource<Img>,
        path: P,
        color_space: ColorSpace,
    ) {
        self.images.insert(
            resource,
            ExternalData::new(
                PathBuf::from(path.as_ref()),
                ImageData {
                    color_space,
                    ..ImageData::default()
                },
            ),
        );
    }

    /// Insert a new packed image
    pub fn insert_image_packed(
        &mut self,
        resource: Resource<Img>,
        data: Vec<u8>,
        color_space: ColorSpace,
    ) {
        self.images.insert(
            resource,
            ExternalData::new_packed(
                data,
                ImageData {
                    color_space,
                    ..ImageData::default()
                },
            ),
        );
    }

    /// Obtain an iterator over all known images
    pub fn iter_images(&self) -> impl Iterator<Item = (&Resource<Img>, &ExternalData<ImageData>)> {
        self.images.iter()
    }
}

impl Default for Externals {
    fn default() -> Self {
        Self {
            images: HashMap::new(),
            svgs: HashMap::new(),
        }
    }
}

/// Load an image from a dynamic image into a u16 buffer with f16 encoding, using the
/// provided sampling functions. Those functions can be used to alter each
/// sample if necessary, e.g. to perform gamma correction.
fn load_rgba16f_image<F: Fn(u8) -> u16, G: Fn(u16) -> u16>(
    img: &image::DynamicImage,
    sample8: F,
    sample16: G,
) -> Vec<u16> {
    use image::GenericImageView;

    let mut loaded: Vec<u16> = Vec::with_capacity(img.width() as usize * img.height() as usize * 4);

    match img {
        image::DynamicImage::ImageLuma8(buf) => {
            for image::Luma([l]) in buf.pixels() {
                let x = sample8(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(255);
            }
        }
        image::DynamicImage::ImageLumaA8(buf) => {
            for image::LumaA([l, a]) in buf.pixels() {
                let x = sample8(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(sample8(*a));
            }
        }
        image::DynamicImage::ImageRgb8(buf) => {
            for image::Rgb([r, g, b]) in buf.pixels() {
                loaded.push(sample8(*r));
                loaded.push(sample8(*g));
                loaded.push(sample8(*b));
                loaded.push(sample8(255));
            }
        }
        image::DynamicImage::ImageRgba8(buf) => {
            for sample in buf.as_flat_samples().as_slice().iter() {
                loaded.push(sample8(*sample))
            }
        }
        image::DynamicImage::ImageBgr8(buf) => {
            for image::Bgr([b, g, r]) in buf.pixels() {
                loaded.push(sample8(*r));
                loaded.push(sample8(*g));
                loaded.push(sample8(*b));
                loaded.push(sample8(255));
            }
        }
        image::DynamicImage::ImageBgra8(buf) => {
            for image::Bgra([b, g, r, a]) in buf.pixels() {
                loaded.push(sample8(*r));
                loaded.push(sample8(*g));
                loaded.push(sample8(*b));
                loaded.push(sample8(*a));
            }
        }
        image::DynamicImage::ImageLuma16(buf) => {
            for image::Luma([l]) in buf.pixels() {
                let x = sample16(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(255);
            }
        }
        image::DynamicImage::ImageLumaA16(buf) => {
            for image::LumaA([l, a]) in buf.pixels() {
                let x = sample16(*l);
                loaded.push(x);
                loaded.push(x);
                loaded.push(x);
                loaded.push(sample16(*a));
            }
        }
        image::DynamicImage::ImageRgb16(buf) => {
            for image::Rgb([r, g, b]) in buf.pixels() {
                loaded.push(sample16(*r));
                loaded.push(sample16(*g));
                loaded.push(sample16(*b));
                loaded.push(sample16(255));
            }
        }
        image::DynamicImage::ImageRgba16(buf) => {
            for sample in buf.as_flat_samples().as_slice().iter() {
                loaded.push(sample16(*sample))
            }
        }
    }

    loaded
}
