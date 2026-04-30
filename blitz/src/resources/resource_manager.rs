#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use anyhow::Result;

use crate::{
    device::Device,
    resources::{
        buffers::{
            index_buffer::IndexBuffer,
            staging_buffer::StagingBuffer,
            uniform_buffer::UniformBuffer,
            vertex_buffer::{Vertex, VertexBuffer},
        },
        image::Textures,
    }
};

#[derive(Debug)]
pub struct ResourceManager {
    pub(crate) staging_buffer: StagingBuffer,
    pub(crate) index_buffer: IndexBuffer,
    pub(crate) vertex_buffer: VertexBuffer,
    pub(crate) uniform_buffer: UniformBuffer,
    pub(crate) textures: Textures,
}

impl ResourceManager {
    pub(crate) unsafe fn new(device: &Device) -> Result<Self> {
        Ok(Self {
            staging_buffer: StagingBuffer::new(device, 1024 * 1024 * 4)?, // 4Mb
            index_buffer: IndexBuffer::new(device, 1024)?,
            vertex_buffer: VertexBuffer::new(device, (size_of::<Vertex>() * 1024) as u64)?,
            uniform_buffer: UniformBuffer::new(device, 16)?,
            textures: Textures::new()?,
        })
    }

    pub(crate) unsafe fn destroy(&mut self, device: &Device) {
        self.staging_buffer.destroy(device);
        self.index_buffer.destroy(device);
        self.vertex_buffer.destroy(device);
        self.uniform_buffer.destroy(device);
        self.textures.destroy(device);
    }
}