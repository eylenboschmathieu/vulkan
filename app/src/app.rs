#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{point3, vec2, vec3, Deg};
use anyhow::Result;
use std::{collections::HashSet, time::Instant};
use log::*;
use winit::{keyboard::KeyCode, event::MouseButton, window::Window};
use blitz::*;

use crate::{camera::FpCamera, world::World};

#[derive(Debug)]
struct DynamicObject {
    mesh: Mesh,
    texture_id: TextureId,
    angle: f32,
    pub speed: f32,
}

impl DynamicObject {
    pub fn new() -> Self {
        Self { mesh: Mesh::default(), texture_id: 0, angle: 0.0, speed: 0.0 }
    }

    pub unsafe fn alloc(&mut self, container: &mut Container, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) {
        self.mesh = container.alloc_mesh(vertices, indices);
    }

    pub unsafe fn free(&self, container: &Container) {
        container.free_mesh(self.mesh);
    }

    pub fn update(&mut self, dt: f32) -> cgmath::Matrix4<f32> {
        self.angle += dt * self.speed;
        if self.angle > 360.0 {
            self.angle -= 360.0;
        }
        cgmath::Matrix4::from_angle_z(Deg(self.angle))
    }

    pub unsafe fn draw_static(&self, blitz: &mut Blitz) {
        blitz.draw_static(self.mesh, self.texture_id);
    }

    pub unsafe fn draw_dynamic(&self, blitz: &mut Blitz, transform: cgmath::Matrix4<f32>) {
        blitz.draw_dynamic(self.mesh, self.texture_id, transform);
    }
}

#[derive(Debug)]
struct StaticObject {
    mesh: Mesh,
    texture_id: TextureId,
}

impl StaticObject {
    pub fn new() -> Self {
        Self { mesh: Mesh::default(), texture_id: 0 }
    }

    pub unsafe fn alloc(&mut self, container: &mut Container, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) {
        self.mesh = container.alloc_mesh(vertices, indices);
    }

    pub unsafe fn free(&self, container: &Container) {
        container.free_mesh(self.mesh);
    }

    pub unsafe fn draw_static(&self, blitz: &mut Blitz) {
        blitz.draw_static(self.mesh, self.texture_id);
    }
}

const HOTBAR_SLOTS: usize = 10;
const SLOT_SIZE: f32 = 48.0;
const SLOT_GAP: f32 = 4.0;
const SLOT_MARGIN_BOTTOM: f32 = 20.0;

// Our Vulkan app.
#[derive(Debug)]
pub struct App {
    delta: Instant,
    blitz: blitz::Blitz,
    camera: FpCamera,
    world: World,
    hotbar_size: (u32, u32),
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        let world = World::new(&mut blitz)?;

        let camera = FpCamera::new(point3(0.0, 2.0, 0.0), 0.0, 0.0);

        info!("+ App");
        Ok(Self { blitz, delta: Instant::now(), camera, world, hotbar_size: (0, 0) })
    }

    pub fn input(&mut self, keys: &HashSet<KeyCode>, mouse_pressed: &mut HashSet<MouseButton>, dt: f32) {
        self.camera.input(keys, dt);

        // Handle clicks
        if mouse_pressed.contains(&MouseButton::Left)  {
            if let Some((pos, face)) = self.world.raycast(self.camera.eye, self.camera.forward(), 4.0) {
                let block = self.world.block_at(pos.x, pos.y, pos.z).unwrap();
                println!("Selected {:?} of {} block at {:?}", face, block, pos);
                self.world.add_block(pos, face);
            } else {
                println!("No block selected")
            }
        }
        if mouse_pressed.contains(&MouseButton::Right) {
            if let Some((pos, _face)) = self.world.raycast(self.camera.eye, self.camera.forward(), 4.0) {
                self.world.remove_block(pos);
            }
        }

        mouse_pressed.clear();
    }

    fn hotbar_verts(sw: u32, sh: u32) -> Vec<Vertex_2D_Color> {
        let total_w = HOTBAR_SLOTS as f32 * SLOT_SIZE + (HOTBAR_SLOTS - 1) as f32 * SLOT_GAP;
        let x0 = (sw as f32 - total_w) / 2.0;
        let y0 = sh as f32 - SLOT_SIZE - SLOT_MARGIN_BOTTOM;
        let color = vec3(0.25, 0.25, 0.25);
        let mut verts = Vec::with_capacity(HOTBAR_SLOTS * 4);
        for i in 0..HOTBAR_SLOTS {
            let x = x0 + i as f32 * (SLOT_SIZE + SLOT_GAP);
            verts.push(Vertex_2D_Color::new(vec2(x,             y0),             color));
            verts.push(Vertex_2D_Color::new(vec2(x + SLOT_SIZE, y0),             color));
            verts.push(Vertex_2D_Color::new(vec2(x + SLOT_SIZE, y0 + SLOT_SIZE), color));
            verts.push(Vertex_2D_Color::new(vec2(x,             y0 + SLOT_SIZE), color));
        }
        verts
    }

    pub fn mouse_move(&mut self, dx: f32, dy: f32) {
        self.camera.mouse_move(dx, dy);
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        let dt = self.delta.elapsed().as_secs_f32();
        self.delta = Instant::now();

        self.world.update(dt);

        let size = window.inner_size();
        let aspect = size.width as f32 / size.height as f32;

        self.blitz.update_camera(self.camera.ubo(aspect));
        self.blitz.update_lighting(self.world.lighting_ubo());

        let t = self.world.lighting_ubo().sun_dir.y.max(0.0);
        let sky   = [0.22_f32, 0.48, 0.72, 1.0];
        let night = [0.01_f32, 0.01, 0.05, 1.0];
        let color = std::array::from_fn(|i| night[i] + (sky[i] - night[i]) * t);
        self.blitz.set_sky_color(color);

        let current_size = (size.width, size.height);
        let needs_upload = self.world.has_dirty_chunks() || current_size != self.hotbar_size;

        if needs_upload {
            let hotbar_verts = if current_size != self.hotbar_size {
                self.hotbar_size = current_size;
                Some(Self::hotbar_verts(size.width, size.height))
            } else {
                None
            };
            let ui_vid = self.blitz.ui_vertex_id();
            self.blitz.upload(|container| unsafe {
                self.world.flush_dirty(container);
                if let Some(verts) = &hotbar_verts {
                    container.stage_vertex_update(ui_vid, verts);
                }
                Ok(())
            })?;
        }

        if self.blitz.start_render(window)? {
            self.world.draw(&mut self.blitz, &self.camera)?;
            self.blitz.draw_ui_quads(0, HOTBAR_SLOTS);
            self.blitz.end_render(window)?;
        }

        Ok(())
    }

    /// Destroys our app.
    pub unsafe fn destroy(&mut self) {
        info!("~ App");
    }
}
