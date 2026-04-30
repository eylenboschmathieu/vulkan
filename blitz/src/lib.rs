#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod context;
mod instance;
mod device;
mod queues;
mod swapchain;
mod pipeline;
mod commands;
mod resources;
mod container;
mod mesh;

use std::{ops::Index, sync::atomic::{AtomicBool, Ordering}, time::Instant};

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
            vertex_buffer::{
                VertexBufferId,
                Vertex,
            }
        }
    },
};

use crate::{
    context::Context,
    device::Device,
    
    pipeline::{
        Pipeline,
        Renderpass,
        descriptors::{
            DescriptorPool,
            DescriptorSetLayout, DescriptorSetUpdateInfo
        },
    },
    resources::{
        image::{
            DepthBuffer,
        },
        buffers::{
            uniform_buffer::UniformBufferId,
        },
    },
    swapchain::Swapchain,
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
    pub image: usize,
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
        Ok(Self { frames, images_in_flight_fences, frame: 0, image: 0 })
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

    pub fn images_in_flight_fence(&self) -> vk::Fence {
        self.images_in_flight_fences[self.image]
    }
    
    pub unsafe fn update_image_in_flight_fence(&mut self) {
        self.images_in_flight_fences[self.image] = self.in_flight_fence();
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
    vertex_buffer: VertexBufferId,
    index_buffer: IndexBufferId,
    uniform_buffers: Vec<UniformBufferId>,
}

impl Blitz {
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

    pub unsafe fn update_descriptor_sets(&mut self, texture_id: TextureId) {
        let data = self.context.resource_manager.uniform_buffer.get_data();
        let descriptor_set_update_info = DescriptorSetUpdateInfo { buffer: self.context.resource_manager.uniform_buffer.handle(), uniforms: data };
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
        let command_buffer = &self.context.command_manager.graphics()[self.sync.frame];
        command_buffer.end_recording(&self.context.device, &self.renderpass)
    }

    pub unsafe fn start_render(&mut self, window: &Window) -> Result<()> {
        self.context.device.logical().wait_for_fences(&[self.sync.in_flight_fence()], true, u64::MAX)?;

        let result = self.context.device.logical()
            .acquire_next_image_khr(self.swapchain.handle(), 
            u64::MAX, 
            self.sync.image_available_semaphore(), 
            vk::Fence::null());

        self.sync.image = match result {
            Ok((image_index, _)) => image_index as usize,
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                self.rebuild_swapchain(window)?;
                return Ok(());
            },
            Err(e) => return Err(anyhow!(e)),
        };

        if !self.sync.images_in_flight_fence().is_null() {
            self.context.device.logical().wait_for_fences(&[self.sync.images_in_flight_fence()], true, u64::MAX)?;
        }
        self.sync.update_image_in_flight_fence();

        self.start_recording()
    }

    pub unsafe fn end_render(&mut self, window: &Window, delta: Instant) -> Result<()> {
        self.end_recording()?;
        self.context.resource_manager.uniform_buffer.update(&self.context.device, self.uniform_buffers[self.sync.frame], &delta, self.swapchain.extent())?;

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

        self.context.device.logical().reset_fences(&[self.sync.in_flight_fence()])?;

        self.context.queue_manager.graphics().submit(&self.context.device, &[submit_info.build()], self.sync.in_flight_fence()).expect("Failed to submit command buffer.");

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

    pub unsafe fn render_mesh(&self, mesh: Mesh) {
        let command_buffer = &self.context.command_manager.graphics()[self.sync.frame];

        self.pipeline.bind(&self.context.device, command_buffer);
        self.descriptor_pool.bind(&self.context.device, &command_buffer, &self.pipeline, self.sync.frame);
        self.context.resource_manager.vertex_buffer.bind(&self.context.device, command_buffer, mesh.vertices);
        self.context.resource_manager.index_buffer.bind(&self.context.device, command_buffer, mesh.indices);

        // let offset = self.context.resource_manager.vertex_buffer.alloc_info(mesh.vertices).offset;
        self.context.resource_manager.index_buffer.draw(&self.context.device, command_buffer, mesh.indices, 0);
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window, delta: Instant) -> Result<()> {
        self.context.device.logical().wait_for_fences(&[self.sync.in_flight_fence()], true, u64::MAX)?;

        let result = self.context.device.logical()
            .acquire_next_image_khr(self.swapchain.handle(), 
            u64::MAX, 
            self.sync.image_available_semaphore(), 
            vk::Fence::null());
        self.sync.image = match result {
            Ok((image_index, _)) => image_index as usize,
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                self.rebuild_swapchain(window)?;
                return Ok(());
            },
            Err(e) => return Err(anyhow!(e)),
        };

        if !self.sync.images_in_flight_fence().is_null() {
            self.context.device.logical().wait_for_fences(&[self.sync.images_in_flight_fence()], true, u64::MAX)?;
        }
        self.sync.update_image_in_flight_fence();
        self.context.resource_manager.uniform_buffer.update(&self.context.device, self.uniform_buffers[self.sync.frame], &delta, self.swapchain.extent())?;

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

        self.context.device.logical().reset_fences(&[self.sync.in_flight_fence()])?;

        self.context.queue_manager.graphics().submit(&self.context.device, &[submit_info.build()], self.sync.in_flight_fence()).expect("Failed to submit command buffer.");

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

        self.sync.frame = (self.sync.image + 1) % FRAMES_IN_FLIGHT;

        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.context.device.logical().device_wait_idle().unwrap();

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

        let mut new_uniform_buffers = vec![];
        self.uniform_buffers
            .iter()
            .for_each(|id| {
                self.context.resource_manager.uniform_buffer.free(*id);
                new_uniform_buffers.push(self.context.resource_manager.uniform_buffer.alloc().unwrap());
        });
        self.uniform_buffers = new_uniform_buffers;
        self.descriptor_pool = DescriptorPool::new(&self.context.device, FRAMES_IN_FLIGHT as u32)?;
        self.descriptor_pool.allocate_descriptor_sets(&self.context.device, &self.descriptor_set_layout, FRAMES_IN_FLIGHT)?;

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

    let sync = Synchronization::new(&context, &swapchain)?;

    let mut uniform_buffers = vec![];
    for _ in 0..FRAMES_IN_FLIGHT {
        uniform_buffers.push(context.resource_manager.uniform_buffer.alloc()?);
    }
    let mut descriptor_pool = DescriptorPool::new(&context.device, FRAMES_IN_FLIGHT as u32)?;
    descriptor_pool.allocate_descriptor_sets(&context.device, &descriptor_set_layout, FRAMES_IN_FLIGHT)?;

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
        vertex_buffer: usize::MAX,
        index_buffer: usize::MAX,
        uniform_buffers,
    })
}
