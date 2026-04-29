#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use anyhow::Result;

use crate::{
    device::Device,
    resources::buffers::{
        index_buffer::IndexBuffer,
        staging_buffer::StagingBuffer,
        uniform_buffer::UniformBuffer,
        vertex_buffer::{Vertex, VertexBuffer}
    }
};

#[derive(Debug)]
pub struct ResourceManager {
    staging_buffer: StagingBuffer,
    index_buffer: IndexBuffer,
    vertex_buffer: VertexBuffer,
    uniform_buffer: UniformBuffer,
}

impl ResourceManager {
    pub unsafe fn new(device: &Device) -> Result<Self> {
        Ok(Self {
            staging_buffer: StagingBuffer::new(device, 1024 * 1024 * 4)?, // 4Mb
            index_buffer: IndexBuffer::new(device, 1024)?,
            vertex_buffer: VertexBuffer::new(device, (size_of::<Vertex>() * 1024) as u64)?,
            uniform_buffer: UniformBuffer::new(device, 16)?,
        })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        self.staging_buffer.destroy(device);
        self.index_buffer.destroy(device);
        self.vertex_buffer.destroy(device);
        self.uniform_buffer.destroy(device);
    }

    pub fn staging_buffers(&self) -> &StagingBuffer {
        &self.staging_buffer
    }

    pub fn staging_buffers_mut(&mut self) -> &mut StagingBuffer {
        &mut self.staging_buffer
    }

    pub fn index_buffers(&self) -> &IndexBuffer {
        &self.index_buffer
    }

    pub fn index_buffers_mut(&mut self) -> &mut IndexBuffer {
        &mut self.index_buffer
    }

    pub fn vertex_buffers(&self) -> &VertexBuffer {
        &self.vertex_buffer
    }

    pub fn vertex_buffers_mut(&mut self) -> &mut VertexBuffer {
        &mut self.vertex_buffer
    }

    pub fn uniform_buffers(&self) -> &UniformBuffer {
        &self.uniform_buffer
    }

    pub fn uniform_buffers_mut(&mut self) -> &mut UniformBuffer {
        &mut self.uniform_buffer
    }
}