#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    mem::size_of,
};

use cgmath::{vec2, vec3};
use anyhow::anyhow;
use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};
use crate::{
    Destroyable, device::Device, instance::Instance
};

type Vec2 = cgmath::Vector2<f32>;
type Vec3 = cgmath::Vector3<f32>;

pub static VERTICES: [Vertex; 4] = [
    Vertex::new(vec2(-0.5, -0.5), vec3(1.0, 0.0, 0.0)),
    Vertex::new(vec2(0.5, -0.5), vec3(0.0, 1.0, 0.0)),
    Vertex::new(vec2(0.5, 0.5), vec3(0.0, 0.0, 1.0)),
    Vertex::new(vec2(-0.5, 0.5), vec3(1.0, 1.0, 1.0)),
];
pub static INDICES: &[u16] = &[0, 1, 2, 2, 3, 0];

pub trait TransferDst {}

#[repr(C)]
#[derive(Debug)]
pub struct Vertex {
    pos: Vec2,
    color: Vec3,
}

impl Vertex {
    const fn new (pos: Vec2, color: Vec3) -> Self {
        Self { pos, color }
    }

    pub fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(size_of::<Self>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    pub fn attribute_description() -> [vk::VertexInputAttributeDescription; 2] {
        let pos = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        let color = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(size_of::<Vec2>() as u32)
            .build();

        [pos, color]
    }
}

#[derive(Debug)]
pub struct Buffer {
    handle: vk::Buffer,
    memory: vk::DeviceMemory,
    size: u32,  // Vertex count - Used by cmd_draw
}

impl Buffer {
    pub unsafe fn new(handle: vk::Buffer, memory: vk::DeviceMemory, size: u32) -> Result<Self> {
        Ok(Self { handle, memory, size })
    }

    pub unsafe fn create_buffer(device: &Device, size: u32, usage: vk::BufferUsageFlags) -> Result<vk::Buffer> {
        let queue_family_indices = &[device.queue_family_indices().graphics(), device.queue_family_indices().transfer()];

        let mut create_info = vk::BufferCreateInfo::builder()
            .size(size as u64)
            .usage(usage)
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .flags(vk::BufferCreateFlags::empty())
            .queue_family_indices(queue_family_indices);  // If SharingMode::CONCURRENT -> List the family indices that will be used
        create_info.queue_family_index_count = 2;

        let handle = device.logical().create_buffer(&create_info, None)?;

        Ok(handle)
    }

    pub unsafe fn create_memory(instance: &Instance, device: &Device, buffer: vk::Buffer, properties: vk::MemoryPropertyFlags) -> Result<vk::DeviceMemory> {
        let requirements = device.logical().get_buffer_memory_requirements(buffer);
        let memory_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(Buffer::get_memory_type_index(
                instance,
                device,
                properties,
                requirements)?);

        let memory = device.logical().allocate_memory(&memory_info, None)?;

        Ok(memory)
    }

    pub fn handle(&self) -> vk::Buffer {
        self.handle
    }

    pub fn memory(&self) -> vk::DeviceMemory {
        self.memory
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    unsafe fn get_memory_type_index(instance: &Instance, device: &Device, properties: vk::MemoryPropertyFlags, requirements: vk::MemoryRequirements) -> Result<u32> {
        let memory = instance.handle().get_physical_device_memory_properties(device.physical());
        (0..memory.memory_type_count)
            .find(|i| {
                let suitable = (requirements.memory_type_bits & (1 << i)) != 0;
                let memory_type = memory.memory_types[*i as usize];
                suitable && memory_type.property_flags.contains(properties)
            })
            .ok_or_else(|| anyhow!("Failed to find suitable memory type."))
    }
}

impl Destroyable for Buffer {
    unsafe fn destroy(&mut self, device: &Device) {
        device.logical().free_memory(self.memory, None);
        self.memory = vk::DeviceMemory::null();
        info!("~ Memory");
        device.logical().destroy_buffer(self.handle, None);
        self.handle = vk::Buffer::null();
        info!("~ Handle");
    }
}