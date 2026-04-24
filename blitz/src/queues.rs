#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

// use std::ops::Index;

use std::ops::Deref;

// use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};

use crate::{
    device::Device,
    commands::CommandBuffer,
    vk::{KhrSwapchainExtensionDeviceCommands, PresentInfoKHR, SubmitInfo}
};

#[derive(Debug)]
pub struct QueuePool {
    graphics: RenderQueue,
    transfer: TransferQueue,
    present: PresentQueue,
}

impl QueuePool {
    pub unsafe fn new(device: &Device) -> Result<Self> {
        Ok(Self {
            graphics: RenderQueue::new(device)?,
            transfer: TransferQueue::new(device)?,
            present: PresentQueue::new(device)?,
        })
    }

    pub fn graphics(&self) -> &RenderQueue {
        &self.graphics
    }

    pub fn transfer(&self) -> &TransferQueue {
        &self.transfer
    }

    pub fn present(&self) -> &PresentQueue {
        &self.present
    }
}

#[derive(Debug)]
pub struct Queue {
    handle: vk::Queue
}

impl Queue {
    pub fn handle(&self) -> vk::Queue {
        self.handle
    }
}

#[derive(Debug)]
pub struct RenderQueue {
    queue: Queue,
}

impl RenderQueue {
    pub unsafe fn new(device: &Device) -> Result<Self> {
        let queue = Queue { handle: device.logical().get_device_queue(device.queue_family_indices().graphics(), 0) };
        Ok(Self { queue })
    }

    pub unsafe fn submit(&self, device: &Device, info: &[SubmitInfo], fence: vk::Fence) -> Result<()> {
        device.logical().queue_submit(self.handle, info, fence)?;
        Ok(())
    }
}

impl Deref for RenderQueue {
    type Target = Queue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

#[derive(Debug)]
pub struct TransferQueue {
    queue: Queue,
}

impl TransferQueue {
    pub unsafe fn new(device: &Device) -> Result<Self> {
        let queue = Queue { handle: device.logical().get_device_queue(device.queue_family_indices().transfer(), 0) };
        Ok(Self { queue })
    }

    pub unsafe fn submit(&self, device: &Device, command_buffer: &CommandBuffer) -> Result<()> {
        let command_buffers = &[command_buffer.handle()];
        let info = vk::SubmitInfo::builder()
            .command_buffers(command_buffers);

        device.logical().queue_submit(self.handle, &[info], vk::Fence::null())?;
        device.logical().queue_wait_idle(self.handle)?;
        Ok(())
    }
}

impl Deref for TransferQueue {
    type Target = Queue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

#[derive(Debug)]
pub struct PresentQueue {
    queue: Queue,
}

impl PresentQueue {
    pub unsafe fn new(device: &Device) -> Result<Self> {
        let queue = Queue { handle: device.logical().get_device_queue(device.queue_family_indices().present(), 0) };
        Ok(Self { queue })
    }

    /// Returns Ok(true) if presenting was successful, but swapchain needs rebuilding, Ok(false) if it doesn't need rebuilding
    pub unsafe fn submit(&self, device: &Device, info: &PresentInfoKHR) -> Result<bool> {
        let result = device.logical().queue_present_khr(self.handle, info);
        let changed = result == Ok(vk::SuccessCode::SUBOPTIMAL_KHR) || result == Err(vk::ErrorCode::OUT_OF_DATE_KHR);
        
        if changed {
            return Ok(true)
        };
        Ok(false)
    }
}

impl Deref for PresentQueue {
    type Target = Queue;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}