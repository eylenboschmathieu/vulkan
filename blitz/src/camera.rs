#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    iter::zip,
    ops::Index
};

use anyhow::Result;
use cgmath::{vec3, point3, Deg, Matrix4};
use vulkanalia::vk::{self, Handle};

type Mat4 = Matrix4<f32>;

use crate::{
    DescriptorSetUpdateInfo, UniformBufferId, globals, pipeline::descriptor_set_layout::{
        DescriptorSetLayout, DescriptorSetLayoutBuildInfo,
    }, resources::buffers::uniform_buffer::CameraUbo, sync::FRAMES_IN_FLIGHT
};

/// Manages the per-frame camera UBO and its descriptor sets (set 0).
///
/// Allocates one [`CameraUbo`] slot and one descriptor set per frame-in-flight
/// so the GPU can read frame N while the CPU is writing frame N+1.
#[derive(Debug)]
pub(crate) struct Camera {
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    uniform_buffers: [UniformBufferId; FRAMES_IN_FLIGHT],
}

impl Camera {
    pub unsafe fn new(extent: vk::Extent2D) -> Result<Self> {
        let build_info = vec![
            DescriptorSetLayoutBuildInfo {
                binding: 0,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                count: 1,
            }
        ];

        let descriptor_set_layout: vk::DescriptorSetLayout = DescriptorSetLayout::alloc(build_info)?;

        let descriptor_sets = globals::descriptor_pool_mut().alloc(
            descriptor_set_layout,
            FRAMES_IN_FLIGHT as u32,
        )?.try_into()
            .unwrap_or_else(|_| panic!("Expected vector of size {}", FRAMES_IN_FLIGHT));

        let uniform_buffers: [UniformBufferId; FRAMES_IN_FLIGHT] = std::array::from_fn(|_| {
            globals::uniform_buffer_mut().alloc(size_of::<CameraUbo>()).expect("Failed to allocate uniform buffer")
        });

        let updates: Vec<DescriptorSetUpdateInfo> = zip(descriptor_sets, uniform_buffers)
            .into_iter()
            .map(|(descriptor_set, uniform_id)| {
                DescriptorSetUpdateInfo {
                    binding: 0,
                    descriptor_set,
                    descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                    id: uniform_id,
                }
            }).collect();

        globals::descriptor_pool().update(&updates);

        // Init camera

        let model = Mat4::from_scale(1.0);

        let view = Mat4::look_at_rh(
            point3(3.0, 3.0, 3.0),
            point3(0.0, 0.0, 0.0),
            vec3(0.0, 1.0, 0.0),
        );

        let fix = Mat4::new(
            1.0, 0.0, 0.0, 0.0,
            0.0, -1.0, 0.0, 0.0,
            0.0, 0.0, 1.0 / 2.0, 0.0,
            0.0, 0.0, 1.0 / 2.0, 1.0,
        );

        let proj = fix * cgmath::perspective(
            Deg(90.0),
            extent.width as f32 / extent.height as f32,
            0.1,
            100.0,
        );

        let ubo = CameraUbo { model, view, proj };
        for id in uniform_buffers {
            globals::uniform_buffer().update(id, ubo)?;
        }

        Ok(Self {
            descriptor_sets,
            descriptor_set_layout,
            uniform_buffers,
        })
    }

    pub unsafe fn destroy(&mut self) {
        DescriptorSetLayout::free(self.descriptor_set_layout);
        self.descriptor_set_layout = vk::DescriptorSetLayout::null();
    }

    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.descriptor_set_layout
    }

    pub unsafe fn update(&mut self, frame: usize, ubo: CameraUbo) {
        globals::uniform_buffer().update(self.uniform_buffers[frame], ubo).unwrap();
    }
}

// Alternativly, pass the sync object and grab the current frame from there
impl Index<usize> for Camera {
    type Output = vk::DescriptorSet;

    fn index(&self, index: usize) -> &Self::Output {
        &self.descriptor_sets[index]
    }
}