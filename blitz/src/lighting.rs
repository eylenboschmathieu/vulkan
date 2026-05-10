#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use std::{iter::zip, mem::size_of, ops::Index};
use anyhow::Result;
use vulkanalia::vk::{self, Handle};
use log::*;

use crate::{
    DescriptorSetUpdateInfo, globals,
    pipeline::descriptor_set_layout::{DescriptorSetLayout, DescriptorSetLayoutBuildInfo},
    resources::buffers::uniform_buffer::{LightingUbo, UniformBufferId},
    sync::FRAMES_IN_FLIGHT,
};

#[derive(Debug)]
pub(crate) struct Lighting {
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    uniform_buffers: [UniformBufferId; FRAMES_IN_FLIGHT],
}

impl Lighting {
    pub unsafe fn new() -> Result<Self> {
        let build_info = vec![
            DescriptorSetLayoutBuildInfo {
                binding: 0,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                count: 1,
            }
        ];

        let descriptor_set_layout = DescriptorSetLayout::alloc(build_info)?;

        let descriptor_sets = globals::descriptor_pool_mut().alloc(
            descriptor_set_layout,
            FRAMES_IN_FLIGHT as u32,
        )?.try_into()
            .unwrap_or_else(|_| panic!("Expected vector of size {}", FRAMES_IN_FLIGHT));

        let uniform_buffers: [UniformBufferId; FRAMES_IN_FLIGHT] = std::array::from_fn(|_| {
            globals::uniform_buffer_mut().alloc(size_of::<LightingUbo>()).expect("Failed to allocate lighting buffer")
        });

        let updates: Vec<DescriptorSetUpdateInfo> = zip(descriptor_sets, uniform_buffers)
            .map(|(descriptor_set, uniform_id)| DescriptorSetUpdateInfo {
                binding: 0,
                descriptor_set,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                id: uniform_id,
            }).collect();

        globals::descriptor_pool().update(&updates);

        info!("+ Lighting");
        Ok(Self { descriptor_set_layout, descriptor_sets, uniform_buffers })
    }

    pub unsafe fn destroy(&mut self) {
        DescriptorSetLayout::free(self.descriptor_set_layout);
        self.descriptor_set_layout = vk::DescriptorSetLayout::null();
        info!("~ Lighting");
    }

    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor_set_layout
    }

    pub unsafe fn update(&mut self, frame: usize, ubo: LightingUbo) {
        globals::uniform_buffer().update(self.uniform_buffers[frame], ubo).unwrap();
    }
}

impl Index<usize> for Lighting {
    type Output = vk::DescriptorSet;
    fn index(&self, index: usize) -> &Self::Output {
        &self.descriptor_sets[index]
    }
}
