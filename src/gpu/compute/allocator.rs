use crate::gpu::{Backend, GPU};
use crate::lang;
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
type AllocId = u64;

/// A Chunk is a piece of VRAM of fixed size that can be allocated for some
/// image. The size is hardcoded as `CHUNK_SIZE`.
#[derive(Debug, Clone)]
struct Chunk {
    /// Offset of the chunk in the contiguous image memory region.
    offset: u64,
    /// Allocation currently occupying this chunk, if any
    alloc: Option<AllocId>,
}

#[derive(Debug, Error)]
pub enum AllocatorError {
    /// Failed to bind image to memory
    #[error("Failed to bind image to memory")]
    Bind(#[from] hal::device::BindError),
    /// Failed to create an image view
    #[error("Failed to create an image view")]
    ViewCreation(#[from] hal::image::ViewCreationError),
    /// Failed to create an image
    #[error("Failed to create an image")]
    ImageCreation(#[from] hal::image::CreationError),
    /// Failed to create a buffer
    #[error("Failed to create a buffer")]
    BufferCreation(#[from] hal::buffer::CreationError),
    /// Failed to find free memory for image
    #[error("Unable to find free memory for image")]
    OutOfMemory,
}

pub struct ComputeAllocator<B: Backend> {
    gpu: Arc<Mutex<GPU<B>>>,
    allocs: Cell<AllocId>,
    image_mem: ManuallyDrop<B::Memory>,
    image_mem_chunks: RefCell<Vec<Chunk>>,
}

impl<B> ComputeAllocator<B>
where
    B: Backend,
{
    /// Size of the image memory region, in bytes
    const IMAGE_MEMORY_SIZE: u64 = 1024 * 1024 * 128; // bytes

    /// Size of a single chunk, in bytes
    const CHUNK_SIZE: u64 = 256 * 256 * 4; // bytes

    /// Number of chunks in the image memory region
    const N_CHUNKS: u64 = Self::IMAGE_MEMORY_SIZE / Self::CHUNK_SIZE;

    pub fn new(gpu: Arc<Mutex<GPU<B>>>) -> Result<Self, hal::device::AllocationError> {
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
                .expect("Unable to find device local memory for compute")
                .into();
            lock.device
                .allocate_memory(memory_type, Self::IMAGE_MEMORY_SIZE)?
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
    pub fn find_free_memory(&self, bytes: u64) -> Option<(u64, Vec<usize>)> {
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
    pub fn allocate_memory(&self, chunks: &[usize]) -> AllocId {
        let alloc = self.allocs.get();
        for i in chunks {
            self.image_mem_chunks.borrow_mut()[*i].alloc = Some(alloc);
        }
        self.allocs.set(alloc.wrapping_add(1));
        alloc
    }

    /// Mark the given set of chunks as free. Memory freed here should no longer
    /// be used!
    pub fn free_memory(&self, alloc: AllocId) {
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

impl<B> Drop for ComputeAllocator<B>
where
    B: Backend,
{
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
#[derive(Clone)]
pub struct Alloc<B: Backend> {
    parent: Arc<Mutex<ComputeAllocator<B>>>,
    id: AllocId,
    offset: u64,
}

impl<B> Drop for Alloc<B>
where
    B: Backend,
{
    /// Allocations will free on drop
    fn drop(&mut self) {
        log::trace!("Release memory for allocation {}", self.id);
        self.parent.lock().unwrap().free_memory(self.id);
    }
}

/// A compute image, which may or may not be allocated.
pub struct Image<B: Backend> {
    parent: Arc<Mutex<ComputeAllocator<B>>>,
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
    pub fn new(
        device: &B::Device,
        parent: Arc<Mutex<ComputeAllocator<B>>>,
        size: u32,
        ty: lang::ImageType,
        transfer_dst: bool,
    ) -> Result<Self, AllocatorError> {
        // Determine formats and sizes
        let format = match ty {
            lang::ImageType::Grayscale => hal::format::Format::R32Sfloat,

            // We use Rgba16 internally on the GPU even though it wastes an
            // entire 16 bit wide channel. The reason here is that the Vulkan
            // spec does not require Rgb16 support. Many GPUs do support it but
            // some may not, and thus requiring it would impose an arbitrary
            // restriction. It might be possible to make this conditional on the
            // specific GPU.
            lang::ImageType::Rgb => hal::format::Format::Rgba16Sfloat,
        };
        let px_width = match format {
            hal::format::Format::R32Sfloat => 4,
            hal::format::Format::Rgba16Sfloat => 8,
            _ => panic!("Unsupported compute image format!"),
        };

        // Create device image
        let image = unsafe {
            device.create_image(
                hal::image::Kind::D2(size, size, 1, 1),
                1,
                format,
                hal::image::Tiling::Optimal,
                hal::image::Usage::SAMPLED
                    | if !transfer_dst {
                        hal::image::Usage::STORAGE
                    } else {
                        hal::image::Usage::TRANSFER_DST
                    }
                    | hal::image::Usage::TRANSFER_SRC,
                hal::image::ViewCapabilities::empty(),
            )
        }?;

        Ok(Self {
            parent,
            size,
            px_width,
            raw: ManuallyDrop::new(Arc::new(Mutex::new(image))),
            layout: Cell::new(hal::image::Layout::Undefined),
            access: Cell::new(hal::image::Access::empty()),
            view: ManuallyDrop::new(None),
            alloc: None,
            format,
        })
    }

    /// Bind this image to some region in the image memory.
    fn bind_memory(&mut self, offset: u64) -> Result<(), AllocatorError> {
        let mut raw_lock = self.raw.lock().unwrap();
        let parent_lock = self.parent.lock().unwrap();
        let gpu_lock = parent_lock.gpu.lock().unwrap();

        unsafe {
            gpu_lock
                .device
                .bind_image_memory(&parent_lock.image_mem, offset, &mut raw_lock)
        }?;

        // Create view once the image is bound
        let view = unsafe {
            gpu_lock.device.create_image_view(
                &raw_lock,
                hal::image::ViewKind::D2,
                self.format,
                hal::format::Swizzle::NO,
                COLOR_RANGE.clone(),
            )
        }?;
        unsafe {
            if let Some(view) = ManuallyDrop::take(&mut self.view) {
                gpu_lock.device.destroy_image_view(view);
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
    pub fn allocate_memory(&mut self) -> Result<(), AllocatorError> {
        debug_assert!(self.alloc.is_none());

        let parent_lock = self.parent.lock().unwrap();

        // Handle memory manager
        let bytes = self.size as u64 * self.size as u64 * self.px_width as u64;
        let (offset, chunks) = parent_lock
            .find_free_memory(bytes)
            .ok_or(AllocatorError::OutOfMemory)?;
        let alloc = parent_lock.allocate_memory(&chunks);

        log::trace!(
            "Allocated memory for {}x{} image ({} bytes, id {})",
            self.size,
            self.size,
            bytes,
            alloc,
        );

        self.alloc = Some(Alloc {
            parent: self.parent.clone(),
            id: alloc,
            offset,
        });

        // Drop parent lock before calling bind_memory
        drop(parent_lock);

        // Bind
        self.bind_memory(offset)?;

        Ok(())
    }

    /// Determine whether an Image is backed by Device memory
    pub fn is_backed(&self) -> bool {
        self.alloc.is_some()
    }

    /// Ensures that the image is backed. If no memory is currently allocated to
    /// it, new memory will be allocated. May fail if out of memory!
    pub fn ensure_alloc(&mut self) -> Result<(), AllocatorError> {
        if self.alloc.is_none() {
            return self.allocate_memory();
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

    /// Get the number of bytes occupied by this image
    pub fn get_bytes(&self) -> u32 {
        self.size * self.size * self.px_width as u32
    }

    /// Get the image format
    pub fn get_format(&self) -> hal::format::Format {
        self.format
    }
}

impl<B> Drop for Image<B>
where
    B: Backend,
{
    /// Drop the raw resource. Any allocated memory will only be dropped when
    /// the last reference to it drops.
    fn drop(&mut self) {
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
        let parent_lock = self.parent.lock().unwrap();
        let gpu_lock = parent_lock.gpu.lock().unwrap();

        unsafe {
            gpu_lock.device.destroy_image(image.into_inner().unwrap());
            if let Some(view) = ManuallyDrop::take(&mut self.view) {
                gpu_lock.device.destroy_image_view(view);
            }
        }
    }
}

/// Temporary buffers in compute memory. Contrary to images these buffers are
/// *always* allocated, for as long as they live. They dealloc automatically on
/// drop.
pub struct TempBuffer<B: Backend> {
    parent: Arc<Mutex<ComputeAllocator<B>>>,
    access: Cell<hal::buffer::Access>,
    raw: ManuallyDrop<B::Buffer>,
    _alloc: Alloc<B>,
}

impl<B> TempBuffer<B>
where
    B: Backend,
{
    pub fn new(
        device: &B::Device,
        parent: Arc<Mutex<ComputeAllocator<B>>>,
        bytes: u64,
    ) -> Result<Self, AllocatorError> {
        let alloc_lock = parent.lock().unwrap();

        let (offset, chunks) = alloc_lock
            .find_free_memory(bytes)
            .ok_or(AllocatorError::OutOfMemory)?;
        let mut buffer = unsafe { device.create_buffer(bytes, hal::buffer::Usage::STORAGE) }?;
        let alloc_id = alloc_lock.allocate_memory(&chunks);

        log::trace!(
            "Allocated memory for buffer ({} bytes, id {})",
            bytes,
            alloc_id,
        );

        unsafe { device.bind_buffer_memory(&alloc_lock.image_mem, offset, &mut buffer) }?;

        Ok(TempBuffer {
            parent: parent.clone(),
            access: Cell::new(hal::buffer::Access::empty()),
            raw: ManuallyDrop::new(buffer),
            _alloc: Alloc {
                parent: parent.clone(),
                id: alloc_id,
                offset,
            },
        })
    }

    /// Create an appropriate image barrier transition to a specified Access and
    /// Layout.
    pub fn barrier_to<'a>(&'a self, access: hal::buffer::Access) -> hal::memory::Barrier<'a, B> {
        let old_access = self.access.get();
        self.access.set(access);
        hal::memory::Barrier::Buffer {
            states: (old_access)..(access),
            target: &*self.raw,
            families: None,
            range: hal::buffer::SubRange::WHOLE,
        }
    }

    pub fn get_raw(&self) -> &B::Buffer {
        &*self.raw
    }
}

impl<B> Drop for TempBuffer<B>
where
    B: Backend,
{
    fn drop(&mut self) {
        let parent_lock = self.parent.lock().unwrap();
        let gpu_lock = parent_lock.gpu.lock().unwrap();

        unsafe {
            gpu_lock
                .device
                .destroy_buffer(ManuallyDrop::take(&mut self.raw))
        };
    }
}
