#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod context;
mod instance;
mod device;
mod queues;
mod transfer_manager;
mod swapchain;
mod pipeline;
mod commands;
mod buffers;
mod image;

use std::{ops::Index, sync::atomic::{AtomicBool, Ordering}, time::Instant};

use log::*;
use anyhow::{anyhow, Result};
use winit::window::Window;
use vulkanalia::{
    vk::{self, DeviceV1_0, Handle, HasBuilder, KhrSwapchainExtensionDeviceCommands}
};

use crate::{
    buffers::{
        buffer::{INDICES, VERTICES},
        index_buffer::IndexBuffer,
        staging_buffer::StagingBuffer,
        uniform_buffer::UniformBuffer,
        vertex_buffer::{Vertex, VertexBuffer}
    },
    context::Context,
    device::Device,
    image::{DepthBuffer, Texture},
    pipeline::{
        Pipeline, Renderpass, descriptors::{DescriptorPool, DescriptorSetLayout}
    }, swapchain::Swapchain
};

static INITIALIZED: AtomicBool = AtomicBool::new(false);
const FRAMES_IN_FLIGHT: usize = 2;

// Structure containing per frame objects
#[derive(Clone, Debug)]
struct FrameSync {
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphore: vk::Semaphore,
    in_flight_fence: vk::Fence,
}

// Helper class to deal with synchronization
#[derive(Clone, Debug)]
struct Synchronization {
    frames: Vec<FrameSync>,
    images_in_flight_fences: Vec<vk::Fence>,
    pub frame: usize,
}

impl Synchronization {
    pub unsafe fn new(context: &Context, swapchain: &Swapchain) -> Result<Self> {
        let swapchain_image_count = swapchain.framebuffer_count();
        let width = swapchain.extent().width;
        let height = swapchain.extent().height;

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let fence_info = vk::FenceCreateInfo::builder()
            .flags(vk::FenceCreateFlags::SIGNALED);

        let mut frames = vec![];

        for _ in 0..FRAMES_IN_FLIGHT {
            frames.push(FrameSync {
                image_available_semaphore: context.device.logical().create_semaphore(&semaphore_info, None)?,
                render_finished_semaphore: context.device.logical().create_semaphore(&semaphore_info, None)?,
                in_flight_fence: context.device.logical().create_fence(&fence_info, None)?,
            });
        }

        let mut images_in_flight_fences = vec![];
        for _ in 0..swapchain_image_count {
            images_in_flight_fences.push(vk::Fence::null());
        }

        info!("+ Synchronization");
        Ok(Self { frames, images_in_flight_fences, frame: 0 })
    }
    
    pub unsafe fn destroy(&self, device: &Device) {
        for frame in &self.frames {
            device.logical().destroy_fence(frame.in_flight_fence, None);
            device.logical().destroy_semaphore(frame.image_available_semaphore, None);
            device.logical().destroy_semaphore(frame.render_finished_semaphore, None);
        }
        info!("~ Synchronization")
    }

    pub fn image_available_semaphore(&self) -> vk::Semaphore {
        self.frames[self.frame].image_available_semaphore
    }

    pub fn render_finished_semaphore(&self) -> vk::Semaphore {
        self.frames[self.frame].render_finished_semaphore
    }

    pub fn in_flight_fence(&self) -> vk::Fence {
        self.frames[self.frame].in_flight_fence
    }

    pub fn images_in_flight_fence(&self, image_index: usize) -> vk::Fence {
        self.images_in_flight_fences[image_index]
    }
    
    pub unsafe fn update_image_in_flight_fence(&mut self, image_index: usize) {
        self.images_in_flight_fences[image_index] = self.in_flight_fence();
    }
}

impl Index<usize> for Synchronization {
    type Output = FrameSync;

    fn index(&self, index: usize) -> &Self::Output  {
        &self.frames[index]
    }
}

#[derive(Debug)]
pub struct Blitz {
    context: Context,
    swapchain: Swapchain,
    sync: Synchronization,
    depth_buffer: DepthBuffer,
    renderpass: Renderpass,
    pipeline: Pipeline,
    descriptor_set_layout: DescriptorSetLayout,
    descriptor_pool: DescriptorPool,
    vertex_buffer: VertexBuffer,
    index_buffer: IndexBuffer,
    uniform_buffers: Vec<UniformBuffer>,
    texture: Texture,
}

pub trait Destroyable {
    unsafe fn destroy(&mut self, device: &Device);
}

impl Blitz {
    pub unsafe fn record(&self) -> Result<()> {
        for (image_index, command_buffer) in self.context.command_manager.graphics().into_iter().enumerate() {
            command_buffer.begin_recording(
                &self.context.device,
                self.swapchain.extent(),
                &self.renderpass,
                self.swapchain[image_index].framebuffer()
            )?;

            self.pipeline.bind(&self.context.device, command_buffer);
            self.vertex_buffer.bind(&self.context.device, command_buffer);
            self.index_buffer.bind(&self.context.device, command_buffer);
            self.descriptor_pool.bind(&self.context.device, &command_buffer, &self.pipeline, image_index);
            self.context.device.logical().cmd_draw_indexed(command_buffer.handle(), self.index_buffer.count(), 1, 0, 0, 0);
            // self.device.logical().cmd_draw(command_buffer.handle(), 3, 1, 0, 0);

            command_buffer.end_recording(&self.context.device, &self.renderpass)?;
        }
        Ok(())
    }

    pub unsafe fn upload(&self) -> Result<()> {
        // Make sure the staging buffer is big enough to hold our data
        let vertices_size = (size_of::<Vertex>() * VERTICES.len()) as u64;
        let indices_size = (size_of::<u16>() * INDICES.len()) as u64;

        let mut staging_buffer = StagingBuffer::new(&self.context, vertices_size + indices_size)?;

        let command_buffer = &self.context.command_manager.begin_one_time_submit(&self.context.device, vk::QueueFlags::TRANSFER)?;

        let ptr = staging_buffer.map(&self.context.device, vertices_size + indices_size, 0)?;

        staging_buffer.copy_to_staging(&self.context.device, &VERTICES, ptr)?;  // Copy vertices into staging buffer
        staging_buffer.copy_to_staging_at(&self.context.device, &INDICES, ptr, vertices_size as usize)?;  // Copy indices into staging buffer

        staging_buffer.unmap(&self.context.device);

        staging_buffer.copy_to_buffer(&self.context.device, command_buffer, &self.vertex_buffer)?;  // Copy data from staging buffer to vertex buffer
        staging_buffer.copy_to_buffer_at(&self.context.device, command_buffer, &self.index_buffer, vertices_size)?;  // Copy data from staging buffer to index buffer
        
        command_buffer.end_one_time_submit(&self.context.device, self.context.queue_manager.transfer(), None)?;
        staging_buffer.destroy(&self.context.device);
        Ok(())
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window, delta: Instant) -> Result<()> {
        self.context.device.logical().wait_for_fences(&[self.sync.in_flight_fence()], true, u64::MAX)?;

        let result = self.context.device.logical()
            .acquire_next_image_khr(self.swapchain.handle(), 
            u64::MAX, 
            self.sync.image_available_semaphore(), 
            vk::Fence::null());
        let image_index = match result {
            Ok((image_index, _)) => image_index as usize,
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                self.rebuild_swapchain(window)?;
                return Ok(());
            },
            Err(e) => return Err(anyhow!(e)),
        };

        if !self.sync.images_in_flight_fence(image_index).is_null() {
            self.context.device.logical().wait_for_fences(&[self.sync.images_in_flight_fence(image_index)], true, u64::MAX)?;
        }
        self.sync.update_image_in_flight_fence(image_index);
        self.uniform_buffers[image_index].update(&self.context.device, &delta, self.swapchain.extent())?;

        // Submit

        let wait_semaphores = &[self.sync.image_available_semaphore()];
        let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = &[self.context.command_manager.graphics()[image_index as usize].handle()];
        let signal_semaphores = &[self.sync.render_finished_semaphore()];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        self.context.device.logical().reset_fences(&[self.sync.in_flight_fence()])?;

        self.context.queue_manager.graphics().submit(&self.context.device, &[submit_info.build()], self.sync.in_flight_fence()).expect("Failed to submit command buffer.");

        // Present

        let swapchains = &[self.swapchain.handle()];
        let image_indices = &[image_index as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        if self.context.queue_manager.present().submit(&self.context.device, &present_info)? {
            self.rebuild_swapchain(window)?;
        };

        self.sync.frame = (image_index + 1) % FRAMES_IN_FLIGHT;

        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.context.device.logical().device_wait_idle().unwrap();

        self.texture.destroy(&self.context.device);
        for uniform_buffer in &mut self.uniform_buffers {
            uniform_buffer.destroy(&self.context.device);
        }
        self.index_buffer.destroy(&self.context.device);
        self.vertex_buffer.destroy(&self.context.device);
        self.pipeline.destroy(&self.context.device);  // Contains renderpass
        self.renderpass.destroy(&self.context.device);
        self.descriptor_set_layout.destroy(&self.context.device);
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
        for uniform_buffer in &mut self.uniform_buffers {
            uniform_buffer.destroy(&self.context.device);
        }
        self.context.command_manager.graphics_mut().free_buffers(&self.context.device);
        self.pipeline.clean(&self.context.device);
        self.renderpass.destroy(&self.context.device);
        self.depth_buffer.destroy(&self.context.device);

        // Recreate resources

        self.swapchain.rebuild(window, &self.context)?;
        self.depth_buffer = DepthBuffer::new(&self.context, self.swapchain.extent().width, self.swapchain.extent().height)?;
        self.renderpass.rebuild(&self.context, self.swapchain.format())?;
        self.pipeline.rebuild(&self.context, &self.renderpass, self.swapchain.extent(), self.swapchain.format())?;
        self.swapchain.create_framebuffers(&self.context.device, &self.renderpass, &self.depth_buffer);
        self.context.command_manager.graphics_mut().allocate_buffers(&self.context.device, self.swapchain.framebuffer_count());
        let mut uniform_buffers = vec![];
        for _ in 0..self.swapchain.framebuffer_count() {
            uniform_buffers.push(UniformBuffer::new(&self.context)?);
        }
        self.uniform_buffers = uniform_buffers;
        self.descriptor_pool = DescriptorPool::new(&self.context.device, self.swapchain.framebuffer_count() as u32)?;
        self.descriptor_pool.allocate_descriptor_sets(&self.context.device, &self.descriptor_set_layout, self.swapchain.framebuffer_count())?;
        self.descriptor_pool.update(&self.context.device, &self.uniform_buffers, &self.texture);

        // Re-record command buffers

        self.record()?;

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
    context.command_manager.allocate_graphics_buffers(&context.device, swapchain.framebuffer_count())?;

    let depth_buffer = DepthBuffer::new(&context, swapchain.extent().width, swapchain.extent().height)?;

    let descriptor_set_layout = DescriptorSetLayout::new(&context.device)?;

    let renderpass= Renderpass::new(&context, swapchain.format())?;

    let pipeline = Pipeline::new(
        &context,
        &renderpass,
        swapchain.extent(),
        swapchain.format(),
        &[descriptor_set_layout.handle()]
    )?;
    swapchain.create_framebuffers(&context.device, &renderpass, &depth_buffer);


    let sync = Synchronization::new(&context, &swapchain)?; // Need to reorder this since I need this to make changes to the renderpass
    let vertex_buffer = VertexBuffer::new(&context, &VERTICES)?;
    let index_buffer = IndexBuffer::new(&context, &INDICES)?;
    let mut uniform_buffers = vec![];
    for _ in 0..swapchain.framebuffer_count() {
        uniform_buffers.push(UniformBuffer::new(&context)?);
    }
    let texture = Texture::new(&context,"/home/krozu/Documents/Code/Rust/vulkan/app/img/image.png")?;

    let mut descriptor_pool = DescriptorPool::new(&context.device, swapchain.framebuffer_count() as u32)?;
    descriptor_pool.allocate_descriptor_sets(&context.device, &descriptor_set_layout, uniform_buffers.len())?;
    descriptor_pool.update(&context.device, &uniform_buffers, &texture);

    // Create

    Ok(Blitz {
        context,
        swapchain,
        sync,
        depth_buffer,
        descriptor_pool,
        descriptor_set_layout,
        renderpass,
        pipeline,
        vertex_buffer,
        index_buffer,
        uniform_buffers,
        texture,
    })
}
