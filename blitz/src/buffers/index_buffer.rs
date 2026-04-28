#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::{Deref, DerefMut};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};
use crate::{
    buffers::{
        buffer::{
            Buffer, TransferDst,
        },
        freelist::{Allocator, Allocation},
    },
    commands::CommandBuffer,
    context::Context,
    device::Device,
};

type IndexType = u16;
pub type IndexBufferId = usize;

#[derive(Clone, Copy, Debug)]
pub struct IndexBufferData { pub allocation: Allocation, pub count: usize }

#[derive(Debug)]
pub struct IndexBuffer {
    buffer: Buffer,
    allocator: Allocator,
    buffer_list: Vec<IndexBufferData>,
    free_list: Vec<IndexBufferId>,
}

// Need to incorporate this into a resource manager at some point
impl IndexBuffer {
    pub unsafe fn new(context: &Context, max_indice_count: usize) -> Result<Self> {
        let size = size_of::<IndexType>() * max_indice_count;  // We're going with the assumption indices ur u16

        // Buffer
        
        let handle = Buffer::create_buffer(
            &context.device,
            size as u64,
            vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST
        )?;
        info!("+ Handle");

        // Memory

        let requirements = context.device.logical().get_buffer_memory_requirements(handle);

        let memory = Buffer::create_memory(
            context,
            requirements,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        info!("+ Memory");

        // Binding

        context.device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size as u64)?;

        let allocator = Allocator::new(size, requirements.alignment as usize);

        Ok(Self { buffer, allocator, buffer_list: vec![], free_list: vec![] })
    }

    pub fn alloc(&mut self, count: usize) -> Result<IndexBufferId> {
        if let Some(allocation) = self.allocator.alloc(count * size_of::<IndexType>()) {
            if self.free_list.is_empty() {
                self.buffer_list.push(IndexBufferData { allocation, count });
                return Ok(self.buffer_list.len() - 1);
            } else {
                let id = self.free_list.pop().unwrap();
                self.buffer_list[id] = IndexBufferData { allocation, count };
                return Ok(id);
            }
        };

        Err(anyhow!("Couldn't allocate index buffer"))
    }

    pub fn free(&mut self, id: IndexBufferId) {
        let data = &self.buffer_list[id];
        self.allocator.free(data.allocation);
        self.free_list.push(id);
        self.buffer_list[id] = IndexBufferData { allocation: Allocation { offset: 0, size: 0 }, count: 0 }
    }

    pub unsafe fn bind(&self, device: &Device, command_buffer: &CommandBuffer, id: usize) {
        device.logical().cmd_bind_index_buffer(
            command_buffer.handle(),
            self.handle(),
            self.buffer_list[id].allocation.offset as u64,
            vk::IndexType::UINT16);
    }

    pub unsafe fn draw(&self, device: &Device, command_buffer: &CommandBuffer, id: IndexBufferId, vertex_offset: Option<i32>) {
        let vertex_offset = match vertex_offset {
            Some(offset) => offset,
            None => 0,
        };

        device.logical().cmd_draw_indexed(
            command_buffer.handle(),
            self.buffer_list[id].count as u32,
            1,
            self.buffer_list[id].allocation.offset as u32,
            vertex_offset,
            0);
    }

    pub fn count(&self, id: IndexBufferId) -> usize {
        self.buffer_list[id].count
    }

    pub fn alloc_info(&self, id: IndexBufferId) -> Allocation {
        self.buffer_list[id].allocation
    }
}

impl TransferDst for IndexBuffer {}

impl Deref for IndexBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for IndexBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}