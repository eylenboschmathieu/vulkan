#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::{Deref, DerefMut}, ptr::{copy_nonoverlapping as memcpy}
};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};
use crate::{
    globals,
    resources::{
        image::Image,
        buffers::{
            buffer::{
                Buffer, TransferDst
            }, freelist::{Allocation, Allocator}
        },
    },
    commands::CommandBuffer,
};

pub type StagingBufferId = usize;

/// `HOST_VISIBLE | HOST_COHERENT` upload buffer, permanently mapped for its lifetime.
///
/// Suballocates regions by `StagingBufferId`.  CPU writes go directly through the
/// mapped pointer; no explicit flush is needed because `HOST_COHERENT` guarantees
/// visibility to the device without it.
///
/// **Offset asymmetry**: `copy_to_staging_at` and `copy_to_image_at` both add
/// `alloc_list[id].offset` internally, so callers pass an allocation-relative offset.
/// `copy_to_buffer_sized` does **not** — it
/// takes an absolute byte offset into the `VkBuffer`; callers must add
/// `alloc_info(id).offset` themselves when needed.
#[derive(Debug)]
pub struct StagingBuffer {
    buffer: Buffer,
    allocator: Allocator,
    alloc_list: Vec<Allocation>,
    free_list: Vec<StagingBufferId>,
    mapped_ptr: *mut c_void,
}

impl StagingBuffer {
    pub unsafe fn new(size: vk::DeviceSize) -> Result<Self> {
        // Buffer

        let handle = Buffer::create_buffer(
            size,
            vk::BufferUsageFlags::TRANSFER_SRC
        )?;
        info!("+ Handle");

        // Memory

        let requirements = globals::device().logical().get_buffer_memory_requirements(handle);

        let memory = Buffer::create_memory(
            requirements,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
        )?;
        info!("+ Memory");

        // Binding

        globals::device().logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        let allocator = Allocator::new(size as usize, requirements.alignment as usize);

        let mapped_ptr = globals::device().logical().map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;

        Ok(Self { buffer, allocator, alloc_list: vec![], free_list: vec![], mapped_ptr })
    }

    pub unsafe fn destroy(&mut self) {
        globals::device().logical().unmap_memory(self.memory());
        self.buffer.destroy();
    }

    /// Copies data into the staging buffer
    pub unsafe fn copy_to_staging<T>(&self, id: StagingBufferId, data: &[T]) -> Result<()> {
        self.copy_to_staging_at(id, data, 0)
    }

    /// Copies data into the staging buffer at offset
    pub unsafe fn copy_to_staging_at<T>(&self, id: StagingBufferId, data: &[T], offset: usize) -> Result<()> {
        if cfg!(debug_assertions) {
            let write_end = offset + data.len() * size_of::<T>();
            let alloc_size = self.alloc_list[id].size;
            assert!(write_end <= alloc_size, "copy_to_staging_at: write of {write_end} bytes exceeds allocation size of {alloc_size}");
        }
        let offset = self.alloc_list[id].offset + offset;
        memcpy(data.as_ptr(), self.mapped_ptr.add(offset).cast(), data.len());
        Ok(())
    }

    /// Records a buffer copy using the allocation's own offset and size as the destination region.
    pub unsafe fn copy_to_buffer<T>(&self, command_buffer: &CommandBuffer, dst_buffer: &T, allocation: Allocation, src_offset: u64) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        self.copy_to_buffer_sized(command_buffer, dst_buffer, allocation.offset as u64, src_offset, allocation.size as u64)
    }

    /// Records a buffer copy with fully explicit offsets and size.
    ///
    /// `src_offset` is an **absolute** byte offset into this staging `VkBuffer` —
    /// it is used as-is.  Callers that staged data via `copy_to_staging_at` must
    /// add `alloc_info(id).offset` themselves:
    /// `alloc_info(id).offset as u64 + local_offset`.
    pub unsafe fn copy_to_buffer_sized<T>(&self, command_buffer: &CommandBuffer, dst_buffer: &T, dst_offset: u64, src_offset: u64, size: u64) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        if cfg!(debug_assertions) {
            assert!(src_offset + size <= self.size(), "copy_to_buffer_sized: src range [{src_offset}, {}) exceeds staging buffer size {}", src_offset + size, self.size());
            assert!(dst_offset + size <= dst_buffer.size(), "copy_to_buffer_sized: dst range [{dst_offset}, {}) exceeds destination buffer size {}", dst_offset + size, dst_buffer.size());
        }
        let regions = vk::BufferCopy::builder()
            .size(size)
            .src_offset(src_offset)
            .dst_offset(dst_offset);

        globals::device().logical().cmd_copy_buffer(
            command_buffer.handle(),
            self.handle(),
            dst_buffer.handle(),
            &[regions]
        );

        Ok(())
    }

    /// Records a buffer-to-image copy from the start of the allocation, targeting layer 0.
    pub unsafe fn copy_to_image(&self, command_buffer: &CommandBuffer, id: StagingBufferId, dst_image: &Image) -> Result<()> {
        self.copy_to_image_layer(command_buffer, id, dst_image, 0, 0)
    }

    /// Records a buffer-to-image copy from `alloc_start + offset`, targeting layer 0.
    pub unsafe fn copy_to_image_at(&self, command_buffer: &CommandBuffer, id: StagingBufferId, dst_image: &Image, offset: u64) -> Result<()> {
        self.copy_to_image_layer(command_buffer, id, dst_image, offset, 0)
    }

    /// Records a buffer-to-image copy from `alloc_start + offset` into `layer` of `dst_image`.
    /// Used for texture arrays where each layer's pixels are staged contiguously.
    pub unsafe fn copy_to_image_layer(&self, command_buffer: &CommandBuffer, id: StagingBufferId, dst_image: &Image, offset: u64, layer: u32) -> Result<()> {
        if cfg!(debug_assertions) {
            let alloc = self.alloc_list[id];
            let abs_offset = alloc.offset as u64 + offset;
            let alloc_end = (alloc.offset + alloc.size) as u64;
            assert!(abs_offset < alloc_end, "copy_to_image_layer: offset {abs_offset} is outside allocation [{}, {alloc_end})", alloc.offset);
        }
        let subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(layer)
            .layer_count(1);

        let regions = vk::BufferImageCopy::builder()
            .buffer_offset(self.alloc_list[id].offset as u64 + offset)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(subresource)
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D { width: dst_image.width(), height: dst_image.height(), depth: 1 });

        globals::device().logical().cmd_copy_buffer_to_image(
            command_buffer.handle(),
            self.handle(),
            dst_image.handle(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[regions]
        );

        Ok(())
    }
    
    /// Suballocates `size` bytes and returns a `StagingBufferId`.
    pub fn alloc(&mut self, size: usize) -> Result<StagingBufferId> {
        if let Some(allocation) = self.allocator.alloc(size) {
            if self.free_list.is_empty() {
                self.alloc_list.push(allocation);
                return Ok(self.alloc_list.len() - 1);
            } else {
                let id = self.free_list.pop().unwrap();
                self.alloc_list[id] = allocation;
                return Ok(id);
            }
        };

        Err(anyhow!("Couldn't allocate staging buffer"))
    }

    pub fn free(&mut self, id: StagingBufferId) {
        let allocation = self.alloc_list[id];
        self.allocator.free(allocation);
        self.free_list.push(id);
        self.alloc_list[id] = allocation;
    }

    pub fn alloc_info(&self, id: StagingBufferId) -> Allocation {
        self.alloc_list[id]
    }
}

impl Deref for StagingBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for StagingBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}