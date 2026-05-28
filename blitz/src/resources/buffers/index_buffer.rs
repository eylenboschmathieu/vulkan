#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::{Deref, DerefMut};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};
use crate::{
    commands::CommandBuffer, globals, resources::buffers::{
        buffer::{Buffer, TransferDst},
        freelist::{Allocation, Allocator},
    }
};

type IndexType = u16;

/// Encodes both which sub-buffer and which suballocation slot within it.
/// Returned by [`IndexBuffer::alloc`]; passed to every other method so
/// callers never need to track the two levels separately.
#[derive(Debug, Clone, Copy)]
pub struct IndexAllocId {
    pub buffer: usize,
    pub slot:   usize,
}

impl Default for IndexAllocId {
    fn default() -> Self {
        Self { buffer: usize::MAX, slot: usize::MAX }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IndexBufferData { pub allocation: Allocation, pub count: usize }

/// One `VkBuffer` handle with its own freelist suballocator.
/// Memory is owned by the parent [`IndexBuffer`] — this struct holds no `DeviceMemory`.
pub(crate) struct SubBuffer {
    buffer:      Buffer,
    allocator:   Allocator,
    buffer_list: Vec<IndexBufferData>,
    free_ids:    Vec<usize>,
}

impl SubBuffer {
    fn alloc(&mut self, count: usize) -> Option<usize> {
        if let Some(allocation) = self.allocator.alloc(count * size_of::<IndexType>()) {
            if self.free_ids.is_empty() {
                self.buffer_list.push(IndexBufferData { allocation, count });
                Some(self.buffer_list.len() - 1)
            } else {
                let slot = self.free_ids.pop().unwrap();
                self.buffer_list[slot] = IndexBufferData { allocation, count };
                Some(slot)
            }
        } else {
            None
        }
    }

    fn free(&mut self, slot: usize) {
        self.allocator.free(self.buffer_list[slot].allocation);
        self.free_ids.push(slot);
        self.buffer_list[slot] = IndexBufferData { allocation: Allocation { offset: 0, size: 0 }, count: 0 };
    }

    pub fn alloc_info(&self, slot: usize) -> Allocation {
        self.buffer_list[slot].allocation
    }

    pub fn count(&self, slot: usize) -> usize {
        self.buffer_list[slot].count
    }
}

impl TransferDst for SubBuffer {}

impl Deref for SubBuffer {
    type Target = Buffer;
    fn deref(&self) -> &Self::Target { &self.buffer }
}

impl DerefMut for SubBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.buffer }
}

impl std::fmt::Debug for SubBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubBuffer")
            .field("size", &self.buffer.size())
            .field("slots", &self.buffer_list.len())
            .finish()
    }
}

/// Collection of `DEVICE_LOCAL` index sub-buffers backed by a single shared `VkDeviceMemory`.
///
/// Each sub-buffer has its own `VkBuffer` and freelist suballocator but no memory of its own.
/// Allocations are addressed by [`IndexAllocId`], which encodes both the sub-buffer index
/// and the slot within it, so callers can never mix the two up.
#[derive(Debug)]
pub struct IndexBuffer {
    buffers: Vec<SubBuffer>,
    memory:  vk::DeviceMemory,
}

impl IndexBuffer {
    /// Creates one `VkBuffer` per entry in `sizes` (element counts), all bound to a single `VkDeviceMemory`.
    pub unsafe fn new(sizes: &[usize]) -> Result<Self> {
        assert!(!sizes.is_empty(), "IndexBuffer requires at least one sub-buffer");

        let mut handles: Vec<(vk::Buffer, vk::MemoryRequirements, usize)> = Vec::new();
        for &count in sizes {
            let size = size_of::<IndexType>() * count;
            let handle = Buffer::create_buffer(
                size as u64,
                vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
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
        let memory = Buffer::create_memory(combined_req, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        info!("+ Memory (shared, {} sub-buffers)", sizes.len());

        let mut buffers = Vec::with_capacity(sizes.len());
        for ((handle, req, size), offset) in handles.into_iter().zip(bind_offsets) {
            globals::device().logical().bind_buffer_memory(handle, memory, offset)?;
            let buffer    = Buffer::new(handle, size as u64)?;
            let allocator = Allocator::new(size, req.alignment as usize);
            buffers.push(SubBuffer { buffer, allocator, buffer_list: vec![], free_ids: vec![] });
        }

        Ok(Self { buffers, memory })
    }

    /// Suballocates `count` indices from sub-buffer `buffer` and returns an [`IndexAllocId`].
    pub fn alloc(&mut self, buffer: usize, count: usize) -> Result<IndexAllocId> {
        self.buffers[buffer]
            .alloc(count)
            .map(|slot| IndexAllocId { buffer, slot })
            .ok_or_else(|| anyhow!("Couldn't allocate index buffer {buffer}"))
    }

    pub fn free(&mut self, id: IndexAllocId) {
        self.buffers[id.buffer].free(id.slot);
    }

    pub fn alloc_info(&self, id: IndexAllocId) -> Allocation {
        self.buffers[id.buffer].alloc_info(id.slot)
    }

    pub fn count(&self, id: IndexAllocId) -> usize {
        self.buffers[id.buffer].count(id.slot)
    }

    /// Returns a reference to the sub-buffer that `id` belongs to.
    /// Used to satisfy the `TransferDst + Deref<Target = Buffer>` constraint
    /// required by `StagingBuffer` copy methods.
    pub(crate) fn sub_buffer(&self, id: IndexAllocId) -> &SubBuffer {
        &self.buffers[id.buffer]
    }

    /// Records `vkCmdBindIndexBuffer` at the byte offset of the allocation.
    pub unsafe fn bind(&self, command_buffer: &CommandBuffer, id: IndexAllocId) {
        let sub = &self.buffers[id.buffer];
        globals::device().logical().cmd_bind_index_buffer(
            command_buffer.handle(),
            sub.handle(),
            sub.alloc_info(id.slot).offset as u64,
            vk::IndexType::UINT16,
        );
    }

    /// Records `vkCmdDrawIndexed` for all indices in the allocation.
    pub unsafe fn draw(&self, command_buffer: &CommandBuffer, id: IndexAllocId, vertex_offset: i32) {
        let sub = &self.buffers[id.buffer];
        globals::device().logical().cmd_draw_indexed(
            command_buffer.handle(),
            sub.count(id.slot) as u32,
            1, 0, vertex_offset, 0,
        );
    }

    /// Records `vkCmdDrawIndexed` for a sub-range of the allocation.
    /// `first_index` and `count` are index element counts, not byte offsets.
    pub unsafe fn draw_range(&self, command_buffer: &CommandBuffer, id: IndexAllocId, first_index: u32, count: u32) {
        globals::device().logical().cmd_draw_indexed(
            command_buffer.handle(),
            count, 1, first_index, 0, 0,
        );
    }

    pub unsafe fn destroy(&mut self) {
        for sub in &mut self.buffers {
            sub.buffer.destroy();  // Destroys VkBuffer handle only; SubBuffer holds no DeviceMemory
        }
        globals::device().logical().free_memory(self.memory, None);
        self.memory = vk::DeviceMemory::null();
        info!("~ Memory (shared)");
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}
