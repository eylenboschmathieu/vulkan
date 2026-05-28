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
use std::time::{Duration, Instant};
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
        descriptors::{DescriptorId, DescriptorSetUpdateInfo},
        PipelineDef,
    },
    resources::{
        image::{TextureId, TextureArrayId},
        buffers::{
            index_buffer::IndexAllocId,
            staging_buffer::StagingAllocId,
            vertex_buffer::VertexAllocId,
            uniform_buffer::{UniformAllocId, CameraUbo, LightingUbo},
        },
        vertices::*,
    },
};

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

pub const MAX_UI_QUADS:    usize = 1024;
pub const MAX_DEBUG_QUADS: usize = 256;

/// Vertex sub-buffer indices — pass to [`Container::alloc_mesh`].
pub const WORLD_VB: usize = 0;
pub const UI_VB:    usize = 1;
pub const DEBUG_VB: usize = 2;

// ── Render layers ─────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct ColorLayer {
    queue: Vec<(Mesh, cgmath::Matrix4<f32>)>,
}

impl ColorLayer {
    fn enqueue(&mut self, mesh: Mesh, transform: cgmath::Matrix4<f32>) {
        self.queue.push((mesh, transform));
    }

    unsafe fn flush(&mut self, cb: &commands::CommandBuffer, camera_set: vk::DescriptorSet) {
        if self.queue.is_empty() { return; }
        let p = globals::pipelines_mut();
        p.mesh_color.bind(cb);
        p.mesh_color.bind_sets(cb, &[camera_set], 0);
        for (mesh, transform) in &self.queue {
            p.mesh_color.push_constants(cb, transform);
            globals::vertex_buffer().bind(cb, mesh.vertices);
            globals::index_buffer().bind(cb, mesh.indices);
            globals::index_buffer().draw(cb, mesh.indices, 0);
        }
        self.queue.clear();
    }
}

#[derive(Debug, Default)]
struct ArrayLayer {
    queue: Vec<(Mesh, TextureArrayId)>,
}

impl ArrayLayer {
    fn enqueue(&mut self, mesh: Mesh, texture_array_id: TextureArrayId) {
        self.queue.push((mesh, texture_array_id));
    }

    unsafe fn flush(&mut self, cb: &commands::CommandBuffer, camera_set: vk::DescriptorSet, lighting_set: vk::DescriptorSet) {
        if self.queue.is_empty() { return; }
        let p = globals::pipelines_mut();
        p.chunk.bind(cb);
        p.chunk.bind_sets(cb, &[camera_set], 0);
        p.chunk.bind_sets(cb, &[lighting_set], 2);
        self.queue.sort_by_key(|(_, id)| *id);
        let mut current_array = None;
        for (mesh, texture_array_id) in &self.queue {
            if current_array != Some(*texture_array_id) {
                let descriptor_set = globals::textures().texture_array(*texture_array_id).descriptor_set;
                p.chunk.bind_sets(cb, &[descriptor_set], 1);
                current_array = Some(*texture_array_id);
            }
            globals::vertex_buffer().bind(cb, mesh.vertices);
            globals::index_buffer().bind(cb, mesh.indices);
            globals::index_buffer().draw(cb, mesh.indices, 0);
        }
        self.queue.clear();
    }
}

/// A 2D quad layer using the UI pipeline. Shared by the UI and debug overlays.
#[derive(Debug, Default)]
struct QuadLayer {
    mesh:  Mesh,
    queue: Vec<(usize, usize, TextureId)>,  // (first_quad, quad_count, texture_id)
}

impl QuadLayer {
    fn vertex_id(&self) -> VertexAllocId { self.mesh.vertices }

    fn enqueue(&mut self, first_quad: usize, quad_count: usize, texture_id: TextureId) {
        self.queue.push((first_quad, quad_count, texture_id));
    }

    unsafe fn flush(&mut self, cb: &commands::CommandBuffer, extent: vk::Extent2D) {
        if self.queue.is_empty() { return; }
        let w = extent.width as f32;
        let h = extent.height as f32;
        let ortho = cgmath::Matrix4::new(
            2.0/w,  0.0,    0.0, 0.0,
            0.0,    2.0/h,  0.0, 0.0,
            0.0,    0.0,    1.0, 0.0,
           -1.0,   -1.0,   0.0, 1.0,
        );
        let p = globals::pipelines_mut();
        p.ui.bind(cb);
        p.ui.push_constants(cb, &ortho);
        globals::vertex_buffer().bind(cb, self.mesh.vertices);
        globals::index_buffer().bind(cb, self.mesh.indices);
        for &(first_quad, quad_count, texture_id) in &self.queue {
            let descriptor_set = globals::textures()[texture_id].descriptor_set;
            p.ui.bind_sets(cb, &[descriptor_set], 0);
            globals::index_buffer().draw_range(cb, self.mesh.indices, (first_quad * 6) as u32, (quad_count * 6) as u32);
        }
        self.queue.clear();
    }
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
    color_layer: ColorLayer,
    array_layer: ArrayLayer,
    ui:          QuadLayer,
    debug:       QuadLayer,
    sky_color: [f32; 4],
    vsync: bool,
    vsync_dirty: bool,
    resize_dirty: bool,
    window_refresh_rate: u32,  // Physical refresh rate of the monitor
    fps_limit: Option<Duration>,
    frame_start: Instant,
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
        Ok(())
    }

    /// Get the swapchain presentation mode
     pub fn get_present_mode(&self) -> vk::PresentModeKHR {
        self.swapchain.present_mode()
     }

    /// Set the RGBA clear color used at the start of each render pass.
    pub unsafe fn set_sky_color(&mut self, color: [f32; 4]) {
        self.sky_color = color;
    }

    /// Signal that the window was resized. The swapchain will be rebuilt at the
    /// start of the next frame and `start_render` will return `Ok(false)`.
    pub fn request_resize(&mut self) {
        self.resize_dirty = true;
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        if self.vsync != vsync {
            self.vsync = vsync;
            self.vsync_dirty = true;
        }
        if vsync {
            self.fps_limit = None;
        }
    }

    /// Set a software frame rate cap. Returns `false` if the cap was ignored.
    ///
    /// With `FIFO` the driver already blocks at vblank, so a software cap is redundant.
    /// With `FIFO_LATEST_READY` the cap is always pinned to the monitor refresh rate —
    /// any value passed in is ignored, since frames rendered faster than that are discarded anyway.
    pub fn set_fps_limit(&mut self, fps: Option<u32>) -> bool {
        match self.swapchain.present_mode() {
            vk::PresentModeKHR::FIFO => false,
            vk::PresentModeKHR::FIFO_LATEST_READY => {
                self.fps_limit = Some(Duration::from_secs_f64(1.0 / self.window_refresh_rate as f64));
                false
            },
            _ => {
                self.fps_limit = fps.map(|f| Duration::from_secs_f64(1.0 / f.clamp(1, 999) as f64));
                true
            }
        }
    }

    /// Begin a frame.  Returns `Ok(false)` when the swapchain was out-of-date
    /// and had to be rebuilt; the caller should skip draw calls for that frame.
    /// Returns `Ok(true)` when the frame is ready to receive draw calls.
    pub unsafe fn start_render(&mut self, window: &Window) -> Result<bool> {
        if let Some(limit) = self.fps_limit {
            let elapsed = self.frame_start.elapsed();
            if elapsed < limit {
                std::thread::sleep(limit - elapsed);
            }
        }
        self.frame_start = Instant::now();

        if self.vsync_dirty || self.resize_dirty {
            self.vsync_dirty = false;
            self.resize_dirty = false;
            self.rebuild_swapchain(window)?;
            return Ok(false);
        }

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

    /// Enqueue a vertex-colored draw call with a per-object transform. No texture needed.
    pub unsafe fn draw_dynamic_color(&mut self, mesh: Mesh, transform: cgmath::Matrix4<f32>) {
        self.color_layer.enqueue(mesh, transform);
    }

    /// Enqueue a draw call that samples from a `sampler2DArray` (e.g. chunk meshes
    /// where each vertex carries a layer index into the tile atlas).
    pub unsafe fn draw_array(&mut self, mesh: Mesh, texture_array_id: TextureArrayId) {
        self.array_layer.enqueue(mesh, texture_array_id);
    }

    /// Enqueue a 2D UI draw call for a range of quads in the shared UI mesh.
    /// Drawn after all 3D geometry with depth testing off and alpha blending on.
    pub unsafe fn draw_ui_quads(&mut self, first_quad: usize, quad_count: usize, texture_id: TextureId) {
        self.ui.enqueue(first_quad, quad_count, texture_id);
    }

    /// Returns the vertex buffer ID for the pre-allocated UI quad mesh.
    pub fn ui_vertex_id(&self) -> VertexAllocId { self.ui.vertex_id() }

    /// Enqueue a debug overlay draw call. Uses the same UI pipeline, drawn on top of UI.
    pub unsafe fn draw_debug_quads(&mut self, first_quad: usize, quad_count: usize, texture_id: TextureId) {
        self.debug.enqueue(first_quad, quad_count, texture_id);
    }

    /// Returns the vertex buffer ID for the pre-allocated debug quad mesh.
    pub fn debug_vertex_id(&self) -> VertexAllocId { self.debug.vertex_id() }

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
        let extent       = self.swapchain.extent();

        self.color_layer.flush(cb, camera_set);
        self.array_layer.flush(cb, camera_set, lighting_set);
        self.ui.flush(cb, extent);
        self.debug.flush(cb, extent);
    }

    unsafe fn rebuild_swapchain(&mut self, window: &Window) -> Result<()> {
        info!("Rebuilding swapchain");
        globals::device().logical().device_wait_idle()?;
        globals::device_mut().refresh_swapchain_support(globals::instance())?;

        globals::commands_mut().graphics_mut().free_buffers();
        self.renderpass.destroy();
        self.depth_buffer.destroy();
        globals::pipelines_mut().destroy();

        self.swapchain.rebuild(window, self.vsync)?;
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

    let window_refresh_rate = window
        .current_monitor()
        .and_then(|m| m.refresh_rate_millihertz())
        .map(|mhz| mhz / 1000)
        .unwrap_or(60);

    let queues = queues::Queues::new()?;
    globals::init_queues(queues);

    let commands = commands::Commands::new(globals::instance())?;
    globals::init_commands(commands);

    let staging_buffer = resources::buffers::staging_buffer::StagingBuffer::new(&[1024 * 1024 * 16])?;
    globals::init_staging_buffer(staging_buffer);

    let index_buffer = resources::buffers::index_buffer::IndexBuffer::new(&[1024 * 512])?;
    globals::init_index_buffer(index_buffer);

    let vertex_buffer = resources::buffers::vertex_buffer::VertexBuffer::new(&[
        1024 * 1024 * 8,                          // WORLD_VB
        MAX_UI_QUADS    * 4 * 64,                 // UI_VB
        MAX_DEBUG_QUADS * 4 * 64,                 // DEBUG_VB
    ])?;
    globals::init_vertex_buffer(vertex_buffer);

    let uniform_buffer = resources::buffers::uniform_buffer::UniformBuffer::new(&[16])?;
    globals::init_uniform_buffer(uniform_buffer);

    let descriptor_pool = pipeline::descriptors::DescriptorPool::new(16)?;
    globals::init_descriptor_pool(descriptor_pool);

    let textures = resources::image::Textures::new()?;
    globals::init_textures(textures);

    let mut swapchain = Swapchain::new(window, false)?;
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

    let mut blitz = Blitz {
        _entry: entry,
        camera,
        lighting,
        swapchain,
        sync,
        depth_buffer,
        renderpass,
        color_layer: ColorLayer::default(),
        array_layer: ArrayLayer::default(),
        ui:          QuadLayer::default(),
        debug:       QuadLayer::default(),
        sky_color: [0.22, 0.48, 0.72, 1.0],
        vsync: false,
        vsync_dirty: false,
        resize_dirty: false,
        window_refresh_rate,
        fps_limit: None,
        frame_start: Instant::now(),
    };

    // Pre-allocate UI and debug quad meshes in one upload. Indices are baked once; vertices are updated each frame.
    let mut ui_mesh    = Mesh::default();
    let mut debug_mesh = Mesh::default();
    let zeroed = resources::vertices::VERTEX_2D_RGBA::new(
        cgmath::vec2(0.0, 0.0), cgmath::vec2(0.0, 0.0), cgmath::vec4(0.0, 0.0, 0.0, 0.0),
    );
    blitz.upload(|container| unsafe {
        let ui_indices: Vec<u16> = (0..MAX_UI_QUADS as u16)
            .flat_map(|q| { let b = q * 4; [b, b+1, b+2, b+2, b+3, b] })
            .collect();
        ui_mesh = container.alloc_mesh(UI_VB, &vec![zeroed; MAX_UI_QUADS * 4], &ui_indices);

        let debug_indices: Vec<u16> = (0..MAX_DEBUG_QUADS as u16)
            .flat_map(|q| { let b = q * 4; [b, b+1, b+2, b+2, b+3, b] })
            .collect();
        debug_mesh = container.alloc_mesh(DEBUG_VB, &vec![zeroed; MAX_DEBUG_QUADS * 4], &debug_indices);

        Ok(())
    })?;
    blitz.ui.mesh    = ui_mesh;
    blitz.debug.mesh = debug_mesh;

    blitz.set_fps_limit(Some(60));

    Ok(blitz)
}
