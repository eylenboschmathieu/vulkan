#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ops::{Deref, DerefMut},
};

use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};
use crate::{
    commands::CommandBuffer,
    buffers::buffer::{
        Buffer, TransferDst,
    }, device::Device, instance::Instance
};

#[derive(Debug)]
pub struct IndexBuffer {
    buffer: Buffer,
    count: u32,
}

impl IndexBuffer {
    pub unsafe fn new(instance: &Instance, device: &Device, indices: &[u16]) -> Result<Self> {
        let size = (size_of::<u16>() * indices.len()) as u64;

        // Buffer
        
        let handle = Buffer::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST
        )?;
        info!("+ Handle");

        // Memory

        let memory = Buffer::create_memory(instance,
            device,
            handle,
            vk::MemoryPropertyFlags::DEVICE_LOCAL
        )?;
        info!("+ Memory");

        // Binding

        device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        Ok(Self { buffer, count: indices.len() as u32 })
    }

    pub unsafe fn bind(&self, device: &Device, command_buffer: &CommandBuffer) {
        device.logical().cmd_bind_index_buffer(command_buffer.handle(), self.handle(), 0, vk::IndexType::UINT16);
    }

    pub fn count(&self) -> u32 {
        self.count
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