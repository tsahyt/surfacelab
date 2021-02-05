/// Basic structures for dealing with memory at a low level of abstraction.
/// Provides builders for buffers and images, backed by their own memory.
use gfx_hal as hal;
use gfx_hal::prelude::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BasicImageBuilderError {
    #[error("Error during creating image")]
    ImageCreation(#[from] hal::image::CreationError),
    #[error("Error during allocation of memory for image")]
    MemoryAllocation(#[from] hal::device::AllocationError),
    #[error("Failed to bind image to memory")]
    MemoryBinding(#[from] hal::device::BindError),
    #[error("Failed create image view")]
    ViewCreation(#[from] hal::image::ViewCreationError),
}

pub struct BasicImageBuilder<'a> {
    memory_types: &'a [hal::adapter::MemoryType],
    memory_type: hal::MemoryTypeId,
    kind: hal::image::Kind,
    mip_levels: u8,
    format: hal::format::Format,
    usage: hal::image::Usage,
    view_caps: hal::image::ViewCapabilities,
    range: hal::image::SubresourceRange,
}

impl<'a> BasicImageBuilder<'a> {
    pub fn new(memory_types: &'a [hal::adapter::MemoryType]) -> Self {
        Self {
            memory_types,
            memory_type: hal::MemoryTypeId(0),
            kind: hal::image::Kind::D2(1024, 1024, 1, 1),
            mip_levels: 1,
            format: hal::format::Format::Rgba8Srgb,
            usage: hal::image::Usage::empty(),
            view_caps: hal::image::ViewCapabilities::empty(),
            range: hal::image::SubresourceRange {
                aspects: hal::format::Aspects::COLOR,
                levels: 0..1,
                layers: 0..1,
            },
        }
    }

    pub fn size_2d(&mut self, width: u32, height: u32) -> &mut Self {
        self.kind = hal::image::Kind::D2(width, height, 1, 1);
        self
    }

    pub fn size_2d_msaa(&mut self, width: u32, height: u32, samples: u8) -> &mut Self {
        self.kind = hal::image::Kind::D2(width, height, 1, samples);
        self
    }

    pub fn size_cube(&mut self, side: u32) -> &mut Self {
        self.kind = hal::image::Kind::D2(side, side, 6, 1);
        self.view_caps = self.view_caps | hal::image::ViewCapabilities::KIND_CUBE;
        self.range.layers = 0..6;
        self
    }

    pub fn format(&mut self, format: hal::format::Format) -> &mut Self {
        self.format = format;
        self
    }

    pub fn usage(&mut self, usage: hal::image::Usage) -> &mut Self {
        self.usage = usage;
        self
    }

    pub fn mip_levels(&mut self, mip_levels: u8) -> &mut Self {
        self.mip_levels = mip_levels;
        self.range.levels = 0..mip_levels;
        self
    }

    pub fn memory_type(&mut self, memory_type: hal::memory::Properties) -> Option<&mut Self> {
        self.memory_type = self
            .memory_types
            .iter()
            .position(|mem_type| mem_type.properties.contains(memory_type))?
            .into();
        Some(self)
    }

    pub fn build<B: hal::Backend>(
        &self,
        device: &B::Device,
    ) -> Result<(B::Image, B::Memory, B::ImageView), BasicImageBuilderError> {
        let mut image = unsafe {
            device.create_image(
                self.kind,
                self.mip_levels,
                self.format,
                hal::image::Tiling::Linear,
                self.usage,
                self.view_caps,
            )
        }?;

        let requirements = unsafe { device.get_image_requirements(&image) };
        let memory = unsafe { device.allocate_memory(self.memory_type, requirements.size) }?;
        unsafe { device.bind_image_memory(&memory, 0, &mut image) }?;

        let view = unsafe {
            device.create_image_view(
                &image,
                match self.kind {
                    hal::image::Kind::D2(_, _, 1, _) => hal::image::ViewKind::D2,
                    hal::image::Kind::D2(_, _, 6, _) => hal::image::ViewKind::Cube,
                    _ => panic!("Invalid kind in BasicImageBuilder"),
                },
                self.format,
                hal::format::Swizzle::NO,
                self.range.clone(),
            )
        }?;

        Ok((image, memory, view))
    }
}

#[derive(Debug, Error)]
pub enum BasicBufferBuilderError {
    #[error("Error during buffer creation")]
    ImageCreation(#[from] hal::buffer::CreationError),
    #[error("Error during allocation of memory for image")]
    MemoryAllocation(#[from] hal::device::AllocationError),
    #[error("Failed to bind image to memory")]
    MemoryBinding(#[from] hal::device::BindError),
    #[error("Failed map memory region")]
    MemoryMapping(#[from] hal::device::MapError),
}

pub struct BasicBufferBuilder<'a> {
    memory_types: &'a [hal::adapter::MemoryType],
    memory_type: hal::MemoryTypeId,
    bytes: u64,
    usage: hal::buffer::Usage,
    data: Option<&'a [u8]>,
}

impl<'a> BasicBufferBuilder<'a> {
    pub fn new(memory_types: &'a [hal::adapter::MemoryType]) -> Self {
        Self {
            memory_types,
            memory_type: hal::MemoryTypeId(0),
            bytes: 1024,
            usage: hal::buffer::Usage::empty(),
            data: None,
        }
    }

    pub fn bytes(&mut self, bytes: u64) -> &mut Self {
        self.bytes = bytes;
        self
    }

    pub fn usage(&mut self, usage: hal::buffer::Usage) -> &mut Self {
        self.usage = usage;
        self
    }

    pub fn data(&mut self, data: &'a [u8]) -> &mut Self {
        self.data = Some(data);
        self
    }

    pub fn memory_type(&mut self, memory_type: hal::memory::Properties) -> Option<&mut Self> {
        self.memory_type = self
            .memory_types
            .iter()
            .position(|mem_type| mem_type.properties.contains(memory_type))?
            .into();
        Some(self)
    }

    pub fn build<B: hal::Backend>(
        &self,
        device: &B::Device,
    ) -> Result<(B::Buffer, B::Memory), BasicBufferBuilderError> {
        let mut buffer = unsafe { device.create_buffer(self.bytes, self.usage) }?;

        let mem = unsafe { device.allocate_memory(self.memory_type, self.bytes) }?;

        unsafe {
            device.bind_buffer_memory(&mem, 0, &mut buffer)?;
        };

        if let Some(data) = self.data {
            unsafe {
                let mapping = device.map_memory(
                    &mem,
                    hal::memory::Segment {
                        offset: 0,
                        size: Some(self.bytes),
                    },
                )?;
                let u8s: &[u8] = std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * std::mem::size_of::<image::Rgba<f32>>(),
                );
                std::ptr::copy_nonoverlapping(u8s.as_ptr(), mapping, self.bytes as usize);
                device.unmap_memory(&mem);
            }
        }

        Ok((buffer, mem))
    }
}
