#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use vulkanalia::vk::{self, *};
use log::*;
use anyhow::{anyhow, Result};

use crate::{
    buffers::buffer::{
            Buffer, TransferDst
        }, commands::CommandBuffer, context::Context, device::Device,
};

pub struct ImageMemoryBarrierQueueFamilyIndices {
    pub src_queue_family_index: u32,
    pub dst_queue_family_index: u32,
}

#[derive(Debug)]
pub struct Image {
    handle: vk::Image,
    memory: vk::DeviceMemory,
    width: u32,
    height: u32,
    size: u64,
}

impl Image {
    pub unsafe fn new(
        context: &Context,
        width: u32,
        height: u32,
        format: vk::Format,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> Result<Self> {
        let handle = Image::build_image(context, width, height, format, tiling, usage)?;
        let memory = Image::build_memory(context, handle, properties)?;
        let size = (width * height * 4) as u64;

        context.device.logical().bind_image_memory(handle, memory, 0)?;

        Ok(Self { handle, memory, width, height, size })
    }

    unsafe fn build_image(context: &Context, width: u32, height: u32, format: vk::Format, tiling: vk::ImageTiling, usage: vk::ImageUsageFlags) -> Result<vk::Image> {
        let device_queue_family_indices = context.device.queue_family_indices();
        // let queue_family_indices = &[device_queue_family_indices.transfer(), device_queue_family_indices.graphics()];

        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::_2D)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(tiling)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            // .queue_family_indices(queue_family_indices)  // Not needed if sharing_mode = SharingMode::EXCLUSIVE
            .samples(vk::SampleCountFlags::_1)
            .flags(vk::ImageCreateFlags::empty());  // Optional

        /*
        The tiling field can have one of two values:
            vk::ImageTiling::LINEAR – Texels are laid out in row-major order like our pixels array.
            vk::ImageTiling::OPTIMAL – Texels are laid out in an implementation defined order for optimal access.
        There are only two possible values for the initial_layout of an image:
            vk::ImageLayout::UNDEFINED – Not usable by the GPU and the very first transition will discard the texels.
            vk::ImageLayout::PREINITIALIZED – Not usable by the GPU, but the first transition will preserve the texels.
        */

        let handle = context.device.logical().create_image(&create_info, None)?;
        info!("+ Handle");

        Ok(handle)
    }

    unsafe fn build_memory(context: &Context, image: vk::Image, properties: vk::MemoryPropertyFlags) -> Result<vk::DeviceMemory> {
        let requirements = context.device.logical().get_image_memory_requirements(image);
        let allocate_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(Buffer::get_memory_type_index(
                context,
                properties,
                requirements)?
            );

        let memory = context.device.logical().allocate_memory(&allocate_info, None)?;
        info!("+ Memory");

        Ok(memory)
    }

    pub fn handle(&self) -> vk::Image {
        self.handle
    }

    pub fn memory(&self) -> vk::DeviceMemory {
        self.memory
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub unsafe fn transition_layout(
        &self,
        device: &Device,
        command_buffer: &CommandBuffer,
        format: vk::Format,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        queue_family_indices: Option<ImageMemoryBarrierQueueFamilyIndices>,
    ) -> Result<()> {
        // debug!("Transitioning layout: {:?} → {:?}", old_layout, new_layout);
        let (src_access_mask, dst_access_mask, src_stage_mask, dst_stage_mask) = match (old_layout, new_layout) {
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
                vk::AccessFlags2::empty(),
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                vk::PipelineStageFlags2::COPY,
            ),
            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
                vk::AccessFlags2::empty(),
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::PipelineStageFlags2::TOP_OF_PIPE,
                vk::PipelineStageFlags2::COPY,
            ),
            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
                vk::AccessFlags2::TRANSFER_WRITE,
                vk::AccessFlags2::SHADER_READ,
                vk::PipelineStageFlags2::COPY,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
            ),
            _ => return Err(anyhow!("Unsupported image layout transition!")),
        };

        let (src_queue_family_index, dst_queue_family_index) = if let Some(indices) = queue_family_indices {
            (indices.src_queue_family_index, indices.dst_queue_family_index)
        } else {
            (vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED)
        };

        let subresource = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        let barriers = &[
            vk::ImageMemoryBarrier2::builder()
                .old_layout(old_layout)
                .new_layout(new_layout)
                .src_queue_family_index(src_queue_family_index)
                .dst_queue_family_index(dst_queue_family_index)
                .image(self.handle)
                .subresource_range(subresource)
                .src_access_mask(src_access_mask)
                .dst_access_mask(dst_access_mask)
                .src_stage_mask(src_stage_mask)
                .dst_stage_mask(dst_stage_mask)
        ];
        debug!("Transitioning layout: {:?} → {:?}", old_layout, new_layout);
        debug!("Access masks: {:?} → {:?}", src_access_mask, dst_access_mask);
        debug!("Stage masks: {:?} → {:?}", src_stage_mask, dst_stage_mask);

        let info = vk::DependencyInfo::builder()
            .image_memory_barriers(barriers);

        device.logical().cmd_pipeline_barrier2(command_buffer.handle(), &info);
        Ok(())
    }

    pub unsafe fn destroy(&self, device: &Device) {
        device.logical().free_memory(self.memory, None);
        device.logical().destroy_image(self.handle, None);
    }
}

impl TransferDst for Image {}