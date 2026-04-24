#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use vulkanalia::vk::{self, *};
use log::*;
use anyhow::{anyhow, Result};

use crate::device::Device;

type Mat4 = cgmath::Matrix4<f32>;

#[derive(Debug)]
pub struct DescriptorSetLayout {
    handle: vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    pub unsafe fn new(device: &Device, binding: u32) -> Result<Self> {
        let bindings = &[
            vk::DescriptorSetLayoutBinding::builder()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX),
        ];

        let info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(bindings);

        let handle = device.logical().create_descriptor_set_layout(&info, None)?;
        info!("+ DescriptorSetLayout");
        Ok(Self { handle })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().destroy_descriptor_set_layout(self.handle, None);
        self.handle = vk::DescriptorSetLayout::null();
        info!("~ DescriptorSetLayout");
    }

    pub fn handle(&self) -> vk::DescriptorSetLayout {
        self.handle
    }
}