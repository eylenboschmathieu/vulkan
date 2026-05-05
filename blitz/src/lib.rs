#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

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
mod camera;
mod globals;

use std::sync::atomic::{AtomicBool, Ordering};

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::{
    Entry,
    loader::{LIBRARY, LibloadingLoader},
    vk::{self, DeviceV1_0, Handle, HasBuilder, KhrSwapchainExtensionDeviceCommands},
};
use winit::window::Window;

pub use crate::{
    container::*,
    mesh::Mesh,
    pipeline::{
        descriptors::DescriptorSetUpdateInfo,
        PipelineDef,
    },
    resources::{
        image::TextureId,
        buffers::{
            index_buffer::IndexBufferId,
            vertex_buffer::VertexBufferId,
            uniform_buffer::{UniformBufferId, UniformBufferObject},
        },
        vertices::*,
    },
};

pub type MaterialId = usize;

use crate::{
    camera::Camera,
    device::Device,
    instance::Instance,
    pipeline::{
        pipeline::Pipeline,
        renderpass::Renderpass,
    },
    resources::image::DepthBuffer,
    swapchain::Swapchain,
    sync::{FRAMES_IN_FLIGHT, Synchronization},
};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct StaticDrawCall {
    mesh: Mesh,
    texture_id: TextureId,
}

#[derive(Debug)]
struct DynamicDrawCall {
    mesh: Mesh,
    texture_id: TextureId,
    transform: cgmath::Matrix4<f32>,
}

#[derive(Debug)]
pub struct Blitz {
    camera: Camera,
    swapchain: Swapchain,
    pipelines: Vec<Pipeline>,
    sync: Synchronization,
    depth_buffer: DepthBuffer,
    renderpass: Renderpass,
    static_queue: Vec<StaticDrawCall>,
    dynamic_queue: Vec<DynamicDrawCall>,
    _entry: Entry, // must be last: dropped last, keeping libvulkan loaded while all other fields clean up
}

impl Blitz {
    /// Uploads meshes and texture in a closure by way of:
    ///     container.upload_mesh(&vertices, &indices)
    ///     container.upload_texture(path)
    /// 
    /// Eagerly returns their id's
    /// 
    /// # Example
    /// ```rust
    /// impl Obj {    /// 
    ///    pub unsafe fn upload(&mut self, container: &mut Container, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) {
    ///        self.mesh = container.load_mesh(vertices, indices);
    ///    }                      // ^^^^^^^^^ Eagerly returns buffer id's
    /// }
    /// 
    /// let obj0 = Obj::new()
    /// let obj1 = Obj::new()
    /// 
    /// blitz.upload(|container| {  // Actual data uploads happens in blitz.upload
    ///     obj0.upload(&container, &VERTICES0, &INDICES0);
    ///     obj1.upload(&container, &VERTICES1, &INDICES1);
    /// })?;
    /// # };
    /// ```
    pub unsafe fn upload<F: FnOnce(&mut Container) -> Result<()>>(&mut self, f: F) -> Result<()> {
        let mut container = Container::new()?;
        f(&mut container)?;
        container.process()?;
        container.destroy();
        Ok(())
    }

    pub unsafe fn start_recording(&mut self) -> Result<()> {
        let command_buffer = &globals::command_manager().graphics()[self.sync.frame];
        command_buffer.begin_recording(
            self.swapchain.extent(),
            &self.renderpass,
            self.swapchain[self.sync.image].framebuffer(),
        )?;
        Ok(())
    }

    pub unsafe fn end_recording(&mut self) -> Result<()> {
        self.flush_draw();

        let command_buffer = &globals::command_manager().graphics()[self.sync.frame];
        command_buffer.end_recording(&self.renderpass)
    }

    pub unsafe fn start_render(&mut self, window: &Window) -> Result<bool> {
        globals::device().logical().wait_for_fences(&[self.sync.in_flight_fence()], true, u64::MAX)?;

        let result = globals::device().logical()
            .acquire_next_image_khr(
                self.swapchain.handle(),
                u64::MAX,
                self.sync.image_available_semaphore(),
                vk::Fence::null(),
            );

        self.sync.image = match result {
            Ok((image_index, _)) => image_index as usize,
            Err(vk::ErrorCode::OUT_OF_DATE_KHR) => {
                self.rebuild_swapchain(window)?;
                return Ok(false);
            },
            Err(e) => return Err(anyhow!(e)),
        };

        if !self.sync.images_in_flight_fence().is_null() {
            globals::device().logical().wait_for_fences(&[self.sync.images_in_flight_fence()], true, u64::MAX)?;
        }
        self.sync.update_image_in_flight_fence();

        self.start_recording()?;

        Ok(true)
    }

    pub unsafe fn end_render(&mut self, window: &Window) -> Result<()> {
        self.end_recording()?;

        let wait_semaphores = &[self.sync.image_available_semaphore()];
        let wait_stages = &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = &[globals::command_manager().graphics()[self.sync.frame].handle()];
        let signal_semaphores = &[self.sync.render_finished_semaphore()];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        globals::device().logical().reset_fences(&[self.sync.in_flight_fence()])?;

        globals::queue_manager().graphics().submit(
            &[submit_info.build()],
            self.sync.in_flight_fence(),
        ).expect("Failed to submit command buffer.");

        let swapchains = &[self.swapchain.handle()];
        let image_indices = &[self.sync.image as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        if globals::queue_manager().present().submit(&present_info)? {
            self.rebuild_swapchain(window)?;
        }

        self.sync.frame = (self.sync.frame + 1) % FRAMES_IN_FLIGHT;

        Ok(())
    }

    pub unsafe fn draw_static(&mut self, mesh: Mesh, texture_id: TextureId) {
        self.static_queue.push(StaticDrawCall { mesh, texture_id });
    }

    pub unsafe fn draw_dynamic(&mut self, mesh: Mesh, texture_id: TextureId, transform: cgmath::Matrix4<f32>) {
        self.dynamic_queue.push(DynamicDrawCall { mesh, texture_id, transform });
    }

    pub unsafe fn update_camera(&mut self, ubo: UniformBufferObject) {
        self.camera.update(self.sync.frame, ubo);
    }

    pub unsafe fn flush_draw(&mut self) {
        let command_buffer = globals::command_manager().graphics()[self.sync.frame];

        if !self.static_queue.is_empty() {
            self.pipelines[0].bind(&command_buffer);
            self.pipelines[0].bind_sets(&command_buffer, &[self.camera[self.sync.frame]], 0);
            self.static_queue.sort_by_key(|d| d.texture_id);
            let mut current_texture = None;
            for draw in &self.static_queue {
                if current_texture != Some(draw.texture_id) {
                    let descriptor_set = globals::textures()[draw.texture_id].descriptor_set;
                    self.pipelines[0].bind_sets(&command_buffer, &[descriptor_set], 1);
                    current_texture = Some(draw.texture_id);
                }
                globals::vertex_buffer().bind(&command_buffer, draw.mesh.vertices);
                globals::index_buffer().bind(&command_buffer, draw.mesh.indices);
                globals::index_buffer().draw(&command_buffer, draw.mesh.indices, 0);
            }
            self.static_queue.clear();
        }

        if !self.dynamic_queue.is_empty() {
            self.pipelines[1].bind(&command_buffer);
            self.pipelines[1].bind_sets(&command_buffer, &[self.camera[self.sync.frame]], 0);
            self.dynamic_queue.sort_by_key(|d| d.texture_id);
            let mut current_texture = None;
            for draw in &self.dynamic_queue {
                if current_texture != Some(draw.texture_id) {
                    let descriptor_set = globals::textures()[draw.texture_id].descriptor_set;
                    self.pipelines[1].bind_sets(&command_buffer, &[descriptor_set], 1);
                    current_texture = Some(draw.texture_id);
                }
                self.pipelines[1].push_constants(&command_buffer, &draw.transform);
                globals::vertex_buffer().bind(&command_buffer, draw.mesh.vertices);
                globals::index_buffer().bind(&command_buffer, draw.mesh.indices);
                globals::index_buffer().draw(&command_buffer, draw.mesh.indices, 0);
            }
            self.dynamic_queue.clear();
        }
    }

    pub unsafe fn destroy(&mut self) {
        globals::device().logical().device_wait_idle().unwrap();
        self.renderpass.destroy();
        self.sync.destroy();
        self.depth_buffer.destroy();
        for pipeline in &mut self.pipelines { pipeline.destroy(); }
        self.swapchain.destroy();
        self.camera.destroy();
        globals::command_manager_mut().destroy();
        globals::descriptor_pool_mut().destroy();
        globals::staging_buffer_mut().destroy();
        globals::index_buffer_mut().destroy();
        globals::vertex_buffer_mut().destroy();
        globals::uniform_buffer_mut().destroy();
        globals::textures_mut().destroy();
        globals::device().destroy();
        globals::instance().destroy();
    }

    unsafe fn rebuild_swapchain(&mut self, window: &Window) -> Result<()> {
        info!("Rebuilding swapchain");
        globals::device().logical().device_wait_idle()?;

        // Cleanup

        globals::command_manager_mut().graphics_mut().free_buffers();
        self.renderpass.destroy();
        self.depth_buffer.destroy();
        for pipeline in &mut self.pipelines { pipeline.destroy(); }

        // Rebuilding

        self.swapchain.rebuild(window)?;
        self.depth_buffer = DepthBuffer::new(self.swapchain.extent().width, self.swapchain.extent().height)?;
        self.renderpass.rebuild(self.swapchain.format())?;
        self.swapchain.create_framebuffers(&self.renderpass, &self.depth_buffer);
        self.build_pipelines()?;
        globals::command_manager_mut().allocate_graphics_buffers(self.swapchain.framebuffer_count())?;

        Ok(())
    }

    unsafe fn build_pipelines(&mut self) -> Result<()> {
        let layouts = &[self.camera.layout(), globals::textures().descriptor_set_layout];

        self.pipelines.push(Pipeline::new(
            &self.renderpass,
            self.swapchain.extent(),
            self.swapchain.format(),
            layouts,
            &PipelineDef {
                vertex_format: VertexFormat::Vertex3D_Color_Texture,
                vertex_shader: include_bytes!("../shaders/mesh_static.vert.spv"),
                fragment_shader: include_bytes!("../shaders/mesh_static.frag.spv"),
                push_constants: false,
            },
        )?);

        self.pipelines.push(Pipeline::new(
            &self.renderpass,
            self.swapchain.extent(),
            self.swapchain.format(),
            layouts,
            &PipelineDef {
                vertex_format: VertexFormat::Vertex3D_Color_Texture,
                vertex_shader: include_bytes!("../shaders/mesh_dynamic.vert.spv"),
                fragment_shader: include_bytes!("../shaders/mesh_dynamic.frag.spv"),
                push_constants: true,
            },
        )?);

        Ok(())
    }
}

pub unsafe fn init(window: &Window) -> Result<Blitz> {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(anyhow!("Vulkan already initialized"));
    }

    info!("Blitz::init");

    let loader = LibloadingLoader::new(LIBRARY)?;
    let entry = Entry::new(loader).map_err(|b| anyhow!("{b}"))?;

    let instance = Instance::new(window, &entry)?;
    globals::init_instance(instance);

    let device = Device::new(&entry, window, globals::instance())?;
    globals::init_device(device);

    let queue_manager = queues::QueueManager::new()?;
    globals::init_queue_manager(queue_manager);

    let command_manager = commands::CommandManager::new(globals::instance())?;
    globals::init_command_manager(command_manager);

    let staging_buffer = resources::buffers::staging_buffer::StagingBuffer::new(1024 * 1024 * 4)?;
    globals::init_staging_buffer(staging_buffer);

    let index_buffer = resources::buffers::index_buffer::IndexBuffer::new(1024)?;
    globals::init_index_buffer(index_buffer);

    let vertex_buffer = resources::buffers::vertex_buffer::VertexBuffer::new(1024 * 1024 * 4)?;
    globals::init_vertex_buffer(vertex_buffer);

    let uniform_buffer = resources::buffers::uniform_buffer::UniformBuffer::new(16)?;
    globals::init_uniform_buffer(uniform_buffer);

    let descriptor_pool = pipeline::descriptors::DescriptorPool::new(16)?;
    globals::init_descriptor_pool(descriptor_pool);

    let textures = resources::image::Textures::new()?;
    globals::init_textures(textures);

    let mut swapchain = Swapchain::new(window)?;
    globals::command_manager_mut().allocate_graphics_buffers(FRAMES_IN_FLIGHT)?;

    let camera = Camera::new(swapchain.extent())?;

    let depth_buffer = DepthBuffer::new(swapchain.extent().width, swapchain.extent().height)?;
    let renderpass = Renderpass::new(swapchain.format())?;
    swapchain.create_framebuffers(&renderpass, &depth_buffer);

    let sync = Synchronization::new(&swapchain)?;

    let mut blitz = Blitz {
        _entry: entry,
        camera,
        swapchain,
        pipelines: vec![],
        sync,
        depth_buffer,
        renderpass,
        static_queue: vec![],
        dynamic_queue: vec![],
    };

    blitz.build_pipelines()?;

    Ok(blitz)
}
