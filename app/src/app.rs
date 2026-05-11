#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{point3, Deg};
use anyhow::Result;
use std::{collections::HashSet, time::Instant};
use log::*;
use winit::{keyboard::KeyCode, event::MouseButton, window::Window};
use blitz::*;

use crate::{camera::FpCamera, sun::Sun, world::World};

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
    texture_array_id: blitz::TextureArrayId,
    sun: Sun,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        let mut world = World::new();

        let mut texture_array_id: blitz::TextureArrayId = 0;
        let mut sun = Sun::new();

        blitz.upload(|container| unsafe {
            texture_array_id = container.alloc_texture_array(&[
                "app/img/tiles/grass.png",
                "app/img/tiles/grass_side.png",
                "app/img/tiles/dirt.png",
                "app/img/tiles/cobble.png",
            ])?;
            world.alloc(container)?;
            sun.alloc(container)?;

            Ok(())
        })?;

        let camera = FpCamera::new(point3(0.0, -60.0, 20.0), 90.0, -20.0);

        info!("+ App");
        Ok(Self { blitz, delta: Instant::now(), camera, world, texture_array_id, sun })
    }

    pub fn input(&mut self, keys: &HashSet<KeyCode>, mouse: &HashSet<MouseButton>, dt: f32) {
        self.camera.input(keys, dt);

        // Handle clicks
        if mouse.contains(&MouseButton::Left)  { println!("LeftClick"); }
        if mouse.contains(&MouseButton::Right) { println!("RightClick"); }
    }

    pub fn mouse_move(&mut self, dx: f32, dy: f32) {
        self.camera.mouse_move(dx, dy);
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        let dt = self.delta.elapsed().as_secs_f32();
        self.delta = Instant::now();

        self.sun.update(dt);

        let size = window.inner_size();
        let aspect = size.width as f32 / size.height as f32;

        self.blitz.update_camera(self.camera.ubo(aspect));
        self.blitz.update_lighting(blitz::LightingUbo { sun_dir: self.sun.sun_dir() });

        let t = (self.sun.sun_dir().z).max(0.0);
        let sky   = [0.22_f32, 0.48, 0.72, 1.0];
        let night = [0.01_f32, 0.01, 0.05, 1.0];
        let color = std::array::from_fn(|i| night[i] + (sky[i] - night[i]) * t);
        self.blitz.set_sky_color(color);

        if self.blitz.start_render(window)? {
            self.world.draw(&mut self.blitz, self.texture_array_id)?;
            self.sun.draw(&mut self.blitz, &self.camera);
            self.blitz.end_render(window)?;
        }

        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        info!("~ App");
    }
}
