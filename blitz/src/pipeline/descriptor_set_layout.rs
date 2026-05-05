#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]


use anyhow::{Result, *};
use log::*;
use vulkanalia::vk::{self, DescriptorSetLayoutBinding, DeviceV1_0, HasBuilder};

use crate::globals;


#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) struct DescriptorSetLayoutBuildInfo {
    pub(crate) binding: u32,
    pub(crate) descriptor_type: vk::DescriptorType,
    pub(crate) count: u32,
}

#[derive(Debug)]
pub(crate) struct DescriptorSetLayout {}

impl DescriptorSetLayout {
    pub unsafe fn alloc(build_info: Vec<DescriptorSetLayoutBuildInfo>) -> Result<vk::DescriptorSetLayout> {
        // Setting descriptor_count > 0 creates an array on a single binding
        let bindings: Vec<DescriptorSetLayoutBinding> = build_info
            .iter()
            .map(|info| {
                match info.descriptor_type {
                    vk::DescriptorType::UNIFORM_BUFFER =>
                        Ok(vk::DescriptorSetLayoutBinding::builder()
                            .binding(info.binding)
                            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                            .descriptor_count(info.count)
                            .stage_flags(vk::ShaderStageFlags::VERTEX)
                            .build()),
                    vk::DescriptorType::COMBINED_IMAGE_SAMPLER =>
                        Ok(vk::DescriptorSetLayoutBinding::builder()
                            .binding(info.binding)
                            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .descriptor_count(info.count)
                            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                            .build()),
                    _ => return Err(anyhow!("Bad descriptor type"))
                }
            })
            .collect::<Result<Vec<_>>>()?;

        let info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings);

        let handle = globals::device().logical().create_descriptor_set_layout(&info, None)?;
        info!("+ DescriptorSetLayout");

        Ok(handle)
    }

    pub unsafe fn free(descriptor_set_layout: vk::DescriptorSetLayout) {
        globals::device().logical().destroy_descriptor_set_layout(descriptor_set_layout, None);
    }
}