#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{point3, Deg};
use anyhow::Result;
use log::*;
use winit::{event::ElementState, window::Window};
use blitz::*;

use crate::{camera::FpCamera, input::{Action, Input, InputManager}, ui::{Ui, UiAction}, world::World};

pub enum AppEvent {
    Exit,
}

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

    pub unsafe fn alloc(&mut self, container: &mut Container, vertices: &[VERTEX_3D_RGBA_TEXTURE], indices: &[u16]) {
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

    pub unsafe fn alloc(&mut self, container: &mut Container, vertices: &[VERTEX_3D_RGBA_TEXTURE], indices: &[u16]) {
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
    blitz: blitz::Blitz,
    input: InputManager,
    camera: FpCamera,
    world: World,
    ui: Ui,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        let input = InputManager::new(window);

        let world = World::new(&mut blitz)?;
        let ui = Ui::new(&window, &blitz);

        let camera = FpCamera::new(point3(0.0, 2.0, 0.0), 0.0, 0.0);

        info!("+ App");
        Ok(Self { blitz, input, camera, world, ui })
    }

    /// Update the state of keyboard or mouse buttons
    pub fn button_update<T: Into<Input>>(&mut self, button: T, state: ElementState) {
        self.input.button_update(button, state);
    }

    pub fn mouse_motion(&mut self, delta: (f32, f32)) {
        if !self.ui.menu_opened() {
            self.camera.mouse_move(delta.0, delta.1);
        }
    }

    pub fn cursor_moved(&mut self, x: f32, y: f32) {
        self.input.cursor_update(x, y);
    }

    pub fn handle_input(&mut self, window: &Window) -> Option<AppEvent> {
        if self.input.is_pressed(Action::Quit) {
            return Some(AppEvent::Exit);
        }

        if self.input.is_pressed(Action::ToggleMenu) {
            self.ui.toggle_menu(window);
        }

        if self.ui.menu_opened() {
            match self.ui.handle_input(&self.input) {
                Some(UiAction::CloseMenu) => self.ui.toggle_menu(window),
                Some(UiAction::ExitApp)   => return Some(AppEvent::Exit),
                _ => {}
            }
        } else {
            self.world.handle_input(&self.input, &self.camera);
        }
        
        self.input.state.clear();
        None
    }

    pub unsafe fn update(&mut self, window: &Window, delta: f32) {
        if !self.ui.menu_opened() {
            self.camera.handle_input(&self.input, delta);
        }
        self.world.update(delta);

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
}
