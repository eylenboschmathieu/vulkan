#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::{Deref, DerefMut}, ptr::copy_nonoverlapping as memcpy
};

use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};
use crate::{
    buffers::buffer::{
        Buffer, TransferDst
    }, commands::CommandBuffer, context::Context, device::Device, image::Image
};

#[derive(Debug)]
pub struct StagingBuffer {
    pub buffer: Buffer,
}

impl StagingBuffer {
    pub unsafe fn new(context: &Context, size: u64) -> Result<Self> {
        // Buffer
        
        let handle = Buffer::create_buffer(
            &context.device,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC
        )?;
        info!("+ Handle");

        // Memory

        let memory = Buffer::create_memory(
            &context,
            handle,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
        )?;
        info!("+ Memory");

        // Binding

        context.device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        Ok(Self { buffer })
    }

    pub unsafe fn map(&self, device: &Device, size: u64, offset: u64) -> Result<*mut c_void> {
        Ok(device.logical().map_memory(self.memory(), offset, size, vk::MemoryMapFlags::empty())?)
    }

    pub unsafe fn unmap(&self, device: &Device) {
        device.logical().unmap_memory(self.memory());
    }

    /// Fetch the dst pointer from StagingBuffer.map()
    pub unsafe fn copy_to_staging<T>(&self, device: &Device, src: &[T], dst: *mut c_void) -> Result<()> {
        self.copy_to_staging_at(device, src, dst, 0)
    }

    /// Fetch the dst pointer from StagingBuffer.map()
    pub unsafe fn copy_to_staging_at<T>(&self, device: &Device, src: &[T], dst: *mut c_void, offset: usize) -> Result<()> {
        let size = (size_of::<T>() * src.len()) as usize;
        memcpy(src.as_ptr(), dst.add(offset).cast(), size as usize);
        Ok(())
    }

    pub unsafe fn copy_to_buffer<T>(&self, device: &Device, command_buffer: &CommandBuffer, dst_buffer: &T) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        self.copy_to_buffer_at(device, command_buffer, dst_buffer, 0)
    }

    pub unsafe fn copy_to_buffer_at<T>(&self, device: &Device, command_buffer: &CommandBuffer, dst_buffer: &T, offset: u64) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        let regions = vk::BufferCopy::builder().size(dst_buffer.size()).src_offset(offset);
        device.logical().cmd_copy_buffer(
            command_buffer.handle(),
            self.handle(),
            dst_buffer.handle(),
            &[regions]
        );
        
        Ok(())
    }

    pub unsafe fn copy_to_image(&self, device: &Device, command_buffer: &CommandBuffer, dst_image: &Image) -> Result<()> {
        self.copy_to_image_at(device, command_buffer, dst_image, 0)
    }

    pub unsafe fn copy_to_image_at(&self, device: &Device, command_buffer: &CommandBuffer, dst_image: &Image, offset: u64) -> Result<()> {
        let subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1);

        let regions = vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(subresource)
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D { width: dst_image.width(), height: dst_image.height(), depth: 1});

        device.logical().cmd_copy_buffer_to_image(
            command_buffer.handle(),
            self.handle(),
            dst_image.handle(),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
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