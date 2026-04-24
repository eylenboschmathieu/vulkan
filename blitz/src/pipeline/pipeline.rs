#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use log::*;
use anyhow::Result;
use vulkanalia::{
    bytecode::Bytecode, vk::{self, DeviceV1_0, Extent2D, Handle, HasBuilder}
};

use crate::{
    buffers::buffer::Vertex, commands::CommandBuffer, device::Device, renderpass::Renderpass
};


#[derive(Debug)]
pub struct Pipeline {
    device: Device,
    handle: vk::Pipeline,
    layout: vk::PipelineLayout,
    renderpass: Renderpass,
}

impl Pipeline {
    pub unsafe fn new(device: &Device, extent: Extent2D, format: vk::Format, descriptor_set_layouts: &[vk::DescriptorSetLayout]) -> Result<Self> {

        // Renderpass
        
        let renderpass = Renderpass::new(device, format)?;
        
        // Layout

        let layout= Pipeline::build_layout(device, descriptor_set_layouts)?;

        // Create
        
        let handle = Pipeline::build_pipeline(device, extent, format, &renderpass, &layout)?;

        Ok(Self { device: device.clone(), handle, layout, renderpass })
    }

    unsafe fn build_pipeline(device: &Device, extent: vk::Extent2D, format: vk::Format, renderpass: &Renderpass, layout: &vk::PipelineLayout) -> Result<vk::Pipeline> {

        // Shaders

        let vert = include_bytes!("../../shaders/ubo.vert.spv");
        let frag = include_bytes!("../../shaders/ubo.frag.spv");

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

        let binding_descriptions = &[Vertex::binding_description()];
        let attribute_descriptions = Vertex::attribute_description();
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
            .front_face(vk::FrontFace::CLOCKWISE)
            .depth_bias_enable(false);

        // Multisampling

        let multisampling_state = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::_1);

        // Depth and stencil

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
            .color_blend_state(&color_blend_state)
            .layout(*layout)
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

    pub unsafe fn rebuild(&mut self, extent: vk::Extent2D, format: vk::Format) -> Result<()> {
        self.renderpass = Renderpass::new(&self.device, format)?;
        self.handle = Pipeline::build_pipeline(&self.device, extent, format, &self.renderpass, &self.layout)?;
        Ok(())
    }
    
    /// Cleaning means destroying the pipeline, and the renderpass. Not the layout. Useful for rebuilding a pipeline.
    pub unsafe fn clean(&mut self) {
        self.device.logical().destroy_pipeline(self.handle, None);
        self.handle = vk::Pipeline::null();
        info!("~ Handle");
        self.renderpass.destroy(&self.device);
    }

    pub unsafe fn destroy(&mut self) {
        self.device.logical().destroy_pipeline(self.handle, None);
        self.handle = vk::Pipeline::null();
        info!("~ Handle");
        self.renderpass.destroy(&self.device);
        self.device.logical().destroy_pipeline_layout(self.layout, None);
        self.layout = vk::PipelineLayout::null();
        info!("~ Layout")
    }

    pub fn renderpass(&self) -> &Renderpass {
        &self.renderpass
    }

    unsafe fn build_shader(device: &Device, bytecode: &[u8]) -> Result<vk::ShaderModule> {
        let bytecode = Bytecode::new(bytecode)?;

        let info = vk::ShaderModuleCreateInfo::builder()
            .code(bytecode.code())
            .code_size(bytecode.code_size());

        Ok(device.logical().create_shader_module(&info, None)?)
    }

    pub unsafe fn bind(&self, command_buffer: &CommandBuffer) {
        self.device.logical().cmd_bind_pipeline(command_buffer.handle(), vk::PipelineBindPoint::GRAPHICS, self.handle);
    }
}
