#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

// use std::ops::Index;

use std::ops::Deref;

// use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};

use crate::{
    commands::CommandBuffer, globals, vk::{KhrSwapchainExtensionDeviceCommands, PresentInfoKHR, SubmitInfo}
};

#[derive(Debug)]
pub struct QueueManager {
    graphics: GraphicsQueue,
    transfer: TransferQueue,
    present: PresentQueue,
}

impl QueueManager {
    pub unsafe fn new() -> Result<Self> {
        Ok(Self {
            graphics: GraphicsQueue::new()?,
            transfer: TransferQueue::new()?,
            present: PresentQueue::new()?,
        })
    }

    pub fn graphics(&self) -> &GraphicsQueue {
        &self.graphics
    }

    pub fn transfer(&self) -> &TransferQueue {
        &self.transfer
    }

    pub fn present(&self) -> &PresentQueue {
        &self.present
    }
}

pub trait QueueType {
    unsafe fn submit_transfer(&self, command_buffer: &CommandBuffer, semaphore: Option<vk::Semaphore>) -> Result<()>;
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
pub struct GraphicsQueue {
    queue: Queue,
}

impl QueueType for GraphicsQueue {
    unsafe fn submit_transfer(&self, command_buffer: &CommandBuffer, wait_semaphore: Option<vk::Semaphore>) -> Result<()> {
        let wait_semaphores = &[wait_semaphore];
        let command_buffers = &[command_buffer.handle()];

        let submit_info = vk::SubmitInfo::builder()
            //.wait_semaphores(wait_semaphores)
            //.wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .command_buffers(command_buffers);

        if let Some(signal) = wait_semaphore {
            let wait_semaphores = &[signal];
            submit_info.signal_semaphores(wait_semaphores);
        }
        globals::device().logical().queue_submit(self.handle, &[submit_info], vk::Fence::null())?;
        Ok(())
    }
}

impl GraphicsQueue {
    pub unsafe fn new() -> Result<Self> {
        let queue = Queue { handle: globals::device().logical().get_device_queue(globals::device().queue_family_indices().graphics(), 0) };
        Ok(Self { queue })
    }

    pub unsafe fn submit(&self, info: &[SubmitInfo], fence: vk::Fence) -> Result<()> {
        globals::device().logical().queue_submit(self.handle, info, fence)?;
        Ok(())
    }
}

impl Deref for GraphicsQueue {
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
    pub unsafe fn new() -> Result<Self> {
        let queue = Queue { handle: globals::device().logical().get_device_queue(globals::device().queue_family_indices().transfer(), 0) };
        Ok(Self { queue })
    }
}

impl QueueType for TransferQueue {
    unsafe fn submit_transfer(&self, command_buffer: &CommandBuffer, signal_semaphore: Option<vk::Semaphore>) -> Result<()> {
        let command_buffers = &[command_buffer.handle()];

        let info = vk::SubmitInfo::builder()
            .command_buffers(command_buffers);

        if let Some(signal) = signal_semaphore {
            let signal_semaphores = &[signal];
            info.signal_semaphores(signal_semaphores);
        }

        globals::device().logical().queue_submit(self.handle, &[info], vk::Fence::null())?;
        globals::device().logical().queue_wait_idle(self.handle)?;
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
    pub unsafe fn new() -> Result<Self> {
        let queue = Queue { handle: globals::device().logical().get_device_queue(globals::device().queue_family_indices().present(), 0) };
        Ok(Self { queue })
    }

    /// Returns Ok(true) if presenting was successful, but swapchain needs rebuilding, Ok(false) if it doesn't need rebuilding
    pub unsafe fn submit(&self, info: &PresentInfoKHR) -> Result<bool> {
        let result = globals::device().logical().queue_present_khr(self.handle, info);
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