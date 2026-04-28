#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use vulkanalia::vk::{self, *};
use log::*;
use anyhow::Result;

use crate::{
    buffers::Allocation, commands::CommandBuffer, device::Device, image::Texture, pipeline::Pipeline
};

type Mat4 = cgmath::Matrix4<f32>;

/* Reminder of how these descriptors actually work.

    DescriptorSetLayout:
        -> binding:
            The binding inside the shader.
            For example:
                layout(binding = 0) uniform UniformBufferObject { ... }
                layout(binding = 1) uniform sampler2D texSampler;

        -> descriptor_type:
            The type of descriptor that will be in a descriptor.
            For example:
                DescriptorType::UNIFORM_BUFFER | DescriptorType::COMBINED_IMAGE_SAMPLER
        -> descriptor_count:
            How many descriptors of this type will be in the descriptor set that will be allocated using this layout

    DescriptorPool:
        -> max_sets:
            Essentially means how many times you can call allocate through allocate_descriptor_sets.
            One of its parameters is an array of descriptor_set_layouts, so available sets in the pool becomes:
                dsl = DescriptorSetLayout {}
                pool = DescriptorPool { max_sets: 8 }
                pool.allocate_descriptor_sets([dsl, dsl ,dsl, dsl])
                
                pool.available_sets: 8 -> 4
                      
        -> pool_sizes:
            The total amount of descriptor pools of a given descriptor type inside the main pool
            When a call to allocate_descriptor_sets is made, it will look at the provided descriptor_set_layouts
            and fetch what it wants from the descriptor pools of any given descriptor type.
            For example:
                dsl = DescriptorSetLayout { UNIFORM_BUFFER: count: 1, COMBINED_IMAGE_SAMPLER: count: 2 }
                pool = DescriptorPool { UNIFORM_BUFFER: count: 8, COMBINED_IMAGE_SAMPLER: count: 8 }
                pool.allocate_descriptor_set([dsl, dsl, dsl])

                # Allocation 1
                pool.uniform_pool_count: 8 -> 7
                pool.combined_image_sampler: 8 -> 6

                # Allocation 2
                pool.uniform_pool_count: 7 -> 6
                pool.combined_image_sampler: 6 -> 4

                # Allocation 3
                pool.uniform_pool_count: 6 -> 5
                pool.combined_image_sampler: 4 -> 2
*/

pub struct DescriptorSetUpdateInfo { pub buffer: vk::Buffer, pub uniforms: Vec<Allocation> }

#[derive(Debug)]
pub struct DescriptorSetLayout {
    handle: vk::DescriptorSetLayout,
}

impl DescriptorSetLayout {
    pub unsafe fn new(device: &Device) -> Result<Self> {
        let bindings = &[
            vk::DescriptorSetLayoutBinding::builder()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX),
            vk::DescriptorSetLayoutBinding::builder()
                .binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
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
    handle: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
}

impl DescriptorPool {
    pub unsafe fn new(device: &Device, max_sets: u32) -> Result<Self> {
        // descriptor_count = swapchain_images_count
        // max_sets = max_frames_in_flight

        let pool_sizes = &[
            vk::DescriptorPoolSize::builder()
                .type_(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(8),
            vk::DescriptorPoolSize::builder()
                .type_(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(8)
        ];

        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(pool_sizes)
            .max_sets(max_sets);

        let handle = device.logical().create_descriptor_pool(&create_info, None)?;

        Ok(Self { handle, descriptor_sets: vec![] })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().destroy_descriptor_pool(self.handle, None);
        self.handle = vk::DescriptorPool::null();
        info!("~ Handle");
    }

    pub unsafe fn allocate_descriptor_sets(&mut self, device: &Device, descriptor_set_layout: &DescriptorSetLayout, descriptor_set_count: usize) -> Result<()> {
        let layouts = vec![descriptor_set_layout.handle(); descriptor_set_count];

        let allocate_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.handle)
            .set_layouts(&layouts);

        self.descriptor_sets = device.logical().allocate_descriptor_sets(&allocate_info)?;

        Ok(())
    }

    pub unsafe fn update(&self, device: &Device, update_info: DescriptorSetUpdateInfo, texture: &Texture) {
        // This method looks to be a better fit for descriptor set layouts

        let mut descriptor_writes: Vec<vk::WriteDescriptorSet> = vec![];

        // Uniform buffer

        let buffer_infos: Vec<vk::DescriptorBufferInfo> = update_info.uniforms
            .iter()
            .map(|uniform_info| {
                vk::DescriptorBufferInfo::builder()
                    .buffer(update_info.buffer)
                    .offset(uniform_info.offset as u64)
                    .range(uniform_info.size as u64)
                    .build()
            })
            .collect();

        buffer_infos
            .iter()
            .enumerate()
            .for_each(|(i, info)| {
                descriptor_writes.push(vk::WriteDescriptorSet::builder()
                    .dst_set(self.descriptor_sets[i])
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(std::slice::from_ref(info))
                    .build()
                );
            });

        // Image

        let image_infos: Vec<vk::DescriptorImageInfo> = (0..update_info.uniforms.len())
            .map(|_| {
                vk::DescriptorImageInfo::builder()
                    .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image_view(texture.view())
                    .sampler(texture.sampler())
                    .build()
            })
            .collect();

        image_infos
            .iter()
            .enumerate()
            .for_each(|(i, info)| {
                descriptor_writes.push(vk::WriteDescriptorSet::builder()
                    .dst_set(self.descriptor_sets[i])
                    .dst_binding(1)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(std::slice::from_ref(info))
                    .build()
                );
            });

        // Update

        device.logical().update_descriptor_sets(
            &descriptor_writes,
            &[] as &[vk::CopyDescriptorSet]
        );
        info!("Updated descriptor sets");
    }

    pub unsafe fn bind(&self, device: &Device, command_buffer: &CommandBuffer, pipeline: &Pipeline, image_index: usize) {
        device.logical().cmd_bind_descriptor_sets(
            command_buffer.handle(),
            vk::PipelineBindPoint::GRAPHICS,
            pipeline.layout(),
            0,
            &[self.descriptor_sets[image_index]],
            &[]
        );
    }

}