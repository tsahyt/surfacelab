use crate::lang::*;
use serde_derive::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{
    interpreter::{ExternalImage, ExternalImageSource},
    ComputeManager,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum StoredExternalImage {
    Disk {
        resource: Resource<Img>,
        path: PathBuf,
        color_space: ColorSpace,
    },
    Packed {
        resource: Resource<Img>,
        data: Vec<u8>,
        color_space: ColorSpace,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComputeData {
    stored_images: Vec<StoredExternalImage>,
}

impl<B> ComputeManager<B>
where
    B: crate::gpu::Backend,
{
    /// Serialize contained data into plain old data
    pub fn serialize(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        log::info!("Serializing compute data");
        let stored_images = self
            .external_images
            .iter()
            .map(|(res, ext_image)| {
                let color_space = ext_image.color_space;
                match &ext_image.source {
                    ExternalImageSource::Packed(img) => {
                        let mut cursor = std::io::Cursor::new(Vec::new());
                        img.write_to(&mut cursor, image::ImageOutputFormat::Png)
                            .expect("Error writing packed image");
                        let data = cursor.into_inner();

                        StoredExternalImage::Packed {
                            resource: res.clone(),
                            data,
                            color_space,
                        }
                    }
                    ExternalImageSource::Disk(path) => StoredExternalImage::Disk {
                        resource: res.clone(),
                        path: path.clone(),
                        color_space,
                    },
                }
            })
            .collect();

        let compute_data = ComputeData { stored_images };

        serde_cbor::ser::to_vec_packed(&compute_data)
    }

    /// Deserialize plain old data into self. This will not reset self!
    pub fn deserialize(&mut self, data: &[u8]) -> Result<Vec<Lang>, serde_cbor::Error> {
        log::info!("Deserializing compute data");
        let mut compute_data: ComputeData = serde_cbor::de::from_slice(data)?;
        let mut evs = Vec::new();

        for stored_image in compute_data.stored_images.drain(0..) {
            match stored_image {
                StoredExternalImage::Disk {
                    resource,
                    path,
                    color_space,
                } => {
                    evs.push(Lang::ComputeEvent(ComputeEvent::ImageResourceAdded(
                        resource.clone(),
                        color_space,
                        false,
                    )));
                    self.external_images
                        .insert(resource, ExternalImage::new(path, color_space));
                }
                StoredExternalImage::Packed {
                    resource,
                    data,
                    color_space,
                } => {
                    evs.push(Lang::ComputeEvent(ComputeEvent::ImageResourceAdded(
                        resource.clone(),
                        color_space,
                        true,
                    )));
                    let image = image::load_from_memory_with_format(&data, image::ImageFormat::Png)
                        .expect("Error deserializing packed image");
                    self.external_images
                        .insert(resource, ExternalImage::new_packed(image, color_space));
                }
            }
        }

        Ok(evs)
    }
}
