#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::Deref, ptr::{copy_nonoverlapping as memcpy}
};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};
use crate::{
    globals,
    resources::{
        image::Image,
        buffers::{
            buffer::{Buffer, TransferDst},
            freelist::{Allocation, Allocator},
        },
    },
    commands::CommandBuffer,
};

/// Encodes both which sub-buffer and which suballocation slot within it.
/// Returned by [`StagingBuffer::alloc`]; passed to every copy method.
#[derive(Debug, Clone, Copy)]
pub struct StagingAllocId {
    pub buffer: usize,
    pub slot:   usize,
}

impl Default for StagingAllocId {
    fn default() -> Self {
        Self { buffer: usize::MAX, slot: usize::MAX }
    }
}

/// One `VkBuffer` handle with its own freelist suballocator.
/// Memory is owned by the parent [`StagingBuffer`] — this struct holds no `DeviceMemory`.
struct SubBuffer {
    buffer:      Buffer,
    allocator:   Allocator,
    alloc_list:  Vec<Allocation>,
    free_ids:    Vec<usize>,
    bind_offset: usize,  // Byte offset of this VkBuffer within the shared VkDeviceMemory, used for mapped-pointer writes
}

impl SubBuffer {
    fn alloc(&mut self, size: usize) -> Option<usize> {
        if let Some(allocation) = self.allocator.alloc(size) {
            if self.free_ids.is_empty() {
                self.alloc_list.push(allocation);
                Some(self.alloc_list.len() - 1)
            } else {
                let slot = self.free_ids.pop().unwrap();
                self.alloc_list[slot] = allocation;
                Some(slot)
            }
        } else {
            None
        }
    }

    fn free(&mut self, slot: usize) {
        let allocation = self.alloc_list[slot];
        self.allocator.free(allocation);
        self.free_ids.push(slot);
        self.alloc_list[slot] = allocation;
    }

    fn alloc_info(&self, slot: usize) -> Allocation {
        self.alloc_list[slot]
    }
}

impl Deref for SubBuffer {
    type Target = Buffer;
    fn deref(&self) -> &Self::Target { &self.buffer }
}

impl std::fmt::Debug for SubBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubBuffer")
            .field("size", &self.buffer.size())
            .field("slots", &self.alloc_list.len())
            .finish()
    }
}

/// Collection of `HOST_VISIBLE | HOST_COHERENT` upload sub-buffers backed by a single shared
/// `VkDeviceMemory`, permanently mapped for the buffer's lifetime.
///
/// Suballocates regions by [`StagingAllocId`].  CPU writes go directly through the mapped
/// pointer; no explicit flush is needed because `HOST_COHERENT` guarantees visibility to the
/// device without it.
///
/// All `copy_to_*` methods take an allocation-relative `offset`; the absolute address is
/// computed internally using the sub-buffer's `bind_offset` and the allocation's own offset.
#[derive(Debug)]
pub struct StagingBuffer {
    subs:       Vec<SubBuffer>,
    memory:     vk::DeviceMemory,
    mapped_ptr: *mut c_void,
}

impl StagingBuffer {
    /// Creates one `VkBuffer` per entry in `sizes` (bytes), all bound to a single `VkDeviceMemory`.
    pub unsafe fn new(sizes: &[usize]) -> Result<Self> {
        assert!(!sizes.is_empty(), "StagingBuffer requires at least one sub-buffer");

        let mut handles: Vec<(vk::Buffer, vk::MemoryRequirements, usize)> = Vec::new();
        for &size in sizes {
            let handle = Buffer::create_buffer(
                size as u64,
                vk::BufferUsageFlags::TRANSFER_SRC,
            )?;
            info!("+ Handle");
            let req = globals::device().logical().get_buffer_memory_requirements(handle);
            handles.push((handle, req, size));
        }

        let mut bind_offsets = Vec::with_capacity(sizes.len());
        let mut cursor = 0u64;
        for (_, req, _) in &handles {
            cursor = align_up(cursor, req.alignment);
            bind_offsets.push(cursor);
            cursor += req.size;
        }
        let total_size = cursor;

        let combined_type_bits = handles.iter().fold(!0u32, |acc, (_, r, _)| acc & r.memory_type_bits);
        let combined_req = vk::MemoryRequirements {
            size: total_size,
            alignment: handles.iter().map(|(_, r, _)| r.alignment).max().unwrap_or(1),
            memory_type_bits: combined_type_bits,
        };
        let memory = Buffer::create_memory(
            combined_req,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
        )?;
        info!("+ Memory (shared, {} sub-buffers)", sizes.len());

        let mapped_ptr = globals::device().logical().map_memory(
            memory, 0, total_size, vk::MemoryMapFlags::empty(),
        )?;

        let mut subs = Vec::with_capacity(sizes.len());
        for ((handle, req, size), offset) in handles.into_iter().zip(bind_offsets) {
            globals::device().logical().bind_buffer_memory(handle, memory, offset)?;
            let buffer      = Buffer::new(handle, size as u64)?;
            let allocator   = Allocator::new(size, req.alignment as usize);
            let bind_offset = offset as usize;
            subs.push(SubBuffer { buffer, allocator, alloc_list: vec![], free_ids: vec![], bind_offset });
        }

        Ok(Self { subs, memory, mapped_ptr })
    }

    pub unsafe fn destroy(&mut self) {
        globals::device().logical().unmap_memory(self.memory);
        for sub in &mut self.subs {
            sub.buffer.destroy();  // Destroys VkBuffer handle only; SubBuffer holds no DeviceMemory
        }
        globals::device().logical().free_memory(self.memory, None);
        self.memory = vk::DeviceMemory::null();
        info!("~ Memory (shared)");
    }

    /// Suballocates `size` bytes from sub-buffer `buffer` and returns a [`StagingAllocId`].
    pub fn alloc(&mut self, buffer: usize, size: usize) -> Result<StagingAllocId> {
        self.subs[buffer]
            .alloc(size)
            .map(|slot| StagingAllocId { buffer, slot })
            .ok_or_else(|| anyhow!("Couldn't allocate staging buffer {buffer}"))
    }

    pub fn free(&mut self, id: StagingAllocId) {
        self.subs[id.buffer].free(id.slot);
    }

    pub fn alloc_info(&self, id: StagingAllocId) -> Allocation {
        self.subs[id.buffer].alloc_info(id.slot)
    }

    /// Copies data into the staging allocation starting at offset 0.
    pub unsafe fn copy_to_staging<T>(&self, id: StagingAllocId, data: &[T]) -> Result<()> {
        self.copy_to_staging_at(id, data, 0)
    }

    /// Copies data into the staging allocation at an allocation-relative `offset`.
    pub unsafe fn copy_to_staging_at<T>(&self, id: StagingAllocId, data: &[T], offset: usize) -> Result<()> {
        let sub  = &self.subs[id.buffer];
        let alloc = sub.alloc_info(id.slot);
        if cfg!(debug_assertions) {
            let write_end = offset + data.len() * size_of::<T>();
            assert!(write_end <= alloc.size, "copy_to_staging_at: write of {write_end} bytes exceeds allocation size of {}", alloc.size);
        }
        let abs_offset = sub.bind_offset + alloc.offset + offset;
        memcpy(data.as_ptr(), self.mapped_ptr.add(abs_offset).cast(), data.len());
        Ok(())
    }

    /// Records a buffer copy from `local_src_offset` within `staging_id`'s allocation into
    /// the destination allocation. `local_src_offset` is allocation-relative.
    pub unsafe fn copy_to_buffer<T>(&self, command_buffer: &CommandBuffer, staging_id: StagingAllocId, dst_buffer: &T, allocation: Allocation, local_src_offset: u64) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        self.copy_to_buffer_sized(command_buffer, staging_id, dst_buffer, allocation.offset as u64, local_src_offset, allocation.size as u64)
    }

    /// Records a buffer copy with fully explicit offsets and size.
    /// Both `local_src_offset` and `dst_offset` are VkBuffer-relative.
    /// `local_src_offset` is relative to the start of `staging_id`'s allocation.
    pub unsafe fn copy_to_buffer_sized<T>(&self, command_buffer: &CommandBuffer, staging_id: StagingAllocId, dst_buffer: &T, dst_offset: u64, local_src_offset: u64, size: u64) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        let sub        = &self.subs[staging_id.buffer];
        let src_offset = sub.alloc_info(staging_id.slot).offset as u64 + local_src_offset;
        if cfg!(debug_assertions) {
            assert!(src_offset + size <= sub.size(), "copy_to_buffer_sized: src range [{src_offset}, {}) exceeds sub-buffer size {}", src_offset + size, sub.size());
            assert!(dst_offset + size <= dst_buffer.size(), "copy_to_buffer_sized: dst range [{dst_offset}, {}) exceeds destination buffer size {}", dst_offset + size, dst_buffer.size());
        }
        let regions = vk::BufferCopy::builder()
            .size(size)
            .src_offset(src_offset)
            .dst_offset(dst_offset);

        globals::device().logical().cmd_copy_buffer(
            command_buffer.handle(),
            sub.handle(),
            dst_buffer.handle(),
            &[regions],
        );

        Ok(())
    }

    /// Records a buffer-to-image copy from the start of the allocation, targeting layer 0.
    pub unsafe fn copy_to_image(&self, command_buffer: &CommandBuffer, id: StagingAllocId, dst_image: &Image) -> Result<()> {
        self.copy_to_image_layer(command_buffer, id, dst_image, 0, 0)
    }

    /// Records a buffer-to-image copy from `alloc_start + offset`, targeting layer 0.
    pub unsafe fn copy_to_image_at(&self, command_buffer: &CommandBuffer, id: StagingAllocId, dst_image: &Image, offset: u64) -> Result<()> {
        self.copy_to_image_layer(command_buffer, id, dst_image, offset, 0)
    }

    /// Records a buffer-to-image copy from `alloc_start + offset` into `layer` of `dst_image`.
    /// Used for texture arrays where each layer's pixels are staged contiguously.
    pub unsafe fn copy_to_image_layer(&self, command_buffer: &CommandBuffer, id: StagingAllocId, dst_image: &Image, offset: u64, layer: u32) -> Result<()> {
        let sub   = &self.subs[id.buffer];
        let alloc = sub.alloc_info(id.slot);
        if cfg!(debug_assertions) {
            let abs_offset = alloc.offset as u64 + offset;
            let alloc_end  = (alloc.offset + alloc.size) as u64;
            assert!(abs_offset < alloc_end, "copy_to_image_layer: offset {abs_offset} is outside allocation [{}, {alloc_end})", alloc.offset);
        }
        let subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(layer)
            .layer_count(1);

        let regions = vk::BufferImageCopy::builder()
            .buffer_offset(alloc.offset as u64 + offset)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(subresource)
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D { width: dst_image.width(), height: dst_image.height(), depth: 1 });

        globals::device().logical().cmd_copy_buffer_to_image(
            command_buffer.handle(),
            sub.handle(),
            dst_image.handle(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[regions],
        );

        Ok(())
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}
