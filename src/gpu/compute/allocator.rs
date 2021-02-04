use crate::gpu::{GPU, InitializationError};
use super::{Backend, GPUCompute};

use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub const COLOR_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    levels: 0..1,
    layers: 0..1,
};

/// Memory allocation ID.
type AllocId = u32;

/// A Chunk is a piece of VRAM of fixed size that can be allocated for some
/// image. The size is hardcoded as `CHUNK_SIZE`.
#[derive(Debug, Clone)]
struct Chunk {
    /// Offset of the chunk in the contiguous image memory region.
    offset: u64,
    /// Allocation currently occupying this chunk, if any
    alloc: Option<AllocId>,
}

pub struct ComputeAllocator<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    allocs: Cell<AllocId>,
    image_mem: ManuallyDrop<B::Memory>,
    image_mem_chunks: RefCell<Vec<Chunk>>,
}

impl<B> ComputeAllocator<B> where B: Backend {
    /// Size of the image memory region, in bytes
    const IMAGE_MEMORY_SIZE: u64 = 1024 * 1024 * 128; // bytes

    /// Size of a single chunk, in bytes
    const CHUNK_SIZE: u64 = 256 * 256 * 4; // bytes

    /// Number of chunks in the image memory region
    const N_CHUNKS: u64 = Self::IMAGE_MEMORY_SIZE / Self::CHUNK_SIZE;

    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, InitializationError> {
        let lock = gpu.lock().unwrap();

        // Preallocate a block of memory for compute images in device local
        // memory. This serves as memory for all images used in compute other
        // than for image nodes, which are uploaded separately.
        let image_mem = unsafe {
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
            lock.device
                .allocate_memory(memory_type, Self::IMAGE_MEMORY_SIZE)
                .map_err(|_| InitializationError::Allocation("Image Memory"))?
        };

        Ok(Self {
            gpu: gpu.clone(),
            allocs: Cell::new(0),
            image_mem: ManuallyDrop::new(image_mem),
            image_mem_chunks: RefCell::new(
                (0..Self::N_CHUNKS)
                    .map(|id| Chunk {
                        offset: Self::CHUNK_SIZE * id,
                        alloc: None,
                    })
                    .collect(),
            ),
        })
    }

    /// Find the first set of chunks of contiguous free memory that fits the
    /// requested number of bytes
    fn find_free_image_memory(&self, bytes: u64) -> Option<(u64, Vec<usize>)> {
        let request = bytes.max(Self::CHUNK_SIZE) / Self::CHUNK_SIZE;
        let mut free = Vec::with_capacity(request as usize);
        let mut offset = 0;

        for (i, chunk) in self.image_mem_chunks.borrow().iter().enumerate() {
            if chunk.alloc.is_none() {
                free.push(i);
                if free.len() == request as usize {
                    return Some((offset, free));
                }
            } else {
                offset = (i + 1) as u64 * Self::CHUNK_SIZE;
                free.clear();
            }
        }

        None
    }

    /// Mark the given set of chunks as used. Assumes that the chunks were
    /// previously free!
    fn allocate_image_memory(&self, chunks: &[usize]) -> AllocId {
        let alloc = self.allocs.get();
        for i in chunks {
            self.image_mem_chunks.borrow_mut()[*i].alloc = Some(alloc);
        }
        self.allocs.set(alloc.wrapping_add(1));
        alloc
    }

    /// Mark the given set of chunks as free. Memory freed here should no longer
    /// be used!
    fn free_image_memory(&self, alloc: AllocId) {
        for mut chunk in self
            .image_mem_chunks
            .borrow_mut()
            .iter_mut()
            .filter(|c| c.alloc == Some(alloc))
        {
            chunk.alloc = None;
        }
    }
}

impl<B> Drop for ComputeAllocator<B> where B: Backend {
    fn drop(&mut self) {
        log::info!("Releasing GPU Compute allocator");

        let lock = self.gpu.lock().unwrap();

        unsafe {
            lock.device
                .free_memory(ManuallyDrop::take(&mut self.image_mem));
        }
    }
}

/// An allocation in the compute image memory.
#[derive(Debug, Clone)]
pub struct Alloc<B: Backend> {
    parent: *const GPUCompute<B>,
    id: AllocId,
    offset: u64,
}

impl<B> Drop for Alloc<B>
where
    B: Backend,
{
    /// Allocations will free on drop
    fn drop(&mut self) {
        log::trace!("Release image memory for allocation {}", self.id);
        let parent = unsafe { &*self.parent };
        parent.free_image_memory(self.id);
    }
}

#[derive(Debug, Error)]
pub enum ImageError {
    /// Failed to bind image to memory
    #[error("Failed to bind image to memory")]
    Bind,
    /// Failed to create an image view
    #[error("Failed to create an image view")]
    ViewCreation,
    /// Failed to find free memory for image
    #[error("Unable to find free memory for image")]
    OutOfMemory,
}

/// A compute image, which may or may not be allocated.
pub struct Image<B: Backend> {
    parent: *const GPUCompute<B>,
    size: u32,
    px_width: u8,
    raw: ManuallyDrop<Arc<Mutex<B::Image>>>,
    layout: Cell<hal::image::Layout>,
    access: Cell<hal::image::Access>,
    view: ManuallyDrop<Option<B::ImageView>>,
    alloc: Option<Alloc<B>>,
    format: hal::format::Format,
}

impl<B> Image<B>
where
    B: Backend,
{
    /// Bind this image to some region in the image memory.
    fn bind_memory(&mut self, offset: u64, compute: &GPUCompute<B>) -> Result<(), ImageError> {
        let mut raw_lock = self.raw.lock().unwrap();
        let lock = compute.gpu.lock().unwrap();

        unsafe {
            lock.device
                .bind_image_memory(&compute.image_mem, offset, &mut raw_lock)
        }
        .map_err(|_| ImageError::Bind)?;

        // Create view once the image is bound
        let view = unsafe {
            lock.device.create_image_view(
                &raw_lock,
                hal::image::ViewKind::D2,
                self.format,
                hal::format::Swizzle::NO,
                COLOR_RANGE.clone(),
            )
        }
        .map_err(|_| ImageError::ViewCreation)?;
        unsafe {
            if let Some(view) = ManuallyDrop::take(&mut self.view) {
                lock.device.destroy_image_view(view);
            }
        }
        self.view = ManuallyDrop::new(Some(view));

        Ok(())
    }

    /// Create an appropriate image barrier transition to a specified Access and
    /// Layout.
    pub fn barrier_to<'a>(
        &self,
        image: &'a B::Image,
        access: hal::image::Access,
        layout: hal::image::Layout,
    ) -> hal::memory::Barrier<'a, B> {
        let old_access = self.access.get();
        let old_layout = self.layout.get();
        self.access.set(access);
        self.layout.set(layout);
        hal::memory::Barrier::Image {
            states: (old_access, old_layout)..(access, layout),
            target: image,
            families: None,
            range: COLOR_RANGE.clone(),
        }
    }

    /// Allocate fresh memory to the image from the underlying memory pool in compute.
    pub fn allocate_memory(&mut self, compute: &GPUCompute<B>) -> Result<(), ImageError> {
        debug_assert!(self.alloc.is_none());

        // Handle memory manager
        let bytes = self.size as u64 * self.size as u64 * self.px_width as u64;
        let (offset, chunks) = compute
            .find_free_image_memory(bytes)
            .ok_or(ImageError::OutOfMemory)?;
        let alloc = compute.allocate_image_memory(&chunks);

        log::trace!(
            "Allocated memory for {}x{} image ({} bytes, id {})",
            self.size,
            self.size,
            bytes,
            alloc,
        );

        self.alloc = Some(Alloc {
            parent: self.parent,
            id: alloc,
            offset,
        });

        // Bind
        self.bind_memory(offset, compute)?;

        Ok(())
    }

    /// Determine whether an Image is backed by Device memory
    pub fn is_backed(&self) -> bool {
        self.alloc.is_some()
    }

    /// Ensures that the image is backed. If no memory is currently allocated to
    /// it, new memory will be allocated. May fail if out of memory!
    pub fn ensure_alloc(&mut self, compute: &GPUCompute<B>) -> Result<(), ImageError> {
        if self.alloc.is_none() {
            return self.allocate_memory(compute);
        }

        log::trace!("Reusing existing allocation");

        Ok(())
    }

    /// Get a view to the image
    pub fn get_view(&self) -> Option<&B::ImageView> {
        match &*self.view {
            Some(view) => Some(view),
            None => None,
        }
    }

    /// Get the current layout of the image
    pub fn get_layout(&self) -> hal::image::Layout {
        self.layout.get()
    }

    /// Get the current image access flags
    pub fn get_access(&self) -> hal::image::Access {
        self.access.get()
    }

    /// Get size of this image
    pub fn get_size(&self) -> u32 {
        self.size
    }

    /// Get the raw image
    pub fn get_raw(&self) -> &Arc<Mutex<B::Image>> {
        &*self.raw
    }
}

impl<B> Drop for Image<B>
where
    B: Backend,
{
    /// Drop the raw resource. Any allocated memory will only be dropped when
    /// the last reference to it drops.
    fn drop(&mut self) {
        let parent = unsafe { &*self.parent };

        // Spinlock to acquire the image.
        let image = {
            let mut raw = unsafe { ManuallyDrop::take(&mut self.raw) };
            loop {
                match Arc::try_unwrap(raw) {
                    Ok(t) => break t,
                    Err(a) => raw = a,
                }
            }
        };

        // NOTE: Lock *after* having aquired the image, to avoid a deadlock
        // between here and the image copy in render
        let lock = parent.gpu.lock().unwrap();

        unsafe {
            lock.device.destroy_image(image.into_inner().unwrap());
            if let Some(view) = ManuallyDrop::take(&mut self.view) {
                lock.device.destroy_image_view(view);
            }
        }
    }
}
