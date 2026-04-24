#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use vulkanalia::vk::{self, *};
use log::*;
use anyhow::Result;

use crate::{buffers::uniform_buffer::UniformBuffer, device::Device, commands::CommandBuffer, pipeline::Pipeline};

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

#[derive(Debug)]
pub struct DescriptorPool {
    device: Device,
    handle: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
}

impl DescriptorPool {
    pub unsafe fn new(device: &Device, descriptor_count: u32) -> Result<Self> {
        let pool_sizes = &[
            vk::DescriptorPoolSize::builder()
                .type_(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(descriptor_count)
        ];

        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(pool_sizes)
            .max_sets(descriptor_count);

        let handle = device.logical().create_descriptor_pool(&create_info, None)?;

        Ok(Self { device: device.clone(), handle, descriptor_sets: vec![] })
    }

    pub unsafe fn destroy(&mut self) {
        self.device.logical().destroy_descriptor_pool(self.handle, None);
        self.handle = vk::DescriptorPool::null();
        info!("~ Handle");
    }

    pub unsafe fn allocate_descriptor_sets(&mut self, descriptor_set_layout: &DescriptorSetLayout, descriptor_set_count: usize) -> Result<()> {
        let layouts = vec![descriptor_set_layout.handle(); descriptor_set_count];

        let allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.handle)
            .set_layouts(&layouts);

        self.descriptor_sets = self.device.logical().allocate_descriptor_sets(&allocate_info)?;

        Ok(())
    }

    pub unsafe fn update(&self, buffers: &[UniformBuffer]) {
        let buffer_infos: Vec<vk::DescriptorBufferInfo> = buffers
            .iter()
            .map(|uniform_buffer| {
                vk::DescriptorBufferInfo::builder()
                    .buffer(uniform_buffer.handle())
                    .offset(0)
                    .range(vk::WHOLE_SIZE)
                    .build()
            })
            .collect();

        let descriptor_writes: Vec<vk::WriteDescriptorSet> = buffer_infos
            .iter()
            .enumerate()
            .map(|(i, info)| {
                vk::WriteDescriptorSet::builder()
                    .dst_set(self.descriptor_sets[i])
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(std::slice::from_ref(info))
                    .build()
            })
            .collect();

        self.device.logical().update_descriptor_sets(&descriptor_writes, &[] as &[vk::CopyDescriptorSet]);
        info!("Updated descriptor sets");
    }

    pub unsafe fn bind(&self, command_buffer: &CommandBuffer, pipeline: &Pipeline, image_index: usize) {
        self.device.logical().cmd_bind_descriptor_sets(
            command_buffer.handle(),
            vk::PipelineBindPoint::GRAPHICS,
            pipeline.layout(),
            0,
            &[self.descriptor_sets[image_index]],
            &[]
        );
    }

}