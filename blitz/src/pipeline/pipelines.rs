#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use anyhow::Result;
use vulkanalia::vk;

use crate::{
    pipeline::{
        pipeline::{Pipeline, PipelineDef},
        renderpass::Renderpass,
    },
    resources::vertices::VertexFormat,
};

/// The three graphics pipelines used by the renderer.
///
/// Each field corresponds to a distinct rendering path:
/// - `mesh_static`  — world-space meshes with a baked-in identity transform
/// - `mesh_dynamic` — per-object transforms uploaded via push constants
/// - `chunk`        — voxel chunk geometry sampling a `sampler2DArray` tile atlas
#[derive(Debug)]
pub(crate) struct Pipelines {
    pub mesh_static:  Pipeline,
    pub mesh_dynamic: Pipeline,
    pub chunk:        Pipeline,
}

impl Pipelines {
    pub unsafe fn new(
        renderpass: &Renderpass,
        extent: vk::Extent2D,
        format: vk::Format,
        layouts: &[vk::DescriptorSetLayout],
    ) -> Result<Self> {
        let mesh_static = Pipeline::new(renderpass, extent, format, layouts, &PipelineDef {
            vertex_format:   VertexFormat::Vertex3D_Color_Texture,
            vertex_shader:   include_bytes!("../../shaders/mesh_static.vert.spv"),
            fragment_shader: include_bytes!("../../shaders/mesh_static.frag.spv"),
            push_constants:  false,
        })?;

        let mesh_dynamic = Pipeline::new(renderpass, extent, format, layouts, &PipelineDef {
            vertex_format:   VertexFormat::Vertex3D_Color_Texture,
            vertex_shader:   include_bytes!("../../shaders/mesh_dynamic.vert.spv"),
            fragment_shader: include_bytes!("../../shaders/mesh_dynamic.frag.spv"),
            push_constants:  true,
        })?;

        let chunk = Pipeline::new(renderpass, extent, format, layouts, &PipelineDef {
            vertex_format:   VertexFormat::Vertex3D_TextureArray,
            vertex_shader:   include_bytes!("../../shaders/chunk.vert.spv"),
            fragment_shader: include_bytes!("../../shaders/chunk.frag.spv"),
            push_constants:  false,
        })?;

        Ok(Self { mesh_static, mesh_dynamic, chunk })
    }

    pub unsafe fn destroy(&mut self) {
        self.mesh_static.destroy();
        self.mesh_dynamic.destroy();
        self.chunk.destroy();
    }
}
