#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{vec2, vec3};
use anyhow::anyhow;
use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};
use crate::{
    buffers::vertex_buffer::Vertex, context::Context, device::Device
};

pub const VERTICES: [Vertex; 8] = [
    Vertex::new(vec3(-0.5, -0.5, 0.0), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex::new(vec3(0.5, -0.5, 0.0), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex::new(vec3(0.5, 0.5, 0.0), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex::new(vec3(-0.5, 0.5, 0.0), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
    Vertex::new(vec3(-0.5, -0.5, -0.5), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex::new(vec3(0.5, -0.5, -0.5), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex::new(vec3(0.5, 0.5, -0.5), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex::new(vec3(-0.5, 0.5, -0.5), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
];

pub const INDICES: &[u16] = &[
    0, 1, 2, 2, 3, 0,
    4, 5, 6, 6, 7, 4,
];

pub trait TransferDst {}  // Implemented by VertexBuffer and IndexBuffer so they're treated as Transfer targets

#[derive(Debug)]
pub struct Buffer {
    handle: vk::Buffer,
    memory: vk::DeviceMemory,
    size: u64,  // Buffer size
}

impl Buffer {
    pub unsafe fn new(handle: vk::Buffer, memory: vk::DeviceMemory, size: u64) -> Result<Self> {
        Ok(Self { handle, memory, size })
    }

    pub unsafe fn create_buffer(device: &Device, size: u64, usage: vk::BufferUsageFlags) -> Result<vk::Buffer> {
        let queue_family_indices = &[device.queue_family_indices().graphics(), device.queue_family_indices().transfer()];

        let mut create_info = vk::BufferCreateInfo::builder()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .flags(vk::BufferCreateFlags::empty())
            .queue_family_indices(queue_family_indices);  // If SharingMode::CONCURRENT -> List the family indices that will be used
        create_info.queue_family_index_count = 2;

        let handle = device.logical().create_buffer(&create_info, None)?;

        Ok(handle)
    }

    pub unsafe fn create_memory(context: &Context, buffer: vk::Buffer, properties: vk::MemoryPropertyFlags) -> Result<vk::DeviceMemory> {
        let requirements = context.device.logical().get_buffer_memory_requirements(buffer);
        let memory_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(Buffer::get_memory_type_index(
                context,
                properties,
                requirements)?);

        let memory = context.device.logical().allocate_memory(&memory_info, None)?;

        Ok(memory)
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().free_memory(self.memory, None);
        self.memory = vk::DeviceMemory::null();
        info!("~ Memory");
        device.logical().destroy_buffer(self.handle, None);
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

    pub unsafe fn get_memory_type_index(context: &Context, properties: vk::MemoryPropertyFlags, requirements: vk::MemoryRequirements) -> Result<u32> {
        let memory_properties = context.device.memory_properties();
        (0..memory_properties.memory_type_count)
            .find(|i| {
                let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
                let memory_type = memory_properties.memory_types[*i as usize];
                suitable && memory_type.property_flags.contains(properties)
            })
            .ok_or_else(|| anyhow!("Failed to find suitable memory type."))
    }
}
