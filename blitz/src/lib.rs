//! Blitz — a minimal Vulkan renderer.
//!
//! # Architecture
//!
//! All Vulkan state lives in module-level globals ([`globals`]) so that subsystems
//! (buffers, textures, descriptor pool, etc.) can reach each other without threading
//! references through every call.  The public surface is intentionally small:
//!
//! - [`init`] — boots Vulkan and returns a [`Blitz`] handle.
//! - [`Blitz::upload`] — uploads GPU resources (meshes, textures) via a [`Container`].
//! - [`Blitz::start_render`] / [`Blitz::end_render`] — bracket each rendered frame.
//! - `draw_*` methods — enqueue draw calls between those two calls.
//!
//! # Descriptor set layout
//!
//! All pipelines share the same three-set layout:
//!
//! | Set | Contents |
//! |-----|----------|
//! | 0   | [`CameraUbo`] — model / view / projection matrices |
//! | 1   | Texture sampler (per draw call, sorted to minimise binds) |
//! | 2   | [`LightingUbo`] — sun direction and other scene lighting |

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
mod lighting;
mod globals;

use std::sync::atomic::{AtomicBool, Ordering};
use lighting::Lighting;

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
        image::{TextureId, TextureArrayId},
        buffers::{
            index_buffer::IndexBufferId,
            vertex_buffer::VertexBufferId,
            uniform_buffer::{UniformBufferId, CameraUbo, LightingUbo},
        },
        vertices::*,
    },
};

pub type MaterialId = usize;

use crate::{
    camera::Camera,
    device::Device,
    instance::Instance,
    pipeline::renderpass::Renderpass,
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
struct ArrayDrawCall {
    mesh: Mesh,
    texture_array_id: TextureArrayId,
}

/// Main renderer handle.
///
/// Owns all Vulkan frame-level state: swapchain, render pass, and
/// synchronisation primitives.  A single instance per process is enforced by
/// [`INITIALIZED`].  GPU resources are released automatically via `Drop`.
#[derive(Debug)]
pub struct Blitz {
    camera: Camera,
    lighting: Lighting,
    swapchain: Swapchain,
    sync: Synchronization,
    depth_buffer: DepthBuffer,
    renderpass: Renderpass,
    static_queue: Vec<StaticDrawCall>,
    dynamic_queue: Vec<DynamicDrawCall>,
    array_queue: Vec<ArrayDrawCall>,
    sky_color: [f32; 4],
    _entry: Entry, // must be last: dropped last, keeping libvulkan loaded while all other fields clean up
}

impl Blitz {
    /// Upload GPU resources inside a closure.
    ///
    /// The closure receives a [`Container`] whose `alloc_*` methods eagerly
    /// reserve GPU buffer / image slots and return live IDs.  The actual DMA
    /// transfers are batched and executed when the closure returns.
    ///
    /// ```rust,ignore
    /// blitz.upload(|c| unsafe {
    ///     my_mesh = c.alloc_mesh(&VERTICES, &INDICES);
    ///     my_tex  = c.alloc_texture("path/to/image.png")?;
    ///     Ok(())
    /// })?;
    /// ```
    pub unsafe fn upload<F: FnOnce(&mut Container) -> Result<()>>(&mut self, f: F) -> Result<()> {
        let mut container = Container::new()?;
        f(&mut container)?;
        container.process()?;
        container.destroy();
        Ok(())
    }

    /// Set the RGBA clear color used at the start of each render pass.
    pub unsafe fn set_sky_color(&mut self, color: [f32; 4]) {
        self.sky_color = color;
    }

    pub unsafe fn start_recording(&mut self) -> Result<()> {
        let command_buffer = &globals::commands().graphics()[self.sync.frame];
        self.sync.command_buffer = *command_buffer;

        command_buffer.begin_recording(
            self.swapchain.extent(),
            &self.renderpass,
            self.swapchain[self.sync.image].framebuffer(),
            self.sky_color,
        )?;
        Ok(())
    }

    pub unsafe fn end_recording(&mut self) -> Result<()> {
        self.flush_draw();
        self.sync.command_buffer.end_recording(&self.renderpass)
    }

    /// Begin a frame.  Returns `Ok(false)` when the swapchain was out-of-date
    /// and had to be rebuilt; the caller should skip draw calls for that frame.
    /// Returns `Ok(true)` when the frame is ready to receive draw calls.
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
        let command_buffers = &[self.sync.command_buffer.handle()];
        let signal_semaphores = &[self.sync.render_finished_semaphore()];
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(command_buffers)
            .signal_semaphores(signal_semaphores);

        globals::device().logical().reset_fences(&[self.sync.in_flight_fence()])?;

        globals::queues().graphics().submit(
            &[submit_info.build()],
            self.sync.in_flight_fence(),
        ).expect("Failed to submit command buffer.");

        let swapchains = &[self.swapchain.handle()];
        let image_indices = &[self.sync.image as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(signal_semaphores)
            .swapchains(swapchains)
            .image_indices(image_indices);

        if globals::queues().present().submit(&present_info)? {
            self.rebuild_swapchain(window)?;
        }

        self.sync.frame = (self.sync.frame + 1) % FRAMES_IN_FLIGHT;

        Ok(())
    }

    /// Enqueue a draw call for a mesh with a fixed world transform (identity).
    /// Batched draw calls are sorted by texture to minimise pipeline state changes.
    pub unsafe fn draw_static(&mut self, mesh: Mesh, texture_id: TextureId) {
        self.static_queue.push(StaticDrawCall { mesh, texture_id });
    }

    /// Enqueue a draw call with a per-object transform uploaded via push constants.
    pub unsafe fn draw_dynamic(&mut self, mesh: Mesh, texture_id: TextureId, transform: cgmath::Matrix4<f32>) {
        self.dynamic_queue.push(DynamicDrawCall { mesh, texture_id, transform });
    }

    /// Enqueue a draw call that samples from a `sampler2DArray` (e.g. chunk meshes
    /// where each vertex carries a layer index into the tile atlas).
    pub unsafe fn draw_array(&mut self, mesh: Mesh, texture_array_id: TextureArrayId) {
        self.array_queue.push(ArrayDrawCall { mesh, texture_array_id });
    }

    /// Write camera matrices for the current frame-in-flight slot.
    /// Call this every frame before [`Blitz::start_render`].
    pub unsafe fn update_camera(&mut self, ubo: CameraUbo) {
        self.camera.update(self.sync.frame, ubo);
    }

    /// Write lighting data for the current frame-in-flight slot.
    /// Call this every frame before [`Blitz::start_render`].
    pub unsafe fn update_lighting(&mut self, ubo: LightingUbo) {
        self.lighting.update(self.sync.frame, ubo);
    }

    pub unsafe fn flush_draw(&mut self) {
        let cb           = &self.sync.command_buffer;
        let camera_set   = self.camera[self.sync.frame];
        let lighting_set = self.lighting[self.sync.frame];
        let p            = globals::pipelines_mut();

        if !self.static_queue.is_empty() {
            p.mesh_static.bind(cb);
            p.mesh_static.bind_sets(cb, &[camera_set], 0);
            p.mesh_static.bind_sets(cb, &[lighting_set], 2);
            self.static_queue.sort_by_key(|d| d.texture_id);
            let mut current_texture = None;
            for draw in &self.static_queue {
                if current_texture != Some(draw.texture_id) {
                    let descriptor_set = globals::textures()[draw.texture_id].descriptor_set;
                    p.mesh_static.bind_sets(cb, &[descriptor_set], 1);
                    current_texture = Some(draw.texture_id);
                }
                globals::vertex_buffer().bind(cb, draw.mesh.vertices);
                globals::index_buffer().bind(cb, draw.mesh.indices);
                globals::index_buffer().draw(cb, draw.mesh.indices, 0);
            }
            self.static_queue.clear();
        }

        if !self.dynamic_queue.is_empty() {
            p.mesh_dynamic.bind(cb);
            p.mesh_dynamic.bind_sets(cb, &[camera_set], 0);
            p.mesh_dynamic.bind_sets(cb, &[lighting_set], 2);
            self.dynamic_queue.sort_by_key(|d| d.texture_id);
            let mut current_texture = None;
            for draw in &self.dynamic_queue {
                if current_texture != Some(draw.texture_id) {
                    let descriptor_set = globals::textures()[draw.texture_id].descriptor_set;
                    p.mesh_dynamic.bind_sets(cb, &[descriptor_set], 1);
                    current_texture = Some(draw.texture_id);
                }
                p.mesh_dynamic.push_constants(cb, &draw.transform);
                globals::vertex_buffer().bind(cb, draw.mesh.vertices);
                globals::index_buffer().bind(cb, draw.mesh.indices);
                globals::index_buffer().draw(cb, draw.mesh.indices, 0);
            }
            self.dynamic_queue.clear();
        }

        if !self.array_queue.is_empty() {
            p.chunk.bind(cb);
            p.chunk.bind_sets(cb, &[camera_set], 0);
            p.chunk.bind_sets(cb, &[lighting_set], 2);
            self.array_queue.sort_by_key(|d| d.texture_array_id);
            let mut current_array = None;
            for draw in &self.array_queue {
                if current_array != Some(draw.texture_array_id) {
                    let descriptor_set = globals::textures().texture_array(draw.texture_array_id).descriptor_set;
                    p.chunk.bind_sets(cb, &[descriptor_set], 1);
                    current_array = Some(draw.texture_array_id);
                }
                globals::vertex_buffer().bind(cb, draw.mesh.vertices);
                globals::index_buffer().bind(cb, draw.mesh.indices);
                globals::index_buffer().draw(cb, draw.mesh.indices, 0);
            }
            self.array_queue.clear();
        }
    }

    unsafe fn rebuild_swapchain(&mut self, window: &Window) -> Result<()> {
        info!("Rebuilding swapchain");
        globals::device().logical().device_wait_idle()?;
        globals::device_mut().refresh_swapchain_support(globals::instance())?;

        globals::commands_mut().graphics_mut().free_buffers();
        self.renderpass.destroy();
        self.depth_buffer.destroy();
        globals::pipelines_mut().destroy();

        self.swapchain.rebuild(window)?;
        self.depth_buffer = DepthBuffer::new(self.swapchain.extent().width, self.swapchain.extent().height)?;
        self.renderpass.rebuild(self.swapchain.format())?;
        self.swapchain.create_framebuffers(&self.renderpass, &self.depth_buffer);

        let layouts = &[self.camera.layout(), globals::textures().descriptor_set_layout, self.lighting.layout()];
        globals::init_pipelines(pipeline::Pipelines::new(&self.renderpass, self.swapchain.extent(), self.swapchain.format(), layouts)?);

        globals::commands_mut().allocate_graphics_buffers(self.swapchain.framebuffer_count())?;

        Ok(())
    }
}

impl Drop for Blitz {
    fn drop(&mut self) {
        unsafe {
            globals::device().logical().device_wait_idle().unwrap();
            self.sync.destroy();
            self.camera.destroy();
            self.lighting.destroy();
            self.renderpass.destroy();
            self.depth_buffer.destroy();
            globals::pipelines_mut().destroy();
            self.swapchain.destroy();
            globals::commands_mut().destroy();
            globals::descriptor_pool_mut().destroy();
            globals::staging_buffer_mut().destroy();
            globals::index_buffer_mut().destroy();
            globals::vertex_buffer_mut().destroy();
            globals::uniform_buffer_mut().destroy();
            globals::textures_mut().destroy();
            globals::device().destroy();
            globals::instance().destroy();
        }
    }
}

/// Initialise Vulkan and return a [`Blitz`] renderer.
///
/// Can only be called once per process; returns an error on subsequent calls.
/// Selects the first discrete GPU, enables validation layers in debug builds,
/// and allocates all shared GPU buffers (vertex, index, uniform, staging).
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

    let queues = queues::Queues::new()?;
    globals::init_queues(queues);

    let commands = commands::Commands::new(globals::instance())?;
    globals::init_commands(commands);

    let staging_buffer = resources::buffers::staging_buffer::StagingBuffer::new(1024 * 1024 * 16)?;
    globals::init_staging_buffer(staging_buffer);

    let index_buffer = resources::buffers::index_buffer::IndexBuffer::new(1024 * 512)?;
    globals::init_index_buffer(index_buffer);

    let vertex_buffer = resources::buffers::vertex_buffer::VertexBuffer::new(1024 * 1024 * 8)?;
    globals::init_vertex_buffer(vertex_buffer);

    let uniform_buffer = resources::buffers::uniform_buffer::UniformBuffer::new(16)?;
    globals::init_uniform_buffer(uniform_buffer);

    let descriptor_pool = pipeline::descriptors::DescriptorPool::new(16)?;
    globals::init_descriptor_pool(descriptor_pool);

    let textures = resources::image::Textures::new()?;
    globals::init_textures(textures);

    let mut swapchain = Swapchain::new(window)?;
    globals::commands_mut().allocate_graphics_buffers(FRAMES_IN_FLIGHT)?;

    let camera = Camera::new(swapchain.extent())?;
    let lighting = Lighting::new()?;

    let depth_buffer = DepthBuffer::new(swapchain.extent().width, swapchain.extent().height)?;
    let renderpass = Renderpass::new(swapchain.format())?;
    swapchain.create_framebuffers(&renderpass, &depth_buffer);

    let sync = Synchronization::new(&swapchain)?;

    let layouts = &[camera.layout(), globals::textures().descriptor_set_layout, lighting.layout()];
    let pipelines = pipeline::Pipelines::new(&renderpass, swapchain.extent(), swapchain.format(), layouts)?;
    globals::init_pipelines(pipelines);

    let blitz = Blitz {
        _entry: entry,
        camera,
        lighting,
        swapchain,
        sync,
        depth_buffer,
        renderpass,
        static_queue: vec![],
        dynamic_queue: vec![],
        array_queue: vec![],
        sky_color: [0.22, 0.48, 0.72, 1.0],
    };

    Ok(blitz)
}
