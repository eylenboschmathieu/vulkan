#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::{Deref, Index};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};

use crate::{
    globals,
    instance::Instance, pipeline::renderpass::Renderpass, queues::{Queue, QueueType}
};

#[derive(Debug)]
pub(crate) struct CommandPool {
    handle: vk::CommandPool,
    buffers: Vec<CommandBuffer>,
}

impl CommandPool {
    pub unsafe fn new(instance: &Instance, queue_family_index: u32, create_flags: Option<CommandPoolCreateFlags>) -> Result<Self> {
        let flags = match create_flags {
            Some(flags) => flags,
            None => vk::CommandPoolCreateFlags::empty(),
        };
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(flags)
            .queue_family_index(queue_family_index); // indices.graphics()

        let handle = globals::device().logical().create_command_pool(&pool_info, None)?;

        info!("+ Handle");
        Ok(Self {
            handle,
            buffers: vec![],
        })
    }

    pub unsafe fn destroy(&mut self) {
        globals::device().logical().destroy_command_pool(self.handle, None);
        self.handle = vk::CommandPool::null();
        info!("~ Handle")
    }

    /// Allocate and store command buffers in pool
    pub unsafe fn allocate_buffers(&mut self, size: usize) {
        let buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(size as u32);

        let buffers_ = globals::device().logical().allocate_command_buffers(&buffer_info).expect("Failed to allocate command buffers");
        let mut buffers: Vec<CommandBuffer> = vec![];
        buffers_.iter().for_each(|handle| {
            buffers.push(CommandBuffer::new(handle.clone(), self.handle))
        });

        self.buffers = buffers;
    }

    /// Allocate and return command buffers
    pub unsafe fn fetch_buffers(&self, size: usize) -> Result<Vec<CommandBuffer>>{
        let buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(size as u32);

        let buffers_ = globals::device().logical().allocate_command_buffers(&buffer_info).expect("Failed to allocate command buffers");
        let mut buffers: Vec<CommandBuffer> = vec![];
        buffers_.iter().for_each(|handle| {
            buffers.push(CommandBuffer::new(handle.clone(), self.handle))
        });

        Ok(buffers)
    }

    pub unsafe fn free_buffers(&mut self) {
        let handles: Vec<vk::CommandBuffer> = self.buffers
            .iter()
            .map(|buffer| buffer.handle)
            .collect();

        globals::device().logical().free_command_buffers(self.handle, &handles);
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
pub(crate) struct CommandManager {
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
    pub unsafe fn new(instance: &Instance) -> Result<Self> {
        let queue_family_indices = globals::device().queue_family_indices();
        let create_flags = vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER;
        let graphics_pool = CommandPool::new(instance, queue_family_indices.graphics(), Some(create_flags))?;
        let transfer_pool = CommandPool::new(instance, queue_family_indices.transfer(), None)?;

        info!("+ CommandManager");

        Ok(Self {
            graphics_pool, transfer_pool
        })
    }

    /// Size should be the number of swapchain images
    pub unsafe fn allocate_graphics_buffers(&mut self, size: usize) -> Result<()> {
        self.graphics_pool.allocate_buffers(size);
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
    
    pub unsafe fn begin_one_time_submit(&self, buffer_type: vk::QueueFlags) -> Result<CommandBuffer> {
        let command_buffer = match buffer_type {
            vk::QueueFlags::GRAPHICS => self.graphics_pool.fetch_buffers(1)?[0],
            vk::QueueFlags::TRANSFER => self.transfer_pool.fetch_buffers(1)?[0],
            _ => return Err(anyhow!("Invalid buffer type for one time submit")),
        };

        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        globals::device().logical().begin_command_buffer(command_buffer.handle(), &info)?;
        Ok(command_buffer)
    }

    pub unsafe fn destroy(&mut self) {
        self.graphics_pool.destroy();
        self.transfer_pool.destroy();

        info!("~ CommandManager")
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommandBuffer {
    handle: vk::CommandBuffer,
}

impl Default for CommandBuffer {
    fn default() -> Self {
        Self { handle: vk::CommandBuffer::null() }
    }
}

impl CommandBuffer {
    pub unsafe fn new(handle: vk::CommandBuffer, pool: vk::CommandPool) -> Self {
        Self { handle }
    }

    pub fn handle(&self) -> vk::CommandBuffer {
        self.handle
    }

    pub unsafe fn end_one_time_submit<T>(&self, queue: &T, wait_semaphore: Option<vk::Semaphore>) -> Result<()>
    where T: QueueType + Deref<Target = Queue> {
        globals::device().logical().end_command_buffer(self.handle())?;
        queue.submit_transfer(&self, wait_semaphore)?;
        globals::device().logical().queue_wait_idle(queue.handle())?;

        Ok(())
    }

    pub unsafe fn begin_recording(&self, extent: Extent2D, renderpass: &Renderpass, framebuffer: Framebuffer, sky_color: [f32; 4]) -> Result<()> {
        let inheritance_info = vk::CommandBufferInheritanceInfo::builder();
        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::empty())
            .inheritance_info(&inheritance_info);

        globals::device().logical().begin_command_buffer(self.handle, &info)?;
        renderpass.begin(&self, framebuffer, extent, sky_color);

        Ok(())
    }

    pub unsafe fn end_recording(&self, renderpass: &Renderpass) -> Result<()> {
        renderpass.end(&self);
        globals::device().logical().end_command_buffer(self.handle)?;

        Ok(())
    }
}