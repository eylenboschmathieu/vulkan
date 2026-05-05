#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{vec2, vec3, point3, Deg};
use anyhow::Result;
use std::{collections::HashSet, time::Instant};
use log::*;
use winit::{keyboard::KeyCode, window::Window};
use blitz::*;

use crate::camera::FpCamera;

pub const VERTICES: [blitz::Vertex_3D_Color_Texture; 8] = [
    Vertex_3D_Color_Texture::new(vec3(-0.5, -0.5, 0.0), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, -0.5, 0.0), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, 0.5, 0.0), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, 0.5, 0.0), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, -0.5, -0.5), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, -0.5, -0.5), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, 0.5, -0.5), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, 0.5, -0.5), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
];

pub const VERTICES2: [blitz::Vertex_3D_Color_Texture; 8] = [
    Vertex_3D_Color_Texture::new(vec3(-0.5, -0.5, -1.0), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, -0.5, -1.0), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, 0.5, -1.0), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, 0.5, -1.0), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-2.0, -2.0, -1.5), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(2.0, -2.0, -1.5), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(2.0, 2.0, -1.5), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-2.0, 2.0, -1.5), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
];

pub const INDICES: &[u16] = &[
    0, 1, 2, 2, 3, 0,
    4, 5, 6, 6, 7, 4,
];

pub const GROUND_VERTICES: [blitz::Vertex_3D_Color_Texture; 4] = [
    Vertex_3D_Color_Texture::new(vec3(-5.0, -5.0, -2.0), vec3(1.0, 1.0, 1.0), vec2(5.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3( 5.0, -5.0, -2.0), vec3(1.0, 1.0, 1.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3( 5.0,  5.0, -2.0), vec3(1.0, 1.0, 1.0), vec2(0.0, 5.0)),
    Vertex_3D_Color_Texture::new(vec3(-5.0,  5.0, -2.0), vec3(1.0, 1.0, 1.0), vec2(5.0, 5.0)),
];

pub const GROUND_INDICES: &[u16] = &[0, 1, 2, 2, 3, 0];

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

    pub unsafe fn upload(&mut self, container: &mut Container, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) {
        self.mesh = container.load_mesh(vertices, indices);
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

    pub unsafe fn upload(&mut self, container: &mut Container, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) {
        self.mesh = container.load_mesh(vertices, indices);
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        blitz.draw_static(self.mesh, self.texture_id);
    }
}

// Our Vulkan app.
#[derive(Debug)]
pub struct App {
    blitz: blitz::Blitz,
    delta: Instant,
    o: DynamicObject,
    o2: DynamicObject,
    ground: StaticObject,
    camera: FpCamera,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        let mut o = DynamicObject::new();
        let mut o2 = DynamicObject::new();
        let mut ground = StaticObject::new();
        let mut texture_id: TextureId = 0;
        let mut grass_id: TextureId = 0;

        blitz.upload(|container| unsafe {
            texture_id = container.load_texture("/home/krozu/Documents/Code/Rust/vulkan/app/img/image.png")?;
            grass_id = container.load_texture("/home/krozu/Documents/Code/Rust/vulkan/app/img/grass_256x256.png")?;
            o.upload(container, &VERTICES, &INDICES);
            o2.upload(container, &VERTICES2, &INDICES);
            ground.upload(container, &GROUND_VERTICES, GROUND_INDICES);
            Ok(())
        })?;

        o.texture_id = texture_id;
        o2.texture_id = texture_id;
        ground.texture_id = grass_id;
        o.speed = 20.0;
        o2.speed = 10.0;

        let camera = FpCamera::new(point3(3.0, 3.0, 3.0), 225.0, -35.0);

        info!("+ App");
        Ok(Self { blitz, delta: Instant::now(), o, o2, ground, camera })
    }

    pub fn input(&mut self, keys: &HashSet<KeyCode>, dt: f32) {
        self.camera.input(keys, dt);
    }

    pub fn mouse_move(&mut self, dx: f32, dy: f32) {
        self.camera.mouse_move(dx, dy);
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        let dt = self.delta.elapsed().as_secs_f32();
        self.delta = Instant::now();

        let size = window.inner_size();
        let aspect = size.width as f32 / size.height as f32;
        self.blitz.update_camera(self.camera.ubo(aspect));

        if self.blitz.start_render(window)? {
            self.ground.draw(&mut self.blitz);

            let transform = self.o.update(dt);
            self.o.draw_dynamic(&mut self.blitz, transform);

            let transform = self.o2.update(dt);
            self.o2.draw_dynamic(&mut self.blitz, transform);

            self.blitz.end_render(window)?;
        }

        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.blitz.destroy();
        info!("~ App");
    }
}
