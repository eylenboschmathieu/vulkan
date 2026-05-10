#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use anyhow::Result;
use log::*;
use vulkanalia::vk::{self, DeviceV1_0, Handle, HasBuilder};

use crate::{
    commands::CommandBuffer, globals, swapchain::Swapchain
};

pub(crate) const FRAMES_IN_FLIGHT: usize = 2;

// Structure containing per frame objects
#[derive(Clone, Debug)]
struct FrameSync {
    image_available_semaphore: vk::Semaphore,
    in_flight_fence: vk::Fence,
}

// Helper class to deal with synchronization
#[derive(Clone, Debug)]
pub(crate) struct Synchronization {
    frames: Vec<FrameSync>,
    // One per swapchain image: when acquire returns an image, the presentation engine
    // guarantees it has consumed this semaphore, making it safe to signal again.
    render_finished_semaphores: Vec<vk::Semaphore>,
    images_in_flight_fences: Vec<vk::Fence>,
    pub frame: usize,
    pub image: usize,
    pub command_buffer: CommandBuffer,
}

impl Synchronization {
    pub unsafe fn new(swapchain: &Swapchain) -> Result<Self> {
        let swapchain_image_count = swapchain.framebuffer_count();

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let fence_info = vk::FenceCreateInfo::builder()
            .flags(vk::FenceCreateFlags::SIGNALED);

        let mut frames = vec![];

        for _ in 0..FRAMES_IN_FLIGHT {
            frames.push(FrameSync {
                image_available_semaphore: globals::device().logical().create_semaphore(&semaphore_info, None)?,
                in_flight_fence: globals::device().logical().create_fence(&fence_info, None)?,
            });
        }

        let mut render_finished_semaphores = vec![];
        for _ in 0..swapchain_image_count {
            render_finished_semaphores.push(globals::device().logical().create_semaphore(&semaphore_info, None)?);
        }

        let images_in_flight_fences = vec![vk::Fence::null(); swapchain_image_count];

        info!("+ Synchronization");
        Ok(Self { frames, render_finished_semaphores, images_in_flight_fences, frame: 0, image: 0, command_buffer: CommandBuffer::default() })
    }
    
    pub unsafe fn destroy(&self) {
        for frame in &self.frames {
            globals::device().logical().destroy_fence(frame.in_flight_fence, None);
            globals::device().logical().destroy_semaphore(frame.image_available_semaphore, None);
        }
        for semaphore in &self.render_finished_semaphores {
            globals::device().logical().destroy_semaphore(*semaphore, None);
        }
        info!("~ Synchronization")
    }

    pub fn image_available_semaphore(&self) -> vk::Semaphore {
        self.frames[self.frame].image_available_semaphore
    }

    pub fn render_finished_semaphore(&self) -> vk::Semaphore {
        self.render_finished_semaphores[self.image]
    }

    pub fn in_flight_fence(&self) -> vk::Fence {
        self.frames[self.frame].in_flight_fence
    }

    pub fn images_in_flight_fence(&self) -> vk::Fence {
        self.images_in_flight_fences[self.image]
    }
    
    pub unsafe fn update_image_in_flight_fence(&mut self) {
        self.images_in_flight_fences[self.image] = self.in_flight_fence();
    }
}
