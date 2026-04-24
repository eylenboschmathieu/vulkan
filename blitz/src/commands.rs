#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::Index;

use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};

use crate::{
    device::Device, instance::{Instance, QueueFamilyIndices}, renderpass::Renderpass
};

#[derive(Debug)]
pub struct InnerCommandPool {
    handle: vk::CommandPool,
    buffers: Vec<CommandBuffer>,
}

impl InnerCommandPool {
    pub unsafe fn new(instance: &Instance, device: &Device, queue_family_index: u32) -> Result<Self> {
        // let indices = QueueFamilyIndices::get(instance, device.physical())?;
        let pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::empty())
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

    pub unsafe fn allocate_buffers(&mut self, device: &Device, size: usize) {
        let buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(self.handle)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(size as u32);

        let buffers_ = device.logical().allocate_command_buffers(&buffer_info).expect("Failed to allocate command buffers");
        let mut buffers: Vec<CommandBuffer> = vec![];
        buffers_.iter().for_each(|handle| {
            buffers.push(CommandBuffer::new(handle.clone()))
        });

        self.buffers = buffers;
    }

    pub unsafe fn free_buffers(&mut self, device: &Device) {
        let handles: Vec<vk::CommandBuffer> = self.buffers
            .iter()
            .map(|buffer| buffer.handle)
            .collect();

        device.logical().free_command_buffers(self.handle, &handles);
    }
}

impl Index<usize> for InnerCommandPool {
    type Output = CommandBuffer;

    // Index must be in range of [0, PoolSize-1]
    fn index(&self, index: usize) -> &Self::Output  {
        &self.buffers[index]
    }
}

impl<'a> IntoIterator for &'a InnerCommandPool {
    type Item = &'a CommandBuffer;
    type IntoIter = std::slice::Iter<'a, CommandBuffer>;
    
    fn into_iter(self) -> Self::IntoIter {
        self.buffers.iter()
    }
}

// Essentially a wrapper to collect command pools
#[derive(Debug)]
pub struct CommandPool {
    device: Device,
    queue_family_indices: QueueFamilyIndices,
    pub graphics: Option<InnerCommandPool>,
    pub transfer: Option<InnerCommandPool>,
}

impl CommandPool {
    /// Size should be the same as the amount of framebuffers
    pub unsafe fn new(instance: &Instance, device: &Device) -> Result<Self> {
        let queue_family_indices = QueueFamilyIndices::get(instance, device.physical())?;
        Ok(Self {
            device: device.clone(),
            queue_family_indices,
            graphics: None,
            transfer: None,
        })
    }

    pub unsafe fn create_render_pool(&mut self, instance: &Instance, size: usize) -> Result<()> {
        let mut pool = InnerCommandPool::new(instance, &self.device, self.queue_family_indices.graphics())?;
        pool.allocate_buffers(&self.device, size);
        self.graphics = Some(pool);
        Ok(())
    }

    pub unsafe fn create_transfer_pool(&mut self, instance: &Instance) -> Result<()> {
        let mut pool = InnerCommandPool::new(instance, &self.device, self.queue_family_indices.transfer())?;
        pool.allocate_buffers(&self.device, 1);
        self.transfer = Some(pool);
        Ok(())
    }

    pub unsafe fn destroy(&mut self) {
        if let Some(pool) = &mut self.graphics {
            pool.destroy(&self.device);
        }
        if let Some(pool) = &mut self.transfer {
            pool.destroy(&self.device);
        }
    }
}

#[derive(Debug)]
pub struct CommandBuffer {
    handle: vk::CommandBuffer
}

impl CommandBuffer {
    pub unsafe fn new(handle: vk::CommandBuffer) -> Self {
        Self { handle }
    }

    pub fn handle(&self) -> vk::CommandBuffer {
        self.handle
    }

    pub unsafe fn begin_one_time_submit(&self, device: &Device) -> Result<()> {
        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device.logical().begin_command_buffer(self.handle, &info)?;
        Ok(())
    }

    pub unsafe fn end_one_time_submit(&self, device: &Device) -> Result<()> {
        device.logical().end_command_buffer(self.handle)?;
        Ok(())
    }

    pub unsafe fn begin_recording(&self, device: &Device, extent: Extent2D, renderpass: &Renderpass, framebuffer: Framebuffer) -> Result<()> {
        let inheritance_info = vk::CommandBufferInheritanceInfo::builder();
        let info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::empty())
            .inheritance_info(&inheritance_info);

        device.logical().begin_command_buffer(self.handle, &info)?;
        self.begin_renderpass(device, extent, renderpass, framebuffer);

        Ok(())
    }

    pub unsafe fn end_recording(&self, device: &Device) -> Result<()> {
        self.end_renderpass(device);
        device.logical().end_command_buffer(self.handle)?;

        Ok(())
    }

    unsafe fn begin_renderpass(&self, device: &Device, extent: Extent2D, renderpass: &Renderpass, framebuffer: Framebuffer) {
        let render_area = vk::Rect2D::builder()
            .offset(vk::Offset2D::default())
            .extent(extent);

        let clear_values = vec![
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0]
                }
            }
        ];

        let info = vk::RenderPassBeginInfo::builder()
            .render_pass(renderpass.handle())
            .framebuffer(framebuffer)
            .render_area(render_area)
            .clear_values(&clear_values);

        device.logical().cmd_begin_render_pass(self.handle, &info, vk::SubpassContents::INLINE);
    }

    unsafe fn end_renderpass(&self, device: &Device) {
        device.logical().cmd_end_render_pass(self.handle);
    }
}