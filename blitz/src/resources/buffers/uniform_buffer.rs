#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::{Deref, DerefMut}, ptr::{copy_nonoverlapping as memcpy}, time::Instant
};
use log::*;
use anyhow::{anyhow, Result};
use cgmath::{vec3, Deg, point3};
use vulkanalia::vk::{self, *};

use crate::{
    resources::buffers::{buffer::Buffer, freelist::{Allocation, Allocator}}, device::Device
};

type Mat4 = cgmath::Matrix4<f32>;
pub type UniformBufferId = usize;

#[repr(C)]
#[derive(Debug)]
pub struct UniformBufferObject {
    model: Mat4,
    view: Mat4,
    proj: Mat4,
}

#[derive(Debug)]
pub struct UniformBuffer {
    buffer: Buffer,
    allocator: Allocator,
    alloc_list: Vec<Allocation>,
    free_list: Vec<UniformBufferId>,
    mapped_ptr: *mut c_void,
}

impl UniformBuffer {
    pub unsafe fn new(device: &Device, count: usize) -> Result<Self> {
        let size = (size_of::<UniformBufferObject>() * count) as u64;

        // Buffer
        
        let handle = Buffer::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::UNIFORM_BUFFER
        )?;
        info!("+ Handle");

        // Memory

        let requirements = device.logical().get_buffer_memory_requirements(handle);

        let memory = Buffer::create_memory(
            device,
            requirements,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        info!("+ Memory");

        // Binding

        device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        let allocator = Allocator::new(size as usize, requirements.alignment as usize);

        let mapped_ptr = device.logical().map_memory(
            memory,
            0,
            size,
            vk::MemoryMapFlags::empty()
        )?;

        Ok(Self { buffer, allocator, alloc_list: vec![], free_list: vec![], mapped_ptr })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().unmap_memory(self.memory());
        self.buffer.destroy(device);
    }

    pub unsafe fn update(&self, device: &Device, id: UniformBufferId, delta: &Instant, extent: vk::Extent2D) -> Result<()> {
        let dt = delta.elapsed().as_secs_f32();

        let model = Mat4::from_axis_angle(
            vec3(0.0, 0.0, 1.0),
            Deg(90.0) * dt
        );

        let view = Mat4::look_at_rh(
            point3(4.0, 4.0, 4.0),
            point3(0.0, 0.0, 0.0),
            vec3(0.0, 0.0, 1.0),
        );

        let fix = Mat4::new(
            1.0, 0.0, 0.0, 0.0,
            0.0, -1.0, 0.0, 0.0,
            0.0, 0.0, 1.0 / 2.0, 0.0,
            0.0, 0.0, 1.0 / 2.0, 1.0,
        );

        let proj = fix * cgmath::perspective(
            Deg(45.0),
            extent.width as f32 / extent.height as f32,
            0.1,
            10.0,
        );

        let ubo = UniformBufferObject { model, view, proj };

        let offset = self.alloc_list[id].offset;

        memcpy(&ubo, self.mapped_ptr.add(offset).cast() , 1);

        Ok(())
    }


    pub fn alloc(&mut self) -> Result<UniformBufferId> {
        if let Some(allocation) = self.allocator.alloc(size_of::<UniformBufferObject>()) {
            if self.free_list.is_empty() {
                self.alloc_list.push(allocation);
                return Ok(self.alloc_list.len() - 1);
            } else {
                let id = self.free_list.pop().unwrap();
                self.alloc_list[id] = allocation;
                return Ok(id);
            }
        };

        Err(anyhow!("Couldn't allocate vertex buffer"))
    }

    pub fn free(&mut self, id: UniformBufferId) {
        self.allocator.free(self.alloc_list[id]);
        self.free_list.push(id);
        self.alloc_list[id] = Allocation { offset: 0, size: 0 };
    }

    pub fn alloc_info(&self, id: UniformBufferId) -> Allocation {
        self.alloc_list[id]
    }

    pub fn get_data(&self) -> Vec<Allocation> {
        self.alloc_list.clone()
    }
}

impl Deref for UniformBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for UniformBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}