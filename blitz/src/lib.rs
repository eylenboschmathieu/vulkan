#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod context;
mod instance;
mod device;
mod sync;
mod queues;
mod swapchain;
mod pipeline;
mod commands;
mod resources;
mod container;
mod mesh;

use std::{sync::atomic::{AtomicBool, Ordering}, time::Instant};

use log::*;
use anyhow::{anyhow, Result};
use winit::window::Window;
use vulkanalia::{
    vk::{self, DeviceV1_0, Handle, HasBuilder, KhrSwapchainExtensionDeviceCommands}
};

pub use crate::{
    container::*,
    mesh::Mesh,
    resources::{
        image::TextureId,
        buffers::{
            index_buffer::IndexBufferId,
            vertex_buffer::VertexBufferId,
            uniform_buffer::UniformBufferId,
        },
        material::{MaterialDef, MaterialId},
        vertices::*,
    },
};

use crate::{
    context::Context, device::Device,
    pipeline::{
        pipeline::Pipeline,
        renderpass::Renderpass,
        descriptors::{
            DescriptorPool,
            DescriptorSetUpdateInfo,
        }
    },
    resources::image::DepthBuffer,
    swapchain::Swapchain,
    sync::{
        FRAMES_IN_FLIGHT,
        Synchronization,
    }
};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct DrawCall {
    pub mesh: Mesh,
    pub material: MaterialId,
}

#[derive(Debug)]
pub struct Blitz {
    context: Context,
    swapchain: Swapchain,
    sync: Synchronization,
    depth_buffer: DepthBuffer,
    renderpass: Renderpass,
    descriptor_pool: DescriptorPool,  // Needs moving to resources
    draw_queue: Vec<DrawCall>,
}

impl Blitz {
    /// Used to upload vertex and index data to the graphics card, as well as textures.
    pub unsafe fn new_container(&self) -> Container<Loading> {
        container::Container::new(&self.context.device).unwrap()
    }

    pub unsafe fn process_container(&mut self, container: Container<Loading>) -> Result<Container<Resolved>> {
        let mut container = container.transition::<Transfer>();

        container.process(
            &self.context.device,
            &self.context.command_manager,
            &mut self.context.resource_manager,
            &self.context.queue_manager,
        )?;
        
        let container = container.transition::<Resolved>();
        container.destroy(&self.context.device);
        Ok(container)
    }

    pub unsafe fn new_material(&mut self, material_def: MaterialDef) -> Result<MaterialId> {
        let descriptor_set_layout = self.context.resource_manager.descriptor_set_layouts.alloc(
            &self.context.device,
            material_def.uniforms,
            material_def.textures,
        );
        
        self.descriptor_pool.allocate_descriptor_sets(&self.context.device, &descriptor_set_layout, FRAMES_IN_FLIGHT)?;
        
        let pipeline = Pipeline::new(
            &self.context.device,
            &self.renderpass,
            self.swapchain.extent(),
            self.swapchain.format(),
            &[descriptor_set_layout.handle()],
            &material_def
        );

        self.context.resource_manager.materials.alloc(
            &self.context.device,
            pipeline,
        )
    }

    pub unsafe fn new_uniform_buffers(&mut self) -> Vec<UniformBufferId> {
        (0..FRAMES_IN_FLIGHT)
            .map(|_| {
                self.context.resource_manager.uniform_buffer.alloc().unwrap()
            })
            .collect()
    }

    pub unsafe fn bind_material(&self, id: MaterialId) {
        let command_buffer = &self.context.command_manager.graphics()[self.sync.frame];
        let pipeline = &self.context.resource_manager.materials[id].pipeline;
        self.context.resource_manager.materials.bind(&self.context.device, command_buffer, id);
        self.descriptor_pool.bind(&self.context.device, command_buffer, pipeline, self.sync.frame);
    }

    pub unsafe fn update_uniform_buffers(&self, uniform_buffers: &[UniformBufferId], delta: Instant) -> Result<()> {
        self.context.resource_manager.uniform_buffer.update(
            &self.context.device,
            uniform_buffers[self.sync.frame],  // Get the uniform buffer id associated with this frame
            &delta,
            self.swapchain.extent()
        )?;

        Ok(())
    }

    pub unsafe fn update_descriptor_sets(&mut self, texture_id: TextureId) {
        let data = self.context.resource_manager.uniform_buffer.get_data();
        let descriptor_set_update_info = DescriptorSetUpdateInfo {
            buffer: self.context.resource_manager.uniform_buffer.handle(),
            uniforms: data
        };
        self.descriptor_pool.update(&self.context.device, descriptor_set_update_info , &self.context.resource_manager.textures[texture_id]);
    }

    pub unsafe fn start_recording(&mut self) -> Result<()> {
        let command_buffer = &self.context.command_manager.graphics()[self.sync.frame];
        command_buffer.begin_recording(
            &self.context.device,
            self.swapchain.extent(),
            &self.renderpass,
            self.swapchain[self.sync.image].framebuffer())
    }

    pub unsafe fn end_recording(&mut self) -> Result<()> {
        self.flush_draw();

        let command_buffer = &self.context.command_manager.graphics()[self.sync.frame];
        command_buffer.end_recording(&self.context.device, &self.renderpass)
    }

    /// Returns Ok(true) if render started successfully, Ok(false) if not (swapchain out of date)
    pub unsafe fn start_render(&mut self, window: &Window) -> Result<bool> {
        // Wait until a frame is available for rendering.

        self.context.device.logical().wait_for_fences(&[self.sync.in_flight_fence()], true, u64::MAX)?;

        // Get the next swapchain image index

        let result = self.context.device.logical()
            .acquire_next_image_khr(self.swapchain.handle(), 
            u64::MAX, 
            self.sync.image_available_semaphore(), 
            vk::Fence::null());

        self.sync.image = match result {
            Ok((image_index, _)) => image_index as usize,
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                self.rebuild_swapchain(window)?;
                return Ok(false);
            },
            Err(e) => return Err(anyhow!(e)),
        };

        // Check if this image is already being rendered to. If so, wait until it is finished.

        if !self.sync.images_in_flight_fence().is_null() {
            self.context.device.logical().wait_for_fences(&[self.sync.images_in_flight_fence()], true, u64::MAX)?;
        }
        self.sync.update_image_in_flight_fence();  // Link the swapchain image to the current frame_in_flight fence

        // Start recording
        // pipeline and descriptor binding currently hardcoded.

        self.start_recording()?;

        Ok(true)
    }

    pub unsafe fn end_render(&mut self, window: &Window) -> Result<()> {
        // Stop recording

        self.end_recording()?;

        // Update uniform buffers

        // ... moved to blitz::update_uniform_buffers()

        // Submit

        let wait_semaphores = &[self.sync.image_available_semaphore()];
        let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = &[self.context.command_manager.graphics()[self.sync.frame as usize].handle()];
        let signal_semaphores = &[self.sync.render_finished_semaphore()];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        self.context.device.logical().reset_fences(&[self.sync.in_flight_fence()])?; // Render was completed for this frame, reset the fence.

        self.context.queue_manager.graphics().submit(
            &self.context.device,
            &[submit_info.build()],
            self.sync.in_flight_fence()
        ).expect("Failed to submit command buffer.");

        // Present

        let swapchains = &[self.swapchain.handle()];
        let image_indices = &[self.sync.image as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        if self.context.queue_manager.present().submit(&self.context.device, &present_info)? {
            self.rebuild_swapchain(window)?;
        };

        self.sync.frame = (self.sync.frame + 1) % FRAMES_IN_FLIGHT;

        Ok(())
    }

    /// Batches draw calls
    pub unsafe fn draw(&mut self, mesh: Mesh, material: MaterialId) {
        self.draw_queue.push(DrawCall { mesh, material });
    }

    /// Record all batched and reordered draw calls by material
    pub unsafe fn flush_draw(&mut self) {
        let command_buffer = &self.context.command_manager.graphics()[self.sync.frame];
        self.draw_queue.sort_by_key(|d| d.material);

        let mut current_material = None;

        for draw in &self.draw_queue {
            if current_material!= Some(draw.material) {
                self.bind_material(draw.material);
                current_material = Some(draw.material);
            }

            self.context.resource_manager.vertex_buffer.bind(&self.context.device, command_buffer, draw.mesh.vertices);
            self.context.resource_manager.index_buffer.bind(&self.context.device, command_buffer, draw.mesh.indices);
            self.context.resource_manager.index_buffer.draw(&self.context.device, command_buffer, draw.mesh.indices, 0);
        }
        self.draw_queue.clear();
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.context.device.logical().device_wait_idle().unwrap();
        self.renderpass.destroy(&self.context.device);
        self.descriptor_pool.destroy(&self.context.device);
        self.sync.destroy(&self.context.device);
        self.depth_buffer.destroy(&self.context.device);
        self.swapchain.destroy(&self.context.device);
        self.context.destroy();
    }

    unsafe fn rebuild_swapchain(&mut self, window: &Window) -> Result<()> {
        info!("Rebuilding swapchain");
        self.context.device.logical().device_wait_idle()?;

        // Clean up resources before rebuilding

        self.descriptor_pool.destroy(&self.context.device);
        self.context.command_manager.graphics_mut().free_buffers(&self.context.device);
        self.context.resource_manager.materials.clean(&self.context.device);
        self.renderpass.destroy(&self.context.device);
        self.depth_buffer.destroy(&self.context.device);

        // Recreate resources

        self.swapchain.rebuild(window, &self.context)?;
        self.depth_buffer = DepthBuffer::new(&self.context, self.swapchain.extent().width, self.swapchain.extent().height)?;
        self.renderpass.rebuild(&self.context, self.swapchain.format())?;
        self.context.resource_manager.materials.rebuild(&self.context.device, &self.renderpass, self.swapchain.extent(), self.swapchain.format());
        self.swapchain.create_framebuffers(&self.context.device, &self.renderpass, &self.depth_buffer);
        self.context.command_manager.graphics_mut().allocate_buffers(&self.context.device, self.swapchain.framebuffer_count());

        //let mut new_uniform_buffers = vec![];
        //self.uniform_buffers
        //    .iter()
        //    .for_each(|id| {
        //        self.context.resource_manager.uniform_buffer.free(*id);
        //        new_uniform_buffers.push(self.context.resource_manager.uniform_buffer.alloc().unwrap());
        //});
        //self.uniform_buffers = new_uniform_buffers;
        self.descriptor_pool = DescriptorPool::new(&self.context.device, FRAMES_IN_FLIGHT as u32)?;

        let descriptor_set_update_info = DescriptorSetUpdateInfo { 
            buffer: self.context.resource_manager.uniform_buffer.handle(),
            uniforms: self.context.resource_manager.uniform_buffer.get_data()
        };
        //self.descriptor_pool.update(&self.context.device, descriptor_set_update_info, &self.texture);

        Ok(())
    }
}

pub unsafe fn init(window: &Window) -> Result<Blitz> {
    // Enforce that Blitz can only be initialized once.
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(anyhow!("Vulkan already initialized"));
    }

    info!("Blitz::init");
    let mut context = Context::new(window)?;
    let mut swapchain = Swapchain::new(window, &context.instance, &context.device)?;
    context.command_manager.allocate_graphics_buffers(&context.device, FRAMES_IN_FLIGHT)?;

    let depth_buffer = DepthBuffer::new(&context, swapchain.extent().width, swapchain.extent().height)?;

    let renderpass= Renderpass::new(&context, swapchain.format())?;

    swapchain.create_framebuffers(&context.device, &renderpass, &depth_buffer);

    let sync = Synchronization::new(&context, &swapchain)?;

    let descriptor_pool = DescriptorPool::new(&context.device, FRAMES_IN_FLIGHT as u32)?;

    // Create

    Ok(Blitz {
        context,
        swapchain,
        sync,
        depth_buffer,
        descriptor_pool,
        renderpass,
        draw_queue: vec![],
    })
}
