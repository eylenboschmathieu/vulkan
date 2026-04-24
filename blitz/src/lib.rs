#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod instance;
mod device;
mod queues;
mod swapchain;
mod pipeline;
mod renderpass;
mod commands;
mod buffers;

use std::{ops::Index, sync::atomic::{AtomicBool, Ordering}};

use thiserror::Error;
use log::*;
use anyhow::{anyhow, Result};
use winit::window::Window;
use vulkanalia::{
    Entry, loader::{LIBRARY, LibloadingLoader}, vk::{self, DeviceV1_0, Handle, HasBuilder, KhrSwapchainExtensionDeviceCommands}
};

type Mat4 = cgmath::Matrix4<f32>;

use crate::{
    buffers::{
        buffer::{INDICES, VERTICES, Vertex}, index_buffer::IndexBuffer, staging_buffer::StagingBuffer, vertex_buffer::VertexBuffer
    }, commands::CommandPool, device::Device, instance::Instance, pipeline::Pipeline, queues::QueuePool, swapchain::Swapchain
};

static INITIALIZED: AtomicBool = AtomicBool::new(false);
const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
const FRAMES_IN_FLIGHT: usize = 2;

#[derive(Debug, Error)]
#[error("Missing {0}.")]
pub struct SuitabilityError(pub &'static str);

#[derive(Clone, Debug)]
struct FrameSync {
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphore: vk::Semaphore,
    in_flight_fence: vk::Fence,
}

#[derive(Clone, Debug)]
struct Synchronization {
    frames: Vec<FrameSync>,
    images_in_flight_fences: Vec<vk::Fence>,
    pub frame: usize,
}

impl Synchronization {
    pub unsafe fn new(device: &Device, swapchain_image_count: usize) -> Result<Self> {
        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let fence_info = vk::FenceCreateInfo::builder()
            .flags(vk::FenceCreateFlags::SIGNALED);

        let mut frames = vec![];

        for _ in 0..FRAMES_IN_FLIGHT {
            frames.push(FrameSync {
                image_available_semaphore: device.logical().create_semaphore(&semaphore_info, None)?,
                render_finished_semaphore: device.logical().create_semaphore(&semaphore_info, None)?,
                in_flight_fence: device.logical().create_fence(&fence_info, None)?,
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
    entry: Entry,
    instance: Instance,
    device: Device,
    sync: Synchronization,
    queue_pool: QueuePool,
    swapchain: Swapchain,
    pipeline: Pipeline,
    command_pool: CommandPool,
    vertex_buffer: VertexBuffer,
    index_buffer: IndexBuffer,
}

pub trait Destroyable {
    unsafe fn destroy(&mut self, device: &Device);
}

impl Blitz {
    pub unsafe fn record(&self) -> Result<()> {
        for (i, command_buffer) in self.command_pool.graphics.as_ref().unwrap().into_iter().enumerate() {
            command_buffer.begin_recording(
                &self.device,
                self.swapchain.extent(),
                self.pipeline.renderpass(),
                self.swapchain[i].framebuffer()
            )?;

            self.pipeline.bind(command_buffer);
            self.vertex_buffer.bind(&self.device, command_buffer);
            self.index_buffer.bind(&self.device, command_buffer);
            self.device.logical().cmd_draw_indexed(command_buffer.handle(), self.index_buffer.count(), 1, 0, 0, 0);
            // self.device.logical().cmd_draw(command_buffer.handle(), 3, 1, 0, 0);

            command_buffer.end_recording(&self.device)?;
        }
        Ok(())
    }

    pub unsafe fn upload(&self) -> Result<()> {
        // Make sure the staging buffer is big enough to hold our data
        let vertices_size = (size_of::<Vertex>() * VERTICES.len()) as u32;
        let indices_size = (size_of::<u16>() * INDICES.len()) as u32;

        let mut staging_buffer = StagingBuffer::new(&self.instance, &self.device, vertices_size + indices_size).unwrap_or_else(|err| {
            panic!("Failed to create staging buffer");
        });

        let command_buffer = &self.command_pool.transfer.as_ref().unwrap()[0];
        command_buffer.begin_one_time_submit(&self.device)?;

        staging_buffer.copy_to_staging(&self.device, &VERTICES)?;  // Copy vertices into staging buffer
        staging_buffer.copy_to_staging_at(&self.device, &INDICES, vertices_size as u64)?;  // Copy indices into staging buffer

        staging_buffer.copy_to_buffer(&self.device, command_buffer, &self.vertex_buffer)?;  // Copy data from staging buffer to vertex buffer
        staging_buffer.copy_to_buffer_at(&self.device, command_buffer, &self.index_buffer, vertices_size as u64)?;  // Copy data from staging buffer to index buffer
        
        command_buffer.end_one_time_submit(&self.device)?;
        self.queue_pool.transfer().submit(&self.device, command_buffer)?;
        staging_buffer.destroy(&self.device);
        Ok(())
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        self.device.logical().wait_for_fences(&[self.sync.in_flight_fence()], true, u64::MAX)?;

        let result = self.device.logical()
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
            self.device.logical().wait_for_fences(&[self.sync.images_in_flight_fence(image_index)], true, u64::MAX)?;
        }
        self.sync.update_image_in_flight_fence(image_index);

        // Submit

        let wait_semaphores = &[self.sync.image_available_semaphore()];
        let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = &[self.command_pool.graphics.as_ref().unwrap()[image_index as usize].handle()];
        let signal_semaphores = &[self.sync.render_finished_semaphore()];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        self.device.logical().reset_fences(&[self.sync.in_flight_fence()])?;

        self.queue_pool.graphics().submit(&self.device, &[submit_info.build()], self.sync.in_flight_fence()).expect("Failed to submit command buffer.");

        // Present

        let swapchains = &[self.swapchain.handle()];
        let image_indices = &[image_index as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        if self.queue_pool.present().submit(&self.device, &present_info)? {
            self.rebuild_swapchain(window)?;
        };

        self.sync.frame = (image_index + 1) % FRAMES_IN_FLIGHT;

        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.device.logical().device_wait_idle().unwrap();

        self.index_buffer.destroy(&self.device);
        self.vertex_buffer.destroy(&self.device);
        self.command_pool.destroy();
        self.swapchain.destroy();
        self.pipeline.destroy();  // Contains renderpass
        self.sync.destroy(&self.device);
        self.device.destroy();
        self.instance.destroy();
    }

    unsafe fn rebuild_swapchain(&mut self, window: &Window) -> Result<()> {
        info!("Rebuilding swapchain");
        self.device.logical().device_wait_idle()?;

        // Clean up resources before rebuilding

        self.command_pool.graphics.as_mut().unwrap().free_buffers(&self.device);
        self.pipeline.clean();

        // Recreate resources

        self.swapchain.rebuild(window, &self.instance)?;
        self.pipeline.rebuild(self.swapchain.extent(), self.swapchain.format())?;
        self.swapchain.create_framebuffers(self.pipeline.renderpass());
        self.command_pool.graphics.as_mut().unwrap().allocate_buffers(&self.device, self.swapchain.framebuffer_count());

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

    info!("+ Blitz");
    let loader = LibloadingLoader::new(LIBRARY)?;
    let entry = Entry::new(loader).map_err(|b| anyhow!("{b}"))?;
    let instance = Instance::new(window, &entry).unwrap_or_else(|err| {
        panic!("Failed to create vulkan instance")
    });
    let device = Device::new(&entry, window, &instance).unwrap_or_else(|err| {
        panic!("Failed to create vulkan device.")
    });
    let queue_pool = QueuePool::new(&device).unwrap_or_else(|err| {
        panic!("Failed to create queues.")
    });
    let mut swapchain = Swapchain::new(window, &instance, &device).unwrap_or_else(|err| {
        panic!("Failed to create swapchain.")
    });
    let pipeline = Pipeline::new(&device, swapchain.extent(), swapchain.format()).unwrap_or_else(|err| {
        panic!("Failed to create pipeline.")
    });
    swapchain.create_framebuffers(pipeline.renderpass());
    let mut command_pool = CommandPool::new(&instance, &device).unwrap_or_else(|err| {
        panic!("Failed to create commands.");
    });
    command_pool.create_render_pool(&instance, swapchain.framebuffer_count())?;
    command_pool.create_transfer_pool(&instance)?;
    let sync = Synchronization::new(&device, swapchain.framebuffer_count()).unwrap_or_else(|err| {
        panic!("Failed to create synchronization.");
    });
    let size = (size_of::<Vertex>() * VERTICES.len()) as u32;
    let vertex_buffer = VertexBuffer::new(&instance, &device, size).unwrap_or_else(|err| {
        panic!("Failed to create vertex buffer");
    });
    let index_buffer = IndexBuffer::new(&instance, &device, INDICES).unwrap_or_else(|err| {
        panic!("Failed to create index buffer");
    });

    // Create

    Ok(Blitz {
        entry,
        instance,
        device,
        sync,
        queue_pool,
        swapchain,
        pipeline,
        command_pool,
        vertex_buffer,
        index_buffer,
    })
}

#[repr(C)]
#[derive(Debug)]
struct UniformBufferObject {
    model: Mat4,
    view: Mat4,
    proj: Mat4,
}