#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::{Deref, DerefMut}, ptr::copy_nonoverlapping as memcpy,
};
use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};

use crate::{
    globals,
    resources::buffers::{buffer::Buffer, freelist::{Allocation, Allocator}},
};

type Mat4 = cgmath::Matrix4<f32>;
type Vec4 = cgmath::Vector4<f32>;

/// Camera matrices uploaded to descriptor set 0, binding 0 every frame.
/// `model` is typically identity; view and proj are computed by the camera.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CameraUbo {
    pub model: Mat4,
    pub view:  Mat4,
    pub proj:  Mat4,
}

/// Scene lighting data uploaded to descriptor set 2, binding 0 every frame.
/// `sun_dir` is a world-space direction vector pointing *toward* the sun; w is unused.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LightingUbo {
    pub sun_dir: Vec4,
}

/// Encodes both which sub-buffer and which suballocation slot within it.
/// Returned by [`UniformBuffer::alloc`]; passed to every other method.
#[derive(Debug, Clone, Copy)]
pub struct UniformAllocId {
    pub buffer: usize,
    pub slot:   usize,
}

impl Default for UniformAllocId {
    fn default() -> Self {
        Self { buffer: usize::MAX, slot: usize::MAX }
    }
}

/// One `VkBuffer` handle with its own freelist suballocator.
/// Memory is owned by the parent [`UniformBuffer`] — this struct holds no `DeviceMemory`.
pub(crate) struct SubBuffer {
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
        self.allocator.free(self.alloc_list[slot]);
        self.free_ids.push(slot);
        self.alloc_list[slot] = Allocation { offset: 0, size: 0 };
    }

    pub fn alloc_info(&self, slot: usize) -> Allocation {
        self.alloc_list[slot]
    }
}

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

/// Collection of `HOST_VISIBLE | HOST_COHERENT` uniform sub-buffers backed by a single shared
/// `VkDeviceMemory`, permanently mapped for the buffer's lifetime.
///
/// Uses the same freelist allocator as the vertex/index buffers so camera and lighting slots
/// can have different sizes and be freed independently.  Writes go via `memcpy` straight to the
/// mapped pointer; no flush needed because `HOST_COHERENT` guarantees device visibility.
#[derive(Debug)]
pub struct UniformBuffer {
    subs:       Vec<SubBuffer>,
    memory:     vk::DeviceMemory,
    mapped_ptr: *mut c_void,
}

impl UniformBuffer {
    /// Creates one `VkBuffer` per entry in `sizes` (element counts of `CameraUbo`-sized slots),
    /// all bound to a single `VkDeviceMemory`.
    pub unsafe fn new(sizes: &[usize]) -> Result<Self> {
        assert!(!sizes.is_empty(), "UniformBuffer requires at least one sub-buffer");

        let mut handles: Vec<(vk::Buffer, vk::MemoryRequirements, usize)> = Vec::new();
        for &count in sizes {
            let size = size_of::<CameraUbo>() * count;
            let handle = Buffer::create_buffer(size as u64, vk::BufferUsageFlags::UNIFORM_BUFFER)?;
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

    /// Suballocates `size` bytes from sub-buffer `buffer` and returns a [`UniformAllocId`].
    pub fn alloc(&mut self, buffer: usize, size: usize) -> Result<UniformAllocId> {
        self.subs[buffer]
            .alloc(size)
            .map(|slot| UniformAllocId { buffer, slot })
            .ok_or_else(|| anyhow!("Couldn't allocate uniform buffer {buffer}"))
    }

    pub fn free(&mut self, id: UniformAllocId) {
        self.subs[id.buffer].free(id.slot);
    }

    pub fn alloc_info(&self, id: UniformAllocId) -> Allocation {
        self.subs[id.buffer].alloc_info(id.slot)
    }

    /// Returns a reference to the sub-buffer that `id` belongs to.
    /// Used by [`DescriptorPool`] to obtain the correct `VkBuffer` handle for descriptor writes.
    pub(crate) fn sub_buffer(&self, id: UniformAllocId) -> &SubBuffer {
        &self.subs[id.buffer]
    }

    /// Writes `data` into the allocation via the permanently-mapped pointer.
    /// No flush needed — `HOST_COHERENT` guarantees device visibility.
    pub unsafe fn update<T: Copy>(&self, id: UniformAllocId, data: T) -> Result<()> {
        let sub    = &self.subs[id.buffer];
        let offset = sub.bind_offset + sub.alloc_info(id.slot).offset;
        memcpy(&data, self.mapped_ptr.add(offset).cast(), 1);
        Ok(())
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}
