#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ops::{Deref, DerefMut}, ptr::copy_nonoverlapping as memcpy, time::Instant
};
use log::*;
use anyhow::{anyhow, Result};
use cgmath::{vec3, Deg, point3};
use vulkanalia::vk::{self, *};

use crate::{
    instance::Instance,
    device::Device,
    buffers::buffer::Buffer,
};

type Mat4 = cgmath::Matrix4<f32>;

#[repr(C)]
#[derive(Debug)]
struct UniformBufferObject {
    model: Mat4,
    view: Mat4,
    proj: Mat4,
}

#[derive(Debug)]
pub struct UniformBuffer {
    buffer: Buffer,
}

impl UniformBuffer {
    pub unsafe fn new(instance: &Instance, device: &Device) -> Result<Self> {// Size
        let size = size_of::<UniformBufferObject>() as u64;
        // Buffer
        
        let handle = Buffer::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::UNIFORM_BUFFER
        )?;
        info!("+ Handle");

        // Memory

        let memory = Buffer::create_memory(instance,
            device,
            handle,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE
        )?;
        info!("+ Memory");

        // Binding

        device.logical().bind_buffer_memory(handle, memory, 0)?;

        let buffer = Buffer::new(handle, memory, size)?;

        Ok(Self { buffer })
    }

    pub unsafe fn update(&self, device: &Device, delta: &Instant, extent: vk::Extent2D) -> Result<()> {
        let time = delta.elapsed().as_secs_f32();

        let model = Mat4::from_axis_angle(
            vec3(0.0, 0.0, 1.0),
            Deg(90.0) * time
        );

        let view = Mat4::look_at_rh(
            point3(2.0, 2.0, 2.0),
            point3(0.0, 0.0, 0.0),
            vec3(0.0, 0.0, 1.0),
        );

        let mut proj = cgmath::perspective(
            Deg(45.0),
            extent.width as f32 / extent.height as f32,
            0.1,
            10.0,
        );

        proj[1][1] *= -1.0;

        let ubo = UniformBufferObject { model, view, proj };
        let dst = device.logical().map_memory(
            self.memory(),
            0,
            size_of::<UniformBufferObject>() as u64,
            vk::MemoryMapFlags::empty()
        )?;

        memcpy(&ubo, dst.cast(), 1);

        device.logical().unmap_memory(self.memory());

        Ok(())
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