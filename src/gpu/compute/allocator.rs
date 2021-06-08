use crate::gpu::{Backend, GPU};
use crate::lang;
use gfx_hal as hal;
use gfx_hal::prelude::*;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use std::{cell::Cell, ops::Range};
use thiserror::Error;

pub const COLOR_RANGE: hal::image::SubresourceRange = hal::image::SubresourceRange {
    aspects: hal::format::Aspects::COLOR,
    level_start: 0,
    level_count: Some(1),
    layer_start: 0,
    layer_count: Some(1),
};

/// Memory allocation ID.
type AllocId = std::num::NonZeroU64;

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
    image_mem_chunks: Vec<Chunk>,
    usage: AllocatorUsage,
}

/// Struct holding usage statistics for the allocator
#[derive(Clone, Copy)]
pub struct AllocatorUsage {
    vram_size: usize,
    vram_used: usize,
}

impl AllocatorUsage {
    pub fn new(vram_size: usize) -> Self {
        Self {
            vram_size,
            vram_used: 0,
        }
    }

    /// Get the total size of managed VRAM
    pub fn vram_size(&self) -> usize {
        self.vram_size
    }

    /// Get the total number of bytes currently allocated
    pub fn vram_used(&self) -> usize {
        self.vram_used
    }
}

impl<B> ComputeAllocator<B>
where
    B: Backend,
{
    /// Size of the image memory region, in bytes
    const CHUNK_SIZE: u64 = 256 * 256 * 4; // bytes

    pub fn new(
        gpu: Arc<Mutex<GPU<B>>>,
        heap_pct: f32,
    ) -> Result<Self, hal::device::AllocationError> {
        let lock = gpu.lock().unwrap();

        let memory_type: hal::MemoryTypeId = lock
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

        let heap_size = lock.memory_properties.memory_heaps[memory_type.0];
        let allocator_size = (heap_size as f32 * heap_pct) as u64;
        let n_chunks = allocator_size / Self::CHUNK_SIZE;

        // Preallocate a block of memory for compute images in device local
        // memory. This serves as memory for all images used in compute other
        // than for image nodes, which are uploaded separately.
        let image_mem = unsafe { lock.device.allocate_memory(memory_type, allocator_size)? };

        Ok(Self {
            gpu: gpu.clone(),
            allocs: Cell::new(unsafe { AllocId::new_unchecked(1) }),
            image_mem: ManuallyDrop::new(image_mem),
            image_mem_chunks: (0..n_chunks)
                .map(|id| Chunk {
                    offset: Self::CHUNK_SIZE * id,
                    alloc: None,
                })
                .collect(),
            usage: AllocatorUsage::new(allocator_size as usize),
        })
    }

    /// Find the first set of chunks of contiguous free memory that fits the
    /// requested number of bytes
    pub fn find_free_memory(&self, bytes: u64) -> Option<(u64, Range<usize>)> {
        let request = bytes.max(Self::CHUNK_SIZE) / Self::CHUNK_SIZE;
        let mut offset = 0;
        let mut lower = 0;
        let mut upper;

        for (i, chunk) in self.image_mem_chunks.iter().enumerate() {
            if chunk.alloc.is_none() {
                upper = i + 1;
                if upper - lower == request as usize {
                    return Some((offset, lower..upper));
                }
            } else {
                offset = (i + 1) as u64 * Self::CHUNK_SIZE;
                lower = i + 1;
            }
        }

        None
    }

    /// Mark the given set of chunks as used. Assumes that the chunks were
    /// previously free!
    pub fn allocate_memory(&mut self, chunks: Range<usize>) -> AllocId {
        let alloc = self.allocs.get();
        for i in chunks {
            self.image_mem_chunks[i].alloc = Some(alloc);
            self.usage.vram_used += Self::CHUNK_SIZE as usize;
        }
        self.allocs.set(
            AllocId::new(alloc.get().wrapping_add(1))
                .unwrap_or(unsafe { AllocId::new_unchecked(1) }),
        );
        alloc
    }

    /// Mark the given set of chunks as free. Memory freed here should no longer
    /// be used!
    pub fn free_memory(&mut self, alloc: AllocId) {
        for mut chunk in self
            .image_mem_chunks
            .iter_mut()
            .filter(|c| c.alloc == Some(alloc))
        {
            chunk.alloc = None;
            self.usage.vram_used -= Self::CHUNK_SIZE as usize;
        }
    }

    /// Produce usage statistics for the allocator
    pub fn usage(&self) -> AllocatorUsage {
        self.usage
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
    /// The parent allocator, for deallocation on drop
    parent: Arc<Mutex<ComputeAllocator<B>>>,
    /// The pixel size of the image
    size: u32,
    /// The byte size of the image
    bytes: u64,
    /// Pixel width
    px_width: u8,
    /// The raw underlying image
    raw: ManuallyDrop<Arc<Mutex<B::Image>>>,
    /// The current layout of the underlying image
    layout: Cell<hal::image::Layout>,
    /// The current access flags of the underlying image
    access: Cell<hal::image::Access>,
    /// The corresponding image view
    view: ManuallyDrop<Option<B::ImageView>>,
    /// Corresponding allocator data if any
    alloc: Option<Alloc<B>>,
    /// The image format
    format: hal::format::Format,
    /// The type of the image, connected to the format
    image_type: lang::ImageType,
}

/// Equality on images is defined as pointer equality of the underlying raw
/// images. As such it won't catch multiple images using the same allocation!
impl<B> PartialEq for Image<B>
where
    B: Backend,
{
    fn eq(&self, other: &Image<B>) -> bool {
        Arc::ptr_eq(&self.raw, &other.raw)
    }
}

impl<B> Eq for Image<B> where B: Backend {}

/// Hash on images is defined on underlying pointers.
impl<B> std::hash::Hash for Image<B>
where
    B: Backend,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.raw).hash(state);
    }
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
        mips: bool,
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

        let mip_levels = if mips {
            size.next_power_of_two().trailing_zeros() as u8
        } else {
            1
        };

        // Create device image
        let image = unsafe {
            device.create_image(
                hal::image::Kind::D2(size, size, 1, 1),
                mip_levels,
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

        let bytes = unsafe { device.get_image_requirements(&image) }.size;

        Ok(Self {
            parent,
            size,
            bytes,
            px_width,
            raw: ManuallyDrop::new(Arc::new(Mutex::new(image))),
            layout: Cell::new(hal::image::Layout::Undefined),
            access: Cell::new(hal::image::Access::empty()),
            view: ManuallyDrop::new(None),
            alloc: None,
            format,
            image_type: ty,
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

        let mut parent_lock = self.parent.lock().unwrap();

        // Handle memory manager
        let (offset, chunks) = parent_lock
            .find_free_memory(self.bytes)
            .ok_or(AllocatorError::OutOfMemory)?;
        let alloc = parent_lock.allocate_memory(chunks);

        log::trace!(
            "Allocated memory for {}x{} image ({} bytes, id {})",
            self.size,
            self.size,
            self.bytes,
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
        match &self.alloc {
            None => return self.allocate_memory(),
            Some(a) => {
                log::trace!("Reusing existing allocation {}", a.id);
                Ok(())
            }
        }
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

    /// Get the image's image type.
    pub fn get_image_type(&self) -> lang::ImageType {
        self.image_type
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
        let mut alloc_lock = parent.lock().unwrap();

        let (offset, chunks) = alloc_lock
            .find_free_memory(bytes)
            .ok_or(AllocatorError::OutOfMemory)?;
        let mut buffer = unsafe { device.create_buffer(bytes, hal::buffer::Usage::STORAGE) }?;
        let alloc_id = alloc_lock.allocate_memory(chunks);

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
    pub fn barrier_to(&'_ self, access: hal::buffer::Access) -> hal::memory::Barrier<'_, B> {
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
