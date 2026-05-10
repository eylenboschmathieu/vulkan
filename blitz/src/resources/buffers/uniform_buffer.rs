#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::{Deref, DerefMut}, ptr::copy_nonoverlapping as memcpy,
};
use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::vk::{self, *};

use crate::{
    globals,
    resources::buffers::{buffer::Buffer, freelist::{Allocation, Allocator}},
};

type Mat4 = cgmath::Matrix4<f32>;
type Vec4 = cgmath::Vector4<f32>;
pub type UniformBufferId = usize;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CameraUbo {
    pub model: Mat4,
    pub view:  Mat4,
    pub proj:  Mat4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LightingUbo {
    pub sun_dir: Vec4,
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
    pub unsafe fn new(count: usize) -> Result<Self> {
        let size = (size_of::<CameraUbo>() * count) as u64;

        // Buffer

        let handle = Buffer::create_buffer(
            size,
            vk::BufferUsageFlags::UNIFORM_BUFFER
        )?;
        info!("+ Handle");

        // Memory

        let requirements = globals::device().logical().get_buffer_memory_requirements(handle);

        let memory = Buffer::create_memory(
            requirements,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        info!("+ Memory");

        // Binding

        globals::device().logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        let allocator = Allocator::new(size as usize, requirements.alignment as usize);

        let mapped_ptr = globals::device().logical().map_memory(
            memory,
            0,
            size,
            vk::MemoryMapFlags::empty()
        )?;

        Ok(Self { buffer, allocator, alloc_list: vec![], free_list: vec![], mapped_ptr })
    }

    pub unsafe fn destroy(&mut self) {
        globals::device().logical().unmap_memory(self.memory());
        self.buffer.destroy();
    }

    pub unsafe fn update<T: Copy>(&self, id: UniformBufferId, data: T) -> Result<()> {
        let offset = self.alloc_list[id].offset;
        memcpy(&data, self.mapped_ptr.add(offset).cast(), 1);
        Ok(())
    }


    pub fn alloc(&mut self, size: usize) -> Result<UniformBufferId> {
        if let Some(allocation) = self.allocator.alloc(size) {
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