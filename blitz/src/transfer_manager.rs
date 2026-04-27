#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

// use std::ops::Index;

use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};

use crate::{
    Destroyable, buffers::staging_buffer::StagingBuffer, context::Context, device::Device, image::{Image, ImageMemoryBarrierQueueFamilyIndices}
};

#[derive(Debug)]
pub struct TransferManager {
    semaphore: vk::Semaphore,  // Syncing ownership transfers
}

impl TransferManager {
    pub unsafe fn new(device: &Device) -> Result<Self> {
        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let semaphore = device.logical().create_semaphore(&semaphore_info, None)?;
        info!("+ TransferManager");
        Ok(Self { semaphore })
    }

    pub unsafe fn destroy(&self, device: &Device) {
        device.logical().destroy_semaphore(self.semaphore, None);
        info!("~ TransferManager")
    }

    pub unsafe fn buffer_to_image(&self, context: &Context,  src: &[u8], size: u64, image: &Image) -> Result<()> {
        
        // Data to StagingBuffer

        let mut staging_buffer = StagingBuffer::new(&context, size)?;
        let dst = staging_buffer.map(&context.device, size, 0)?;
        staging_buffer.copy_to_staging(&context.device, &src, dst.cast())?;
        staging_buffer.unmap(&context.device);
        
        // Data from StagingBuffer to Image

        let queue_family_indices = context.device.queue_family_indices();

        let command_buffer= &context.command_manager.begin_one_time_submit(&context.device, vk::QueueFlags::TRANSFER)?;

        image.transition_image_layout(
            &context.device,
            command_buffer,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            None,
        )?;
        staging_buffer.copy_to_image(&context.device, command_buffer, &image)?;
        image.transition_image_layout(
            &context.device,
            command_buffer,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            Some(ImageMemoryBarrierQueueFamilyIndices {  // Release queue ownership
                src_queue_family_index: queue_family_indices.transfer(),
                dst_queue_family_index: queue_family_indices.graphics(),
            }),
        )?;
        command_buffer.end_one_time_submit(&context.device, context.queue_manager.transfer(), Some(self.semaphore))?;
        staging_buffer.destroy(&context.device);

        // Ownership transfer

        let command_buffer = &context.command_manager.begin_one_time_submit(&context.device, vk::QueueFlags::GRAPHICS)?;
        
        image.transition_image_layout(
            &context.device,
            command_buffer,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            Some(ImageMemoryBarrierQueueFamilyIndices {  // Acquire queue ownership
                src_queue_family_index: queue_family_indices.transfer(),
                dst_queue_family_index: queue_family_indices.graphics(),
            }),
        )?;
        command_buffer.end_one_time_submit(&context.device, context.queue_manager.graphics(), Some(self.semaphore))?;

        Ok(())
    }
}