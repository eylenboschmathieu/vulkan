#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{point3, vec3, Deg};
use anyhow::Result;
use std::time::Instant;
use log::*;
use winit::{event::{ElementState, MouseButton}, keyboard::KeyCode, window::{CursorGrabMode, Window}};
use blitz::*;

use crate::{camera::FpCamera, input::{Action, InputManager}, ui::Ui, world::World};

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
    input: InputManager,
    camera: FpCamera,
    world: World,
    ui: Ui,
    mouse_capture: bool,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        let input = InputManager::new();

        let world = World::new(&mut blitz)?;
        let ui = Ui::new(&blitz);

        let camera = FpCamera::new(point3(0.0, 2.0, 0.0), 0.0, 0.0);

        info!("+ App");
        Ok(Self { blitz, delta: Instant::now(), input, camera, world, ui, mouse_capture: true })
    }

    pub fn keyboard_update(&mut self, button: KeyCode, state: ElementState) {
        self.input.state.update_key(button, state);
    }

    pub fn mouse_button_update(&mut self, button: MouseButton, state: ElementState) {
        self.input.state.update_mouse(button, state);
    }

    pub fn mouse_cursor_update(&mut self, delta: (f64, f64)) {
        self.camera.mouse_move(delta.0 as f32, delta.1 as f32);
    }

    pub fn handle_input(&mut self, window: &Window, delta: f32) {
        if self.input.is_pressed(Action::AddBlock) {
            if let Some((pos, face)) = self.world.raycast(self.camera.eye, self.camera.forward(), 4.0) {
                let block = self.world.block_at(pos.x, pos.y, pos.z).unwrap();
                println!("Selected {:?} of {} block at {:?}", face, block, pos);
                self.world.add_block(pos, face);
            } else {
                println!("No block selected")
            }
        }

        if self.input.is_pressed(Action::RemoveBlock) {
            if let Some((pos, _face)) = self.world.raycast(self.camera.eye, self.camera.forward(), 4.0) {
                self.world.remove_block(pos);
            }
        }

        if self.input.is_pressed(Action::ToggleMouseLock) {
            if self.mouse_capture {
                window.set_cursor_grab(CursorGrabMode::None)
                    .expect("Failed to free cursor");
                window.set_cursor_visible(true);
                self.mouse_capture = false;
            } else {
                window.set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                    .expect("Failed to grab cursor");
                window.set_cursor_visible(false);
                self.mouse_capture = true;
            }
        }

        self.input.state.clear();
    }

    pub fn update_camera(&mut self, delta: f32) {
        let fwd   = self.camera.forward();
        let right = self.camera.right();
        let up    = vec3(0.0_f32, 1.0, 0.0);
        const SPEED: f32 = 6.0;

        if self.input.is_held(Action::MoveForward)  { self.camera.eye += fwd   * SPEED * delta; }
        if self.input.is_held(Action::MoveBackward) { self.camera.eye -= fwd   * SPEED * delta; }
        if self.input.is_held(Action::MoveLeft)     { self.camera.eye -= right * SPEED * delta; }
        if self.input.is_held(Action::MoveRight)    { self.camera.eye += right * SPEED * delta; }
        if self.input.is_held(Action::Jump)         { self.camera.eye += up    * SPEED * delta; }
        if self.input.is_held(Action::Crouch)       { self.camera.eye -= up    * SPEED * delta; }
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
