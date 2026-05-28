#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::{Deref, DerefMut};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, DeviceV1_0, Handle};

use crate::{
    globals,
    resources::buffers::{buffer::{Buffer, TransferDst}, freelist::{Allocation, Allocator}},
    commands::CommandBuffer,
};

/// Encodes both which sub-buffer and which suballocation slot within it.
/// Returned by [`VertexBuffer::alloc`]; passed to every other method so
/// callers never need to track the two levels separately.
#[derive(Debug, Clone, Copy)]
pub struct VertexAllocId {
    pub buffer: usize,  // index into VertexBuffer's sub-buffer list
    pub slot:   usize,  // suballocation slot within that sub-buffer
}

impl Default for VertexAllocId {
    fn default() -> Self {
        Self { buffer: usize::MAX, slot: usize::MAX }
    }
}

/// One `VkBuffer` handle with its own freelist suballocator.
/// Memory is owned by the parent [`VertexBuffer`] and shared across all sub-buffers.
pub(crate) struct SubBuffer {
    buffer:     Buffer,           // VkBuffer handle only — Buffer holds no DeviceMemory; memory is owned by VertexBuffer
    allocator:  Allocator,        // Manages byte ranges within this VkBuffer
    alloc_list: Vec<Allocation>,  // Maps slot → byte offset and size
    free_ids:   Vec<usize>,       // Reusable alloc_list slots from previous frees
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
        self.allocator.free(self.alloc_list[slot]);
        self.free_ids.push(slot);
        self.alloc_list[slot] = Allocation { offset: 0, size: 0 };
    }

    pub fn alloc_info(&self, slot: usize) -> Allocation {
        self.alloc_list[slot]
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
            .field("slots", &self.alloc_list.len())
            .finish()
    }
}

/// Collection of `DEVICE_LOCAL` vertex sub-buffers backed by a single shared `VkDeviceMemory`.
///
/// Each sub-buffer has its own `VkBuffer` and freelist suballocator but no memory of its own.
/// Allocations are addressed by [`VertexAllocId`], which encodes both the sub-buffer index
/// and the slot within it, so callers can never mix the two up.
#[derive(Debug)]
pub struct VertexBuffer {
    buffers: Vec<SubBuffer>,
    memory:  vk::DeviceMemory,  // Shared across all sub-buffers
}

impl VertexBuffer {
    /// Creates one `VkBuffer` per entry in `sizes`, all bound to a single `VkDeviceMemory`.
    pub unsafe fn new(sizes: &[usize]) -> Result<Self> {
        assert!(!sizes.is_empty(), "VertexBuffer requires at least one sub-buffer");

        // Create all VkBuffer handles and query their memory requirements.
        let mut handles: Vec<(vk::Buffer, vk::MemoryRequirements, usize)> = Vec::new();
        for &size in sizes {
            let handle = Buffer::create_buffer(
                size as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            )?;
            info!("+ Handle");
            let req = globals::device().logical().get_buffer_memory_requirements(handle);
            handles.push((handle, req, size));
        }

        // Compute the aligned binding offset for each buffer within the shared allocation.
        let mut bind_offsets = Vec::with_capacity(sizes.len());
        let mut cursor = 0u64;
        for (_, req, _) in &handles {
            cursor = align_up(cursor, req.alignment);
            bind_offsets.push(cursor);
            cursor += req.size;
        }
        let total_size = cursor;

        // Intersect memory type bits across all buffers; find a compatible DEVICE_LOCAL type.
        let combined_type_bits = handles.iter().fold(!0u32, |acc, (_, r, _)| acc & r.memory_type_bits);
        let combined_req = vk::MemoryRequirements {
            size: total_size,
            alignment: handles.iter().map(|(_, r, _)| r.alignment).max().unwrap_or(1),
            memory_type_bits: combined_type_bits,
        };
        let memory = Buffer::create_memory(combined_req, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        info!("+ Memory (shared, {} sub-buffers)", sizes.len());

        // Bind each VkBuffer to its offset within the shared memory.
        // SubBuffer's Buffer holds no DeviceMemory — the parent VertexBuffer owns it.
        let mut buffers = Vec::with_capacity(sizes.len());
        for ((handle, req, size), offset) in handles.into_iter().zip(bind_offsets) {
            globals::device().logical().bind_buffer_memory(handle, memory, offset)?;
            let buffer = Buffer::new(handle, size as u64)?;
            let allocator = Allocator::new(size, req.alignment as usize);
            buffers.push(SubBuffer { buffer, allocator, alloc_list: vec![], free_ids: vec![] });
        }

        Ok(Self { buffers, memory })
    }

    /// Suballocates `size` bytes from sub-buffer `buffer` and returns a [`VertexAllocId`].
    pub fn alloc(&mut self, buffer: usize, size: usize) -> Result<VertexAllocId> {
        self.buffers[buffer]
            .alloc(size)
            .map(|slot| VertexAllocId { buffer, slot })
            .ok_or_else(|| anyhow!("Couldn't allocate vertex buffer {buffer}"))
    }

    pub fn free(&mut self, id: VertexAllocId) {
        self.buffers[id.buffer].free(id.slot);
    }

    pub fn alloc_info(&self, id: VertexAllocId) -> Allocation {
        self.buffers[id.buffer].alloc_info(id.slot)
    }

    /// Records `vkCmdBindVertexBuffers` for the sub-buffer and offset encoded in `id`.
    pub unsafe fn bind(&self, command_buffer: &CommandBuffer, id: VertexAllocId) {
        let sub = &self.buffers[id.buffer];
        globals::device().logical().cmd_bind_vertex_buffers(
            command_buffer.handle(),
            0,
            &[sub.handle()],
            &[sub.alloc_info(id.slot).offset as u64],
        );
    }

    /// Returns a reference to the sub-buffer that `id` belongs to.
    /// Used by [`Container`] to satisfy the `TransferDst + Deref<Target = Buffer>`
    /// constraint required by `StagingBuffer` copy methods.
    pub(crate) fn sub_buffer(&self, id: VertexAllocId) -> &SubBuffer {
        &self.buffers[id.buffer]
    }

    pub unsafe fn destroy(&mut self) {
        for sub in &mut self.buffers {
            sub.buffer.destroy();  // Destroys VkBuffer handle only; Buffer holds no DeviceMemory
        }
        globals::device().logical().free_memory(self.memory, None);
        self.memory = vk::DeviceMemory::null();
        info!("~ Memory (shared)");
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}
