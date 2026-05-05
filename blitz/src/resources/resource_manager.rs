#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use anyhow::Result;

use crate::{
    pipeline::descriptors::DescriptorPool,
    resources::{
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
    pub(crate) descriptors: DescriptorPool,
    pub(crate) textures: Textures,
    pub(crate) materials: Materials,
}

impl ResourceManager {
    pub(crate) unsafe fn new() -> Result<Self> {
        Ok(Self {
            staging_buffer: StagingBuffer::new(1024 * 1024 * 4)?, // 4Mb
            index_buffer: IndexBuffer::new(1024)?, // 2Kb
            vertex_buffer: VertexBuffer::new(1024 * 1024 * 4)?, // 4Mb
            uniform_buffer: UniformBuffer::new(16)?,
            descriptors: DescriptorPool::new(4)?,
            textures: Textures::new()?,
            materials: Materials::new()?,
        })
    }

    pub(crate) unsafe fn destroy(&mut self) {
        self.materials.destroy();
        self.descriptors.destroy();
        self.staging_buffer.destroy();
        self.index_buffer.destroy();
        self.vertex_buffer.destroy();
        self.uniform_buffer.destroy();
        self.textures.destroy();
    }
}