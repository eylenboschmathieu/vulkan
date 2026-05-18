#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{point3, Deg};
use anyhow::Result;
use std::{collections::HashSet, time::Instant};
use log::*;
use winit::{keyboard::KeyCode, event::MouseButton, window::Window};
use blitz::*;

use crate::{camera::FpCamera, ui::Ui, world::World};

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

// Our Vulkan app.
#[derive(Debug)]
pub struct App {
    delta: Instant,
    blitz: blitz::Blitz,
    camera: FpCamera,
    world: World,
    ui: Ui,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        let world = World::new(&mut blitz)?;
        let ui = Ui::new(&blitz);

        let camera = FpCamera::new(point3(0.0, 2.0, 0.0), 0.0, 0.0);

        info!("+ App");
        Ok(Self { blitz, delta: Instant::now(), camera, world, ui })
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

    pub fn mouse_move(&mut self, dx: f32, dy: f32) {
        self.camera.mouse_move(dx, dy);
    }

    pub unsafe fn update(&mut self, window: &Window) {
        let dt = self.delta.elapsed().as_secs_f32();
        self.delta = Instant::now();

        self.world.update(dt);

        let size = window.inner_size();
        let aspect = size.width as f32 / size.height as f32;
        self.blitz.update_camera(self.camera.ubo(aspect));
        self.blitz.update_lighting(self.world.lighting_ubo());
        self.blitz.set_sky_color(self.world.sky_color());
    }

    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        let size = window.inner_size();
        let current_size = (size.width, size.height);
        let needs_upload = self.world.has_dirty_chunks() || self.ui.is_dirty(current_size);

        if needs_upload {
            self.blitz.upload(|container| unsafe {
                self.world.flush_dirty(container);
                self.ui.flush(container, current_size);
                Ok(())
            })?;
        }

        if self.blitz.start_render(window)? {
            self.world.draw(&mut self.blitz, &self.camera)?;
            self.ui.draw(&mut self.blitz);
            self.blitz.end_render(window)?;
        }

        Ok(())
    }

    /// Destroys our app.
    pub unsafe fn destroy(&mut self) {
        info!("~ App");
    }
}
