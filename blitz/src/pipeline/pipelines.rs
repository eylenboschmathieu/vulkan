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

/// The graphics pipelines used by the renderer.
///
/// Each field corresponds to a distinct rendering path:
/// - `mesh_static`  — world-space meshes with a baked-in identity transform
/// - `mesh_dynamic` — per-object transforms uploaded via push constants
/// - `mesh_color`   — per-object transforms, vertex color only (no texture)
/// - `chunk`        — voxel chunk geometry sampling a `sampler2DArray` tile atlas
/// - `ui`           — 2D screen-space quads with alpha blending, no depth test
#[derive(Debug)]
pub(crate) struct Pipelines {
    pub mesh_static:  Pipeline,
    pub mesh_dynamic: Pipeline,
    pub mesh_color:   Pipeline,
    pub chunk:        Pipeline,
    pub ui:           Pipeline,
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
            depth_test:      true,
            alpha_blend:     false,
        })?;

        let mesh_dynamic = Pipeline::new(renderpass, extent, format, layouts, &PipelineDef {
            vertex_format:   VertexFormat::Vertex3D_Color_Texture,
            vertex_shader:   include_bytes!("../../shaders/mesh_dynamic.vert.spv"),
            fragment_shader: include_bytes!("../../shaders/mesh_dynamic.frag.spv"),
            push_constants:  true,
            depth_test:      true,
            alpha_blend:     false,
        })?;

        // Color-only dynamic pipeline: camera (set 0) + push constants, no texture.
        let mesh_color = Pipeline::new(renderpass, extent, format, &[layouts[0]], &PipelineDef {
            vertex_format:   VertexFormat::Vertex3D_Color,
            vertex_shader:   include_bytes!("../../shaders/mesh_color.vert.spv"),
            fragment_shader: include_bytes!("../../shaders/mesh_color.frag.spv"),
            push_constants:  true,
            depth_test:      true,
            alpha_blend:     false,
        })?;

        let chunk = Pipeline::new(renderpass, extent, format, layouts, &PipelineDef {
            vertex_format:   VertexFormat::Vertex3D_TextureArray,
            vertex_shader:   include_bytes!("../../shaders/chunk.vert.spv"),
            fragment_shader: include_bytes!("../../shaders/chunk.frag.spv"),
            push_constants:  false,
            depth_test:      true,
            alpha_blend:     false,
        })?;

        // UI pipeline: vertex color only, no descriptor sets, push constants for ortho matrix.
        let ui = Pipeline::new(renderpass, extent, format, &[], &PipelineDef {
            vertex_format:   VertexFormat::Vertex2D_Color,
            vertex_shader:   include_bytes!("../../shaders/ui.vert.spv"),
            fragment_shader: include_bytes!("../../shaders/ui.frag.spv"),
            push_constants:  true,
            depth_test:      false,
            alpha_blend:     false,
        })?;

        Ok(Self { mesh_static, mesh_dynamic, mesh_color, chunk, ui })
    }

    pub unsafe fn destroy(&mut self) {
        self.mesh_static.destroy();
        self.mesh_dynamic.destroy();
        self.mesh_color.destroy();
        self.chunk.destroy();
        self.ui.destroy();
    }
}
