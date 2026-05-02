#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ops::{Deref, DerefMut},
};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};

use crate::{
    resources::buffers::{buffer::{
        Buffer, TransferDst,
    }, freelist::{Allocation, Allocator},}, commands::CommandBuffer, device::Device,
};

pub type VertexBufferId = usize;

#[derive(Debug)]
pub struct VertexBuffer {
    buffer: Buffer,
    allocator: Allocator,
    alloc_list: Vec<Allocation>,
    free_list: Vec<VertexBufferId>,
}

impl VertexBuffer {
    pub unsafe fn new(device: &Device, size: vk::DeviceSize) -> Result<Self> {

        // Buffer
        
        let handle = Buffer::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST
        )?;
        info!("+ Handle");

        // Memory

        let requirements = device.logical().get_buffer_memory_requirements(handle);

        let memory = Buffer::create_memory(
            device,
            requirements,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        info!("+ Memory");

        // Binding

        device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        let allocator = Allocator::new(size as usize, requirements.alignment as usize);

        Ok(Self { buffer, allocator, alloc_list: vec![], free_list: vec![] })
    }

    pub unsafe fn bind(&self, device: &Device, command_buffer: &CommandBuffer, id: VertexBufferId) {
        device.logical().cmd_bind_vertex_buffers(
            command_buffer.handle(),
            0,
            &[self.handle()],
            &[self.alloc_list[id].offset as u64]
        );
    }

    pub fn alloc(&mut self, size: usize) -> Result<VertexBufferId> {
        if let Some(allocation) = self.allocator.alloc(size) {
            if self.free_list.is_empty() {
                self.alloc_list.push(allocation);
                return Ok(self.alloc_list.len() - 1);
            } else {
                let id = self.free_list.pop().unwrap();
                self.alloc_list[id] = allocation;
                return Ok(id);
            }
        };

        Err(anyhow!("Couldn't allocate vertex buffer"))
    }

    pub fn free(&mut self, id: VertexBufferId) {
        let allocation = self.alloc_list[id];
        self.allocator.free(allocation);
        self.free_list.push(id);
        self.alloc_list[id] = Allocation { offset: 0, size: 0 }
    }

    pub fn alloc_info(&self, id: VertexBufferId) -> Allocation {
        self.alloc_list[id]
    }
}

impl TransferDst for VertexBuffer {}

impl Deref for VertexBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for VertexBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}