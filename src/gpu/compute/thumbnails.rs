use crate::gpu::{Backend, GPU};

use gfx_hal as hal;
use gfx_hal::prelude::*;
use smallvec::{smallvec, SmallVec};
use std::sync::{Arc, Mutex};

const COLOR_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    levels: 0..1,
    layers: 0..1,
};

/// An index into the thumbnail cache
#[derive(Debug)]
pub struct ThumbnailIndex(Option<usize>);

/// Drop implementation checks whether thumbnail was returned properly to the
/// cache before dropping. Failure to do so will cause a panic in Debug builds
/// only!
impl Drop for ThumbnailIndex {
    fn drop(&mut self) {
        debug_assert!(self.0.is_none());
    }
}

/// The thumbnail cache stores all thumbnails to be used in the system. They
/// reside in the compute component and are managed here.
///
/// The cache is dynamically sized. If more thumbnails are required, another
/// memory region will be allocated for them.
pub struct ThumbnailCache<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    memory: SmallVec<[B::Memory; 4]>,
    images: Vec<Option<B::Image>>,
    views: Vec<Option<Arc<Mutex<B::ImageView>>>>,
}

impl<B> Drop for ThumbnailCache<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        let n = self.memory.len() * Self::THUMBNAIL_CHUNK_LENGTH;
        for i in 0..n {
            self.free(ThumbnailIndex(Some(i)));
        }

        let lock = self.gpu.lock().unwrap();
        for chunk in self.memory.drain(0..self.memory.len()) {
            unsafe { lock.device.free_memory(chunk) };
        }
    }
}

impl<B> ThumbnailCache<B>
where
    B: Backend,
{
    /// Pixel per side
    pub const THUMBNAIL_SIZE: usize = 128;

    /// Size of a single thumbnail in bytes
    const THUMBNAIL_BYTES: usize = Self::THUMBNAIL_SIZE * Self::THUMBNAIL_SIZE * 4;

    /// Format of thumbnails
    const THUMBNAIL_FORMAT: hal::format::Format = hal::format::Format::Rgba8Unorm;

    /// Size of a single allocation, in number of thumbnails. 512 is roughly 32M in memory
    const THUMBNAIL_CHUNK_LENGTH: usize = 512;

    /// Swizzle setting for grayscale images
    const GRAYSCALE_SWIZZLE: hal::format::Swizzle = hal::format::Swizzle(
        hal::format::Component::R,
        hal::format::Component::R,
        hal::format::Component::R,
        hal::format::Component::A,
    );

    /// Create a new thumbnail cache
    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Self {
        let chunk = {
            let lock = gpu.lock().unwrap();

            let memory_type = lock
                .memory_properties
                .memory_types
                .iter()
                .position(|mem_type| {
                    mem_type
                        .properties
                        .contains(hal::memory::Properties::DEVICE_LOCAL)
                })
                .unwrap()
                .into();

            unsafe {
                lock.device.allocate_memory(
                    memory_type,
                    Self::THUMBNAIL_CHUNK_LENGTH as u64 * Self::THUMBNAIL_BYTES as u64,
                )
            }
            .expect("Error allocating thumbnail memory")
        };

        let memory = smallvec![chunk];

        Self {
            gpu,
            memory,
            images: (0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None).collect(),
            views: (0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None).collect(),
        }
    }

    /// Obtain the next free thumbnail index from the cache. This will set up
    /// all required internal data structures.
    pub fn next(&mut self, grayscale: bool) -> ThumbnailIndex {
        if let Some(i) = self
            .images
            .iter()
            .enumerate()
            .filter(|(_, x)| x.is_none())
            .map(|(i, _)| i)
            .next()
        {
            self.new_thumbnail_at(i, grayscale);
            ThumbnailIndex(Some(i))
        } else {
            self.grow();
            self.next(grayscale)
        }
    }

    fn new_thumbnail_at(&mut self, i: usize, grayscale: bool) {
        let lock = self.gpu.lock().unwrap();

        let mut image = unsafe {
            lock.device.create_image(
                hal::image::Kind::D2(Self::THUMBNAIL_SIZE as _, Self::THUMBNAIL_SIZE as _, 1, 1),
                1,
                Self::THUMBNAIL_FORMAT,
                hal::image::Tiling::Linear,
                hal::image::Usage::TRANSFER_DST | hal::image::Usage::SAMPLED,
                hal::image::ViewCapabilities::empty(),
            )
        }
        .expect("Error creating thumbnail image");

        let mem = &self.memory[i / Self::THUMBNAIL_CHUNK_LENGTH];
        let offset = (i % Self::THUMBNAIL_CHUNK_LENGTH) * Self::THUMBNAIL_BYTES;

        unsafe {
            lock.device
                .bind_image_memory(mem, offset as u64, &mut image)
        }
        .expect("Error binding thumbnail memory");

        let view = unsafe {
            lock.device.create_image_view(
                &image,
                hal::image::ViewKind::D2,
                Self::THUMBNAIL_FORMAT,
                if grayscale {
                    Self::GRAYSCALE_SWIZZLE
                } else {
                    hal::format::Swizzle::NO
                },
                COLOR_RANGE.clone(),
            )
        }
        .expect("Error creating thumbnail image view");

        self.images[i] = Some(image);
        self.views[i] = Some(Arc::new(Mutex::new(view)));
    }

    fn grow(&mut self) {
        let new_chunk = {
            let lock = self.gpu.lock().unwrap();

            let memory_type = lock
                .memory_properties
                .memory_types
                .iter()
                .position(|mem_type| {
                    mem_type
                        .properties
                        .contains(hal::memory::Properties::DEVICE_LOCAL)
                })
                .unwrap()
                .into();

            unsafe {
                lock.device.allocate_memory(
                    memory_type,
                    Self::THUMBNAIL_CHUNK_LENGTH as u64 * Self::THUMBNAIL_BYTES as u64,
                )
            }
            .expect("Error allocating thumbnail memory")
        };

        self.memory.push(new_chunk);

        self.images
            .extend((0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None));
        self.views
            .extend((0..Self::THUMBNAIL_CHUNK_LENGTH).map(|_| None));
    }

    /// Get the underlying Image from a thumbnail
    pub fn image(&self, index: &ThumbnailIndex) -> &B::Image {
        self.images[index.0.unwrap()].as_ref().unwrap()
    }

    /// Get the underlying image view from a thumbnail
    pub fn image_view(&self, index: &ThumbnailIndex) -> &Arc<Mutex<B::ImageView>> {
        self.views[index.0.unwrap()].as_ref().unwrap()
    }

    /// Free a thumbnail by its index. Note that this takes ownership.
    pub fn free(&mut self, mut index: ThumbnailIndex) {
        let idx = index.0.take().unwrap();
        let view = {
            let mut inner = self.views[idx].take().unwrap();
            loop {
                match Arc::try_unwrap(inner) {
                    Ok(t) => break t,
                    Err(a) => inner = a,
                }
            }
        };
        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device.destroy_image_view(view.into_inner().unwrap());
            lock.device.destroy_image(self.images[idx].take().unwrap());
        }
    }

    /// The size of a single thumbnail, measured in pixels per side.
    pub fn thumbnail_size(&self) -> usize {
        Self::THUMBNAIL_SIZE
    }
}
