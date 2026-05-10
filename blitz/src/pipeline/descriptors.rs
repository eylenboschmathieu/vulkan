#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use vulkanalia::vk::{self, *};
use log::*;
use anyhow::Result;

use crate::globals;

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

/* General rule:
        1 texture -> 1 descriptor set
        uniform buffers -> FRAMES_IN_FLIGHT descriptor sets

        camera uniform buffers are bound to set 0
        textures are bound to set 1
        uniform buffers are bound to set 2
*/

pub struct DescriptorSetUpdateInfo {
    pub descriptor_set: vk::DescriptorSet,
    pub binding: u32,
    pub descriptor_type: vk::DescriptorType,
    pub id: usize, // Uniform Id, or Texture Id
}

pub type DescriptorSetId = usize;

#[derive(Debug)]
pub(crate) struct DescriptorPool {
    handle: vk::DescriptorPool,
}

impl DescriptorPool {
    pub unsafe fn new(max_sets: u32) -> Result<Self> {
        // Just set the pool to contain 16 of each for now

        let pool_sizes = &[
            vk::DescriptorPoolSize::builder()
                .type_(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(16),
            vk::DescriptorPoolSize::builder()
                .type_(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(16),
        ];

        let create_info = vk::DescriptorPoolCreateInfo::builder()
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
            .pool_sizes(pool_sizes)
            .max_sets(max_sets);

        let handle = globals::device().logical().create_descriptor_pool(&create_info, None)?;
        info!("+ DescriptorPool");
        Ok(Self { handle })
    }

    pub unsafe fn destroy(&mut self) {
        globals::device().logical().destroy_descriptor_pool(self.handle, None);
        self.handle = vk::DescriptorPool::null();
        info!("~ DescriptorPool");
    }

    pub unsafe fn alloc(&mut self, layout: vk::DescriptorSetLayout, count: u32) -> Result<Vec<DescriptorSet>> {
        let layouts = vec![layout; count as usize];

        let mut alloc_info: DescriptorSetAllocateInfoBuilder<'_> = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(self.handle)
            .set_layouts(&layouts);

        alloc_info.descriptor_set_count = count;

        Ok(globals::device().logical().allocate_descriptor_sets(&alloc_info)?)
    }

    pub unsafe fn free(&mut self, sets: &[DescriptorSet]) -> Result<()> {
        globals::device().logical().free_descriptor_sets(self.handle, sets)?;
        Ok(())
    }

    pub unsafe fn update(&self, updates: &[DescriptorSetUpdateInfo]) {
        let mut descriptor_writes: Vec<vk::WriteDescriptorSet> = vec![];

        // Intermediate storage to keep buffer/image infos alive until the Vulkan call.
        let mut buffer_infos_store: Vec<vk::DescriptorBufferInfo> = vec![];
        let mut image_infos_store: Vec<vk::DescriptorImageInfo> = vec![];

        for update_info in updates {
            match update_info.descriptor_type {
                vk::DescriptorType::UNIFORM_BUFFER => {
                    let alloc = globals::uniform_buffer().alloc_info(update_info.id);
                    buffer_infos_store.push(vk::DescriptorBufferInfo::builder()
                        .buffer(globals::uniform_buffer().handle())
                        .offset(alloc.offset as u64)
                        .range(alloc.size as u64)
                        .build()
                    );
                    descriptor_writes.push(vk::WriteDescriptorSet::builder()
                        .dst_set(update_info.descriptor_set)
                        .dst_binding(update_info.binding)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(std::slice::from_ref(buffer_infos_store.last().unwrap()))
                        .build()
                    );
                },
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER => {
                    let texture = &globals::textures()[update_info.id];
                    image_infos_store.push(vk::DescriptorImageInfo::builder()
                        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image_view(texture.view())
                        .sampler(texture.sampler())
                        .build()
                    );
                    descriptor_writes.push(vk::WriteDescriptorSet::builder()
                        .dst_set(texture.descriptor_set)
                        .dst_binding(update_info.binding)
                        .dst_array_element(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(std::slice::from_ref(image_infos_store.last().unwrap()))
                        .build()
                    );
                },
                _ => {}
            }
        }

        globals::device().logical().update_descriptor_sets(
            &descriptor_writes,
            &[] as &[vk::CopyDescriptorSet]
        );
        info!("Updated descriptor sets");
    }

    pub unsafe fn update_image_sampler(&self, descriptor_set: vk::DescriptorSet, binding: u32, view: vk::ImageView, sampler: vk::Sampler) {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(view)
            .sampler(sampler)
            .build();

        let write = vk::WriteDescriptorSet::builder()
            .dst_set(descriptor_set)
            .dst_binding(binding)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info))
            .build();

        globals::device().logical().update_descriptor_sets(&[write], &[] as &[vk::CopyDescriptorSet]);
        info!("Updated texture array descriptor set");
    }
}