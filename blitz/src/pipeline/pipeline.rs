#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use log::*;
use anyhow::Result;
use vulkanalia::{
    bytecode::Bytecode, vk::{self, DeviceV1_0, Handle, HasBuilder}
};

use crate::{
    MaterialDef, VertexFormat, commands::CommandBuffer, device::Device, pipeline::renderpass::Renderpass,
};

#[derive(Debug)]
pub(crate) struct Pipelines {

}

impl Pipelines {

}

#[derive(Debug)]
pub(crate) struct Pipeline {
    handle: vk::Pipeline,
    layout: vk::PipelineLayout,

    vertex_format: VertexFormat,
    vertex_shader: &'static [u8],
    fragment_shader: &'static [u8],
}

impl Pipeline {    
    pub unsafe fn new(device: &Device, renderpass: &Renderpass, extent: vk::Extent2D, format: vk::Format, descriptor_set_layouts: &[vk::DescriptorSetLayout], material_def: &MaterialDef) -> Self {
                
        // Layout

        let layout= Pipeline::build_layout(device, descriptor_set_layouts).unwrap();

        // Create
        
        let handle = Pipeline::build_pipeline(device, extent, format, &renderpass, layout, material_def).unwrap();

        Self {
            handle,
            layout,
            vertex_format: material_def.vertex_format.clone(),
            vertex_shader: material_def.vertex_shader,
            fragment_shader: material_def.fragment_shader
        }
    }

    unsafe fn build_pipeline(device: &Device, extent: vk::Extent2D, format: vk::Format, renderpass: &Renderpass, layout: vk::PipelineLayout, material_def: &MaterialDef) -> Result<vk::Pipeline> {

        // Shaders

        let vert = material_def.vertex_shader;
        let frag = material_def.fragment_shader;

        let shaders = vec![ // Used for cleanup later
            Pipeline::build_shader(device, vert)?, // Vertex shader
            Pipeline::build_shader(device, frag)?, // Fragment shader
        ];

        let stages = vec![
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(shaders[0])
                .name(b"main\0"),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(shaders[1])
                .name(b"main\0")
        ];

        // Vertex input

        let binding_descriptions = &[material_def.vertex_format.binding_description(0)];
        let attribute_descriptions = material_def.vertex_format.attribute_description(0);
        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(binding_descriptions)
            .vertex_attribute_descriptions(&attribute_descriptions);

        // Input assembly

        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        // Viewports

        let viewports = vec![
            vk::Viewport::builder()
                .x(0.0)
                .y(0.0)
                .width(extent.width as f32)
                .height(extent.height as f32)
                .min_depth(0.0)
                .max_depth(1.0)
        ];

        let scissors = vec![
            vk::Rect2D::builder()
                .offset(vk::Offset2D { x: 0, y: 0 })
                .extent(extent)
        ];
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .scissors(&scissors);

        // Rasterizer

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::BACK)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false);

        // Multisampling

        let multisampling_state = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::_1);

        // Depth and stencil
        let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(true);
            //.front(vk::StencilOpState::default()) // Optional
            //.back(vk::StencilOpState::default()); // Optional

        // Color blending

        let attachments = vec![vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(false)
            .src_color_blend_factor(vk::BlendFactor::ONE)   // Optional
            .dst_color_blend_factor(vk::BlendFactor::ZERO)  // Optional
            .color_blend_op(vk::BlendOp::ADD) // Optional
            .src_alpha_blend_factor(vk::BlendFactor::ONE) // Optional
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO) // Optional
            .alpha_blend_op(vk::BlendOp::ADD) // Optional
        ];
        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .logic_op(vk::LogicOp::COPY)
            .attachments(&attachments)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        // Create

        let info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&stages)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisampling_state)
            .depth_stencil_state(&depth_stencil_state)
            .color_blend_state(&color_blend_state)
            .layout(layout)
            .render_pass(renderpass.handle())
            .subpass(0)
            .base_pipeline_handle(vk::Pipeline::null())
            .base_pipeline_index(-1); // Optional

        // the creation function returns a tuple when successful. A vector of pipelines, and a success code. Hence the ?.0
        let handle = device.logical().create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)?.0[0];

        info!("+ Handle");

        for shader in shaders {
            device.logical().destroy_shader_module(shader, None);
        }

        Ok(handle)
    }

    unsafe fn build_layout(device: &Device, descriptor_set_layouts: &[vk::DescriptorSetLayout]) -> Result<vk::PipelineLayout> {
        let layout_info = vk::PipelineLayoutCreateInfo::builder()
            .set_layouts(descriptor_set_layouts);

        let layout = device.logical().create_pipeline_layout(&layout_info, None)?;
        info!("+ Layout");
        Ok(layout)
    }

    pub unsafe fn rebuild(&mut self, device: &Device, renderpass: &Renderpass, extent: vk::Extent2D, format: vk::Format) -> Result<()> {
        self.handle = Pipeline::build_pipeline(
            device,
            extent, format,
            renderpass,
            self.layout,
            &MaterialDef {
                vertex_format: self.vertex_format.clone(),
                vertex_shader: self.vertex_shader,
                fragment_shader: self.fragment_shader,
                uniforms: 0,
                textures: 0,
            }
        )?;
        Ok(())
    }
    
    /// Cleaning means destroying the pipeline. Not the layout. Useful for rebuilding a pipeline.
    pub unsafe fn clean(&mut self, device: &Device) {
        device.logical().destroy_pipeline(self.handle, None);
        self.handle = vk::Pipeline::null();
        info!("~ Handle");
    }

    pub unsafe fn destroy(&mut self, device: &Device) {
        device.logical().destroy_pipeline(self.handle, None);
        self.handle = vk::Pipeline::null();
        info!("~ Handle");
        device.logical().destroy_pipeline_layout(self.layout, None);
        self.layout = vk::PipelineLayout::null();
        info!("~ Layout")
    }

    pub fn layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    unsafe fn build_shader(device: &Device, bytecode: &[u8]) -> Result<vk::ShaderModule> {
        let bytecode = Bytecode::new(bytecode)?;

        let info = vk::ShaderModuleCreateInfo::builder()
            .code(bytecode.code())
            .code_size(bytecode.code_size());

        Ok(device.logical().create_shader_module(&info, None)?)
    }

    pub unsafe fn bind(&self, device: &Device, command_buffer: &CommandBuffer) {
        device.logical().cmd_bind_pipeline(command_buffer.handle(), vk::PipelineBindPoint::GRAPHICS, self.handle);
    }
}
