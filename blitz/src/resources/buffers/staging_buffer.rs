#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::{Deref, DerefMut}, ptr::{copy_nonoverlapping as memcpy}
};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};
use crate::{
    resources::{
        buffers::{
            buffer::{
                Buffer, TransferDst
            }, freelist::{Allocation, Allocator}
        },
    },
    commands::CommandBuffer,
    device::Device,
    image::Image
};

pub type StagingBufferId = usize;

#[derive(Debug)]
pub struct StagingBuffer {
    buffer: Buffer,
    allocator: Allocator,
    alloc_list: Vec<Allocation>,
    free_list: Vec<StagingBufferId>,
    mapped_ptr: *mut c_void,
}

impl StagingBuffer {
    pub unsafe fn new(device: &Device, size: vk::DeviceSize) -> Result<Self> {
        // Buffer
        
        let handle = Buffer::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC
        )?;
        info!("+ Handle");

        // Memory

        let requirements = device.logical().get_buffer_memory_requirements(handle);

        let memory = Buffer::create_memory(
            device,
            requirements,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
        )?;
        info!("+ Memory");

        // Binding

        device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        let allocator = Allocator::new(size as usize, requirements.alignment as usize);

        let mapped_ptr = device.logical().map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;

        Ok(Self { buffer, allocator, alloc_list: vec![], free_list: vec![], mapped_ptr })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().unmap_memory(self.memory());
        self.buffer.destroy(device);
    }

    /// Copies data into the staging buffer
    pub unsafe fn copy_to_staging<T>(&self, device: &Device, id: StagingBufferId, data: &[T]) -> Result<()> {
        self.copy_to_staging_at(device, id, data, 0)
    }

    /// Copies data into the staging buffer at offset
    pub unsafe fn copy_to_staging_at<T>(&self, device: &Device, id: StagingBufferId, data: &[T], offset: usize) -> Result<()> {
        let offset = offset + self.alloc_list[id].offset;
        let size = (size_of::<T>() * data.len()) as usize;
        memcpy(data.as_ptr(), self.mapped_ptr.add(offset).cast(), size as usize);
        Ok(())
    }

    // dst_buffer should be the buffer, and the alloc info
    pub unsafe fn copy_to_buffer<T>(&self, device: &Device, command_buffer: &CommandBuffer, dst_buffer: &T, allocation: Allocation, src_offset: u64) -> Result<()>
    where T: TransferDst + Deref<Target = Buffer> {
        let regions = vk::BufferCopy::builder()
            .size(allocation.size as u64)
            .src_offset(src_offset)
            .dst_offset(allocation.offset as u64); 

        device.logical().cmd_copy_buffer(
            command_buffer.handle(),
            self.handle(),
            dst_buffer.handle(),
            &[regions]
        );
        
        Ok(())
    }

    pub unsafe fn copy_to_image(&self, device: &Device, command_buffer: &CommandBuffer, id: StagingBufferId, dst_image: &Image) -> Result<()> {
        self.copy_to_image_at(device, command_buffer, id, dst_image, 0)
    }

    pub unsafe fn copy_to_image_at(&self, device: &Device, command_buffer: &CommandBuffer, id: StagingBufferId, dst_image: &Image, offset: u64) -> Result<()> {
        let subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1);

        let regions = vk::BufferImageCopy::builder()
            .buffer_offset(self.alloc_list[id].offset as u64)
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

    pub fn alloc(&mut self, size: usize) -> Result<StagingBufferId> {
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

        Err(anyhow!("Couldn't allocate staging buffer"))
    }

    pub fn free(&mut self, id: StagingBufferId) {
        let allocation = self.alloc_list[id];
        self.allocator.free(allocation);
        self.free_list.push(id);
        self.alloc_list[id] = allocation;
    }

    pub fn alloc_info(&self, id: StagingBufferId) -> Allocation {
        self.alloc_list[id]
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