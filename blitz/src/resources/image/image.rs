#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::Deref;

use vulkanalia::vk::{self, *};
use log::*;
use anyhow::{anyhow, Result};

use crate::{
    globals,
    resources::buffers::buffer::{
            Buffer, TransferDst
        }, commands::CommandBuffer,
};

pub struct ImageMemoryBarrierQueueFamilyIndices {
    pub src_queue_family_index: u32,
    pub dst_queue_family_index: u32,
}

#[derive(Debug, Clone)]
pub struct Image {
    handle: vk::Image,
    memory: vk::DeviceMemory,
    width: u32,
    height: u32,
    array_layers: u32,
    size: u64,
}

impl Image {
    pub unsafe fn new(
        width: u32,
        height: u32,
        format: vk::Format,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> Result<Self> {
        let handle = Image::build_image(width, height, 1, format, tiling, usage)?;
        let memory = Image::build_memory(handle, properties)?;
        let size = (width * height * 4) as u64;

        globals::device().logical().bind_image_memory(handle, memory, 0)?;

        Ok(Self { handle, memory, width, height, array_layers: 1, size })
    }

    pub unsafe fn new_array(
        width: u32,
        height: u32,
        array_layers: u32,
        format: vk::Format,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
        properties: vk::MemoryPropertyFlags,
    ) -> Result<Self> {
        let handle = Image::build_image(width, height, array_layers, format, tiling, usage)?;
        let memory = Image::build_memory(handle, properties)?;
        let size = (width * height * 4 * array_layers) as u64;

        globals::device().logical().bind_image_memory(handle, memory, 0)?;

        Ok(Self { handle, memory, width, height, array_layers, size })
    }

    unsafe fn build_image(width: u32, height: u32, array_layers: u32, format: vk::Format, tiling: vk::ImageTiling, usage: vk::ImageUsageFlags) -> Result<vk::Image> {
        let create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::_2D)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(array_layers)
            .format(format)
            .tiling(tiling)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::_1)
            .flags(vk::ImageCreateFlags::empty());

        let handle = globals::device().logical().create_image(&create_info, None)?;
        info!("+ Handle");

        Ok(handle)
    }

    pub unsafe fn build_view_array(image: vk::Image, format: vk::Format, layer_count: u32) -> Result<vk::ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(layer_count);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D_ARRAY)
            .format(format)
            .subresource_range(subresource_range);

        Ok(globals::device().logical().create_image_view(&create_info, None)?)
    }

    unsafe fn build_memory(image: vk::Image, properties: vk::MemoryPropertyFlags) -> Result<vk::DeviceMemory> {
        let requirements = globals::device().logical().get_image_memory_requirements(image);
        let allocate_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(Buffer::get_memory_type_index(
                properties,
                requirements)?
            );

        let memory = globals::device().logical().allocate_memory(&allocate_info, None)?;
        info!("+ Memory");

        Ok(memory)
    }

    pub unsafe fn build_view(image: vk::Image, format: vk::Format, aspects: vk::ImageAspectFlags) -> Result<vk::ImageView> {
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1)
            .aspect_mask(aspects);

        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D)
            .format(format)
            .subresource_range(subresource_range);

        let view = globals::device().logical().create_image_view(&create_info, None)?;

        Ok(view)
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

    pub fn array_layers(&self) -> u32 {
        self.array_layers
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub unsafe fn transition_image_layout(
        &self,
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
            .layer_count(self.array_layers);

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

        let info = vk::DependencyInfo::builder()
            .image_memory_barriers(barriers);

        globals::device().logical().cmd_pipeline_barrier2(command_buffer.handle(), &info);
        Ok(())
    }

    pub unsafe fn transition_depth_layout(
        &self,
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
        // debug!("Transitioning layout: {:?} → {:?}", old_layout, new_layout);
        // debug!("Access masks: {:?} → {:?}", src_access_mask, dst_access_mask);
        // debug!("Stage masks: {:?} → {:?}", src_stage_mask, dst_stage_mask);

        let info = vk::DependencyInfo::builder()
            .image_memory_barriers(barriers);

        globals::device().logical().cmd_pipeline_barrier2(command_buffer.handle(), &info);
        Ok(())
    }

    pub unsafe fn destroy(&self) {
        globals::device().logical().free_memory(self.memory, None);
        globals::device().logical().destroy_image(self.handle, None);
    }
}

impl TransferDst for Image {}

#[derive(Debug, Clone)]
pub struct DepthBuffer {
    image: Image,
    view: vk::ImageView,
}

impl DepthBuffer {
    pub unsafe fn new(width: u32, height: u32) -> Result<Self> {
        let format = DepthBuffer::get_depth_format()?;

        let image = Image::new(
            width, height,
            format,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::MemoryPropertyFlags::DEVICE_LOCAL)?;

        let view = Image::build_view(image.handle(), format, vk::ImageAspectFlags::DEPTH)?;

        info!("+ DepthBuffer");
        Ok(Self { image, view })
    }

    pub unsafe fn get_depth_format() -> Result<vk::Format> {
        let candidates = &[
            vk::Format::D32_SFLOAT,
            vk::Format::D32_SFLOAT_S8_UINT,
            vk::Format::D24_UNORM_S8_UINT,
        ];

        globals::instance().get_supported_format(
            globals::device(),
            candidates,
            vk::ImageTiling::OPTIMAL,
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT
        )
    }

    pub unsafe fn destroy(&self) {
        globals::device().logical().destroy_image_view(self.view, None);
        self.image.destroy();
        info!("~ DepthBuffer")
    }

    pub fn view(&self) -> vk::ImageView {
        self.view
    }
}

impl Deref for DepthBuffer {
    type Target = Image;

    fn deref(&self) -> &Self::Target {
        &self.image
    }
}