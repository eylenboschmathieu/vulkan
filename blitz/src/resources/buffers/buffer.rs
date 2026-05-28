#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use anyhow::{anyhow,Result};
use log::*;
use vulkanalia::vk::{self, *};
use crate::globals;

/// Marker for buffers that may be the destination of a `vkCmdCopyBuffer`.
/// Implemented by [`VertexBuffer`] and [`IndexBuffer`] so [`StagingBuffer`] copy
/// methods are generic without accepting arbitrary types.
pub trait TransferDst {}

/// Raw Vulkan buffer + device memory pair with no suballocator.
///
/// Higher-level buffer types (`VertexBuffer`, `IndexBuffer`, `StagingBuffer`,
/// `UniformBuffer`) all `Deref` into this to expose the handle and memory.
#[derive(Debug)]
pub struct Buffer {
    handle: vk::Buffer,
    memory: vk::DeviceMemory,
    size: u64,
}

impl Buffer {
    pub unsafe fn new(handle: vk::Buffer, memory: vk::DeviceMemory, size: u64) -> Result<Self> {
        Ok(Self { handle, memory, size })
    }

    /// Creates a `VkBuffer` shared between the graphics and transfer queue families.
    ///
    /// `CONCURRENT` sharing mode avoids explicit ownership transfers for buffer copies.
    /// Images are a different story and do require ownership transfers when the families differ.
    pub unsafe fn create_buffer(size: u64, usage: vk::BufferUsageFlags) -> Result<vk::Buffer> {
        let queue_family_indices = &[globals::device().queue_family_indices().graphics(), globals::device().queue_family_indices().transfer()];

        let mut create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .flags(vk::BufferCreateFlags::empty())
            .queue_family_indices(queue_family_indices);
        create_info.queue_family_index_count = 2;

        let handle = globals::device().logical().create_buffer(&create_info, None)?;

        Ok(handle)
    }

    /// Allocates device memory that satisfies `requirements` and has all bits in `properties`.
    pub unsafe fn create_memory(requirements: vk::MemoryRequirements, properties: vk::MemoryPropertyFlags) -> Result<vk::DeviceMemory> {
        let memory_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(Buffer::get_memory_type_index(
                properties,
                requirements)?
            );

        let memory = globals::device().logical().allocate_memory(&memory_info, None)?;

        Ok(memory)
    }

    pub unsafe fn destroy(&mut self) {
        globals::device().logical().free_memory(self.memory, None);
        self.memory = vk::DeviceMemory::null();
        info!("~ Memory");
        globals::device().logical().destroy_buffer(self.handle, None);
        self.handle = vk::Buffer::null();
        info!("~ Handle");
    }

    pub fn handle(&self) -> vk::Buffer {
        self.handle
    }

    pub fn memory(&self) -> vk::DeviceMemory {
        self.memory
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns the first memory type index whose type bits overlap `requirements` and whose
    /// flags contain all of `properties`.
    pub unsafe fn get_memory_type_index(properties: vk::MemoryPropertyFlags, requirements: vk::MemoryRequirements) -> Result<u32> {
        let memory_properties = globals::device().memory_properties();
        (0..memory_properties.memory_type_count)
            .find(|i| {
                let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
                let memory_type = memory_properties.memory_types[*i as usize];
                suitable && memory_type.property_flags.contains(properties)
            })
            .ok_or_else(|| anyhow!("Failed to find suitable memory type."))
    }
}
