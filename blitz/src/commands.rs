#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::{Deref, Index};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};

use crate::{
    device::Device, instance::Instance, pipeline::Renderpass, queues::{Queue, QueueType}
};

#[derive(Debug)]
pub struct CommandPool {
    handle: vk::CommandPool,
    buffers: Vec<CommandBuffer>,
}

impl CommandPool {
    pub unsafe fn new(instance: &Instance, device: &Device, queue_family_index: u32, create_flags: Option<CommandPoolCreateFlags>) -> Result<Self> {
        let flags = match create_flags {
            Some(flags) => flags,
            None => vk::CommandPoolCreateFlags::empty(),
        };
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(flags)
            .queue_family_index(queue_family_index); // indices.graphics()

        let handle = device.logical().create_command_pool(&pool_info, None)?;

        info!("+ Handle");
        Ok(Self {
            handle,
            buffers: vec![],
        })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().destroy_command_pool(self.handle, None);
        self.handle = vk::CommandPool::null();
        info!("~ Handle")
    }

    /// Allocate and store command buffers in pool
    pub unsafe fn allocate_buffers(&mut self, device: &Device, size: usize) {
        let buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(size as u32);

        let buffers_ = device.logical().allocate_command_buffers(&buffer_info).expect("Failed to allocate command buffers");
        let mut buffers: Vec<CommandBuffer> = vec![];
        buffers_.iter().for_each(|handle| {
            buffers.push(CommandBuffer::new(handle.clone(), self.handle))
        });

        self.buffers = buffers;
    }

    /// Allocate and return command buffers
    pub unsafe fn fetch_buffers(&self, device: &Device, size: usize) -> Result<Vec<CommandBuffer>>{
        let buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(size as u32);

        let buffers_ = device.logical().allocate_command_buffers(&buffer_info).expect("Failed to allocate command buffers");
        let mut buffers: Vec<CommandBuffer> = vec![];
        buffers_.iter().for_each(|handle| {
            buffers.push(CommandBuffer::new(handle.clone(), self.handle))
        });

        Ok(buffers)
    }

    pub unsafe fn free_buffers(&mut self, device: &Device) {
        let handles: Vec<vk::CommandBuffer> = self.buffers
            .iter()
            .map(|buffer| buffer.handle)
            .collect();

        device.logical().free_command_buffers(self.handle, &handles);
    }
}

impl Index<usize> for CommandPool {
    type Output = CommandBuffer;

    // Index must be in range of [0, PoolSize-1]
    fn index(&self, index: usize) -> &Self::Output  {
        &self.buffers[index]
    }
}

impl<'a> IntoIterator for &'a CommandPool {
    type Item = &'a CommandBuffer;
    type IntoIter = std::slice::Iter<'a, CommandBuffer>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.buffers.iter()
    }
}

// Essentially a wrapper to collect command pools
#[derive(Debug)]
pub struct CommandManager {
    /*
    === Graphics Pool ===
        Creating 1 command buffer for each swapchain image for render operations
        Creating 1 command buffer for ownership transfers
    
    === Transfer Pool ===
        Creating 1 command buffer for transfer operations
    */
    graphics_pool: CommandPool,  // Used for rendering operations
    transfer_pool: CommandPool,  // Used for transfer operations
}

impl CommandManager {
    pub unsafe fn new(instance: &Instance, device: &Device) -> Result<Self> {
        let queue_family_indices = device.queue_family_indices();
        let create_flags = vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER;
        let graphics_pool = CommandPool::new(instance, &device, queue_family_indices.graphics(), Some(create_flags))?;
        let transfer_pool = CommandPool::new(instance, &device, queue_family_indices.transfer(), None)?;

        info!("+ CommandManager");

        Ok(Self {
            graphics_pool, transfer_pool
        })
    }

    /// Size should be the number of swapchain images
    pub unsafe fn allocate_graphics_buffers(&mut self, device: &Device, size: usize) -> Result<()> {
        self.graphics_pool.allocate_buffers(device, size);
        Ok(())
    }

    pub fn graphics(&self) -> &CommandPool {
        &self.graphics_pool
    }

    pub fn graphics_mut(&mut self) -> &mut CommandPool {
        &mut self.graphics_pool
    }

    pub fn transfer(&self) -> &CommandBuffer {
        &self.transfer_pool[0]
    }
    
    pub unsafe fn begin_one_time_submit(&self, device: &Device, buffer_type: vk::QueueFlags) -> Result<CommandBuffer> {
        let command_buffer = match buffer_type {
            vk::QueueFlags::GRAPHICS => self.graphics_pool.fetch_buffers(device, 1)?[0],
            vk::QueueFlags::TRANSFER => self.transfer_pool.fetch_buffers(device, 1)?[0],
            _ => return Err(anyhow!("Invalid buffer type for one time submit")),
        };

        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device.logical().begin_command_buffer(command_buffer.handle(), &info)?;
        Ok(command_buffer)
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        self.graphics_pool.destroy(device);
        self.transfer_pool.destroy(device);

        info!("~ CommandManager")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CommandBuffer {
    handle: vk::CommandBuffer,
    pool: vk::CommandPool,
}

impl CommandBuffer {
    pub unsafe fn new(handle: vk::CommandBuffer, pool: vk::CommandPool) -> Self {
        Self { handle, pool }
    }

    pub fn handle(&self) -> vk::CommandBuffer {
        self.handle
    }

    pub unsafe fn end_one_time_submit<T>(&self, device: &Device, queue: &T, wait_semaphore: Option<vk::Semaphore>) -> Result<()>
    where T: QueueType + Deref<Target = Queue> {
        device.logical().end_command_buffer(self.handle())?;
        queue.submit_transfer(device, &self, wait_semaphore)?;
        device.logical().queue_wait_idle(queue.handle())?;

        Ok(())
    }
    pub unsafe fn begin_recording(&self, device: &Device, extent: Extent2D, renderpass: &Renderpass, framebuffer: Framebuffer) -> Result<()> {
        let inheritance_info = vk::CommandBufferInheritanceInfo::builder();
        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::empty())
            .inheritance_info(&inheritance_info);

        device.logical().begin_command_buffer(self.handle, &info)?;
        renderpass.begin(device, &self, framebuffer, extent);

        Ok(())
    }

    pub unsafe fn end_recording(&self, device: &Device, renderpass: &Renderpass) -> Result<()> {
        renderpass.end(device, &self);
        device.logical().end_command_buffer(self.handle)?;

        Ok(())
    }
}