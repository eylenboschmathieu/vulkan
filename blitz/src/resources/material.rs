#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::unnecessary_wraps)]

use std::ops::Index;

use log::*;
use anyhow::Result;
use vulkanalia::vk;

use crate::{
    TextureId,
    commands::CommandBuffer,
    device::Device,
    pipeline::{
        pipeline::Pipeline,
        renderpass::Renderpass,
    },
    resources::{
        vertices::*
    }
};

pub type MaterialId = usize;

#[derive(Debug)]
pub struct MaterialDef {
    pub vertex_shader: &'static [u8],
    pub fragment_shader: &'static [u8],
    pub vertex_format: VertexFormat,
    pub textures: u32,
    pub uniforms: u32,
}


#[derive(Debug)]
pub(crate) struct Materials {
    materials: Vec<Material>,
    free_ids: Vec<MaterialId>
}

impl Materials {
    pub fn new() -> Result<Self> {
        Ok(Self { materials: vec![], free_ids: vec![] })
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        for pipeline in &mut self.materials {
            pipeline.destroy(device);
        }
        info!("~ Materials");
    }

    pub unsafe fn alloc(&mut self, device: &Device, pipeline: Pipeline) -> Result<MaterialId> {
        match self.free_ids.pop() {
            Some(id) => {
                self.materials[id] = Material::new(pipeline);
                Ok(id)
            },
            None => {
                self.materials.push(Material::new(pipeline));
                Ok(self.materials.len() - 1)
            },
        }
    }

    pub unsafe fn free(&mut self, device: &Device, id: MaterialId) -> Result<()> {
        self.materials[id].destroy(device);
        Ok(())
    }

    pub unsafe fn clean(&mut self, device: &Device) {
        for material in &mut self.materials {
            material.pipeline.clean(device);
        }
    }

    pub unsafe fn rebuild(&mut self, device: &Device, renderpass: &Renderpass, extent: vk::Extent2D, format: vk::Format) -> Result<()> {
        for material in &mut self.materials {
            material.pipeline.rebuild(device, renderpass, extent, format)?;
        }
        Ok(())
    }

    pub unsafe fn bind(&self, device: &Device, command_buffer: &CommandBuffer, id: MaterialId) {
        self.materials[id].pipeline.bind(device, command_buffer);
    }
}

impl Index<usize> for Materials {
    type Output = Material;

    fn index(&self, index: usize) -> &Self::Output {
        &self.materials[index]
    }
}


#[derive(Debug)]
pub(crate) struct Material {
    pub pipeline: Pipeline,
}

impl Material {
    pub fn new(pipeline: Pipeline) -> Self {
        Self { pipeline }
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        self.pipeline.destroy(device);
        info!("~ Pipeline");
    } 
}