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
pub struct VertexBuffer {
    buffer: Buffer,
}

impl VertexBuffer {
    pub unsafe fn new(instance: &Instance, device: &Device, size: u32) -> Result<Self> {// Size

        // Buffer
        
        let handle = Buffer::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST
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

        Ok(Self { buffer })
    }

    pub unsafe fn bind(&self, device: &Device, command_buffer: &CommandBuffer) {
        device.logical().cmd_bind_vertex_buffers(command_buffer.handle(), 0, &[self.handle()], &[0]);
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