#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, *};

use crate::{
    Device,
    commands::CommandBuffer, context::Context, image::DepthBuffer
};

#[derive(Clone, Debug)]
pub struct Renderpass {
    handle: vk::RenderPass
}

impl Renderpass {
    pub unsafe fn new(context: &Context, format: Format) -> Result<Self> {
        Ok(Self { handle: Renderpass::build_renderpass(context, format)? })
    }

    unsafe fn build_renderpass(context: &Context, format: Format) -> Result<vk::RenderPass> {
        let attachments = &[
            vk::AttachmentDescription::builder() // Color attachment
                .format(format)
                .samples(vk::SampleCountFlags::_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR),
            vk::AttachmentDescription::builder() // Depth attachment
                .format(DepthBuffer::get_depth_format(context)?)
                .samples(vk::SampleCountFlags::_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        ];

        let color_attachment_refs = vec![
            vk::AttachmentReference::builder()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        ];

        let depth_stencil_attachment_ref = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let subpasses = &[
            vk::SubpassDescription::builder()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_attachment_refs)
                .depth_stencil_attachment(&depth_stencil_attachment_ref)
        ];

        let dependencies = &[
            vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
        ];

        let info = vk::RenderPassCreateInfo::builder()
            .attachments(attachments)
            .subpasses(subpasses)
            .dependencies(dependencies);

        let handle = context.device.logical().create_render_pass(&info, None)?;
        
        info!("+ Handle");

        Ok(handle)
    }

    pub unsafe fn rebuild(&mut self, context: &Context, format: Format) -> Result<()> {
        self.handle = Renderpass::build_renderpass(context, format)?;
        Ok(())
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().destroy_render_pass(self.handle, None);
        self.handle = vk::RenderPass::null();
        info!("~ Handle") 
    }

    pub fn handle(&self) -> vk::RenderPass{
        self.handle
    }

    pub unsafe fn begin(&self, device: &Device, command_buffer: &CommandBuffer, frame_buffer: vk::Framebuffer, extent: vk::Extent2D) {
        let render_area = vk::Rect2D::builder()
            .offset(vk::Offset2D::default())
            .extent(extent);

        let clear_values = &[
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                }
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                }
            }
        ];

        let info = vk::RenderPassBeginInfo::builder()
            .render_pass(self.handle())
            .framebuffer(frame_buffer)
            .render_area(render_area)
            .clear_values(clear_values);

        device.logical().cmd_begin_render_pass(command_buffer.handle(), &info, vk::SubpassContents::INLINE);
    }

    pub unsafe fn end(&self, device: &Device, command_buffer: &CommandBuffer) {
        device.logical().cmd_end_render_pass(command_buffer.handle());
    }
}