#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use log::*;
use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0, Format, Handle, HasBuilder};

use crate::{
    Device,
};

#[derive(Clone, Debug)]
pub struct Renderpass {
    handle: vk::RenderPass
}

impl Renderpass {
    pub unsafe fn new(device: &Device, format: Format) -> Result<Self> {
        let color_attachments = vec![
            vk::AttachmentDescription::builder()
                .format(format)
                .samples(vk::SampleCountFlags::_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        ];

        let color_attachment_refs = vec![
            vk::AttachmentReference::builder()
                .attachment(0)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        ];

        let dependencies = vec![
            vk::SubpassDependency::builder()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        ];

        let subpasses = vec![
            vk::SubpassDescription::builder()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_attachment_refs)
        ];

        let info = vk::RenderPassCreateInfo::builder()
            .attachments(&color_attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);

        let handle = device.logical().create_render_pass(&info, None)?;
        
        info!("+ Handle");

        Ok(Self { handle })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().destroy_render_pass(self.handle, None);
        self.handle = vk::RenderPass::null();
        info!("~ Handle") 
    }

    pub fn handle(&self) -> vk::RenderPass{
        self.handle
    }
}