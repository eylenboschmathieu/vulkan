#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use anyhow::Result;

use crate::{
    device::Device, pipeline::descriptors::DescriptorSetLayouts, resources::{
        buffers::{
            index_buffer::IndexBuffer,
            staging_buffer::StagingBuffer,
            uniform_buffer::UniformBuffer,
            vertex_buffer::VertexBuffer,
        },
        image::Textures, material::Materials,
    }
};

#[derive(Debug)]
pub struct ResourceManager {
    pub(crate) staging_buffer: StagingBuffer,
    pub(crate) index_buffer: IndexBuffer,
    pub(crate) vertex_buffer: VertexBuffer,
    pub(crate) uniform_buffer: UniformBuffer,
    pub(crate) descriptor_set_layouts: DescriptorSetLayouts,
    pub(crate) textures: Textures,
    pub(crate) materials: Materials,
}

impl ResourceManager {
    pub(crate) unsafe fn new(device: &Device) -> Result<Self> {
        Ok(Self {
            staging_buffer: StagingBuffer::new(device, 1024 * 1024 * 4)?, // 4Mb
            index_buffer: IndexBuffer::new(device, 1024)?, // 2Kb
            vertex_buffer: VertexBuffer::new(device, 1024 * 1024 * 4)?, // 4Mb
            uniform_buffer: UniformBuffer::new(device, 16)?,
            descriptor_set_layouts: DescriptorSetLayouts::new()?,
            // descriptors: Descriptors::new(),
            textures: Textures::new()?,
            materials: Materials::new()?,
        })
    }

    pub(crate) unsafe fn destroy(&mut self, device: &Device) {
        self.materials.destroy(device);
        self.descriptor_set_layouts.destroy(device);
        self.staging_buffer.destroy(device);
        self.index_buffer.destroy(device);
        self.vertex_buffer.destroy(device);
        self.uniform_buffer.destroy(device);
        self.textures.destroy(device);
    }
}