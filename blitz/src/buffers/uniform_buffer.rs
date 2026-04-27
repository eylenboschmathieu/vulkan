#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ffi::c_void, ops::{Deref, DerefMut}, ptr::copy_nonoverlapping as memcpy, time::Instant
};
use log::*;
use anyhow::Result;
use cgmath::{vec3, Deg, point3};
use vulkanalia::vk::{self, *};

use crate::{
    buffers::buffer::Buffer, context::Context, device::Device
};

type Mat4 = cgmath::Matrix4<f32>;

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
    mapped_ptr: *mut c_void,
}

impl UniformBuffer {
    pub unsafe fn new(context: &Context) -> Result<Self> {// Size
        let size = size_of::<UniformBufferObject>() as u64;
        // Buffer
        
        let handle = Buffer::create_buffer(
            &context.device,
            size,
            vk::BufferUsageFlags::UNIFORM_BUFFER
        )?;
        info!("+ Handle");

        // Memory

        let memory = Buffer::create_memory(
            context,
            handle,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        info!("+ Memory");

        // Binding

        context.device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        // Persistent mapping
        let mapped_ptr = context.device.logical().map_memory(
            buffer.memory(),
            0,
            size_of::<UniformBufferObject>() as u64,
            vk::MemoryMapFlags::empty()
        )?;

        Ok(Self { buffer, mapped_ptr })
    }

    pub unsafe fn update(&self, device: &Device, delta: &Instant, extent: vk::Extent2D) -> Result<()> {
        let dt = delta.elapsed().as_secs_f32();

        let model = Mat4::from_axis_angle(
            vec3(0.0, 0.0, 1.0),
            Deg(90.0) * dt
        );

        let view = Mat4::look_at_rh(
            point3(2.0, 2.0, 2.0),
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

        memcpy(&ubo, self.mapped_ptr.cast(), 1);

        Ok(())
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().unmap_memory(self.memory());
        self.buffer.destroy(device);
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