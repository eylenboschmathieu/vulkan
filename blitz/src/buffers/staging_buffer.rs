#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ops::{Deref, DerefMut},
    ptr::copy_nonoverlapping as memcpy,
};

use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};
use crate::{buffers::{
        buffer::{
            Buffer, TransferDst
        },
    }, commands::CommandBuffer, device::Device, instance::Instance
};

#[derive(Debug)]
pub struct StagingBuffer {
    pub buffer: Buffer,
}

impl StagingBuffer {
    pub unsafe fn new(instance: &Instance, device: &Device, size: u32) -> Result<Self> {
        // Buffer
        
        let handle = Buffer::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC
        )?;
        info!("+ Handle");

        // Memory

        let memory = Buffer::create_memory(
            instance,
            device,
            handle,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
        )?;
        info!("+ Memory");

        // Binding

        device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        Ok(Self { buffer })
    }
    pub unsafe fn copy_to_staging<T>(&self, device: &Device, data: &[T]) -> Result<()> {
        self.copy_to_staging_at(device, data, 0)
    }

    pub unsafe fn copy_to_staging_at<T>(&self, device: &Device, data: &[T], offset: u64) -> Result<()> {
        let size = (size_of::<T>() * data.len()) as u64;
        let memory = device.logical().map_memory(self.memory(), offset, size, vk::MemoryMapFlags::empty())?;
        memcpy(data.as_ptr(), memory.cast(), size as usize);
        device.logical().unmap_memory(self.memory());
        Ok(())
    }

    pub unsafe fn copy_to_buffer<T>(&self, device: &Device, command_buffer: &CommandBuffer, dst_buffer: &T) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        self.copy_to_buffer_at(device, command_buffer, dst_buffer, 0)
    }

    pub unsafe fn copy_to_buffer_at<T>(&self, device: &Device, command_buffer: &CommandBuffer, dst_buffer: &T, offset: u64) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        let regions = vk::BufferCopy::builder().size(dst_buffer.size() as u64).src_offset(offset);
        device.logical().cmd_copy_buffer(
            command_buffer.handle(),
            self.handle(),
            dst_buffer.handle(),
            &[regions]
        );
        
        Ok(())
    }
}

impl Deref for StagingBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for StagingBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}