#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use anyhow::{anyhow,Result};
use log::*;
use vulkanalia::vk::{self, *};
use crate::globals;

/// Marker for buffers that may be the destination of a `vkCmdCopyBuffer`.
/// Implemented by [`VertexBuffer`] and [`IndexBuffer`] so [`StagingBuffer`] copy
/// methods are generic without accepting arbitrary types.
pub trait TransferDst {}

/// Raw Vulkan buffer handle and size, with no associated `VkDeviceMemory`.
///
/// Each higher-level buffer type (`VertexBuffer`, `IndexBuffer`, `StagingBuffer`,
/// `UniformBuffer`) `Deref`s into this to expose the handle and size.
/// Memory ownership belongs to the concrete type, not to this struct.
///
/// The static helpers [`Buffer::create_buffer`], [`Buffer::create_memory`], and
/// [`Buffer::get_memory_type_index`] are construction utilities shared by all buffer types.
#[derive(Debug)]
pub struct Buffer {
    handle: vk::Buffer,
    size: u64,
}

impl Buffer {
    pub unsafe fn new(handle: vk::Buffer, size: u64) -> Result<Self> {
        Ok(Self { handle, size })
    }

    /// Creates a `VkBuffer` accessible by both the graphics and transfer queue families.
    ///
    /// Uses `EXCLUSIVE` sharing when both queues belong to the same family (no overhead),
    /// and `CONCURRENT` when they differ (avoids explicit ownership transfers for buffers).
    /// Images still require ownership transfers regardless — `CONCURRENT` only applies to buffers.
    pub unsafe fn create_buffer(size: u64, usage: vk::BufferUsageFlags) -> Result<vk::Buffer> {
        let qfi = globals::device().queue_family_indices();
        let sharing_mode = qfi.sharing_mode();

        let queue_family_indices = &[qfi.graphics(), qfi.transfer()];
        let mut create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(sharing_mode)
            .flags(vk::BufferCreateFlags::empty());

        if sharing_mode == vk::SharingMode::CONCURRENT {
            create_info = create_info.queue_family_indices(queue_family_indices);
            create_info.queue_family_index_count = 2;
        }

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
        globals::device().logical().destroy_buffer(self.handle, None);
        self.handle = vk::Buffer::null();
        info!("~ Handle");
    }

    pub fn handle(&self) -> vk::Buffer {
        self.handle
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
