#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::rc::Rc;

use cgmath::point3;
use anyhow::Result;
use log::*;
use winit::{event::ElementState, window::Window};
use blitz::*;

use crate::{camera::FpCamera, debug::DebugInfo, font::FontManager, input::{Action, Input, InputManager}, ui::{Ui, UiAction}, world::World};

pub enum AppEvent {
    Exit,
}


// Our Vulkan app.
#[derive(Debug)]
pub struct App {
    blitz: blitz::Blitz,
    debug: DebugInfo,
    input: InputManager,
    camera: FpCamera,
    world: World,
    ui: Ui,
    pub fonts: FontManager,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;
        let fonts = FontManager::new(&mut blitz)?;

        let debug = DebugInfo::new(window, &blitz, Rc::clone(&fonts.debug_atlas));

        let input = InputManager::new(window);

        let world = World::new(&mut blitz)?;
        let ui = Ui::new(&window, &blitz, Rc::clone(&fonts.ui_atlas));

        let camera = FpCamera::new(point3(0.0, 2.0, 0.0), 0.0, 0.0);

        info!("+ App");
        Ok(Self { blitz, input, camera, world, ui, fonts, debug })
    }

    /// Signal that the window was resized so the swapchain is rebuilt next frame.
    pub fn request_resize(&mut self) {
        self.blitz.request_resize();
    }

    /// Update the state of keyboard or mouse buttons
    pub fn button_update<T: Into<Input>>(&mut self, button: T, state: ElementState) {
        self.input.button_update(button, state);
    }

    /// Apply raw mouse delta to the camera. Ignored while the menu is open.
    pub fn mouse_motion(&mut self, delta: (f32, f32)) {
        if !self.ui.menu_opened() {
            self.camera.mouse_move(delta.0, delta.1);
        }
    }

    /// Update the absolute cursor position (used for UI hit-testing).
    pub fn cursor_moved(&mut self, x: f32, y: f32) {
        self.input.cursor_update(x, y);
    }

    /// Process one tick of input. Returns `Some(AppEvent::Exit)` when the app should quit.
    /// Clears per-tick pressed/released state at the end.
    pub fn handle_input(&mut self, window: &Window) -> Option<AppEvent> {
        if self.input.is_pressed(Action::Quit) {
            return Some(AppEvent::Exit);
        }

        if self.input.is_pressed(Action::ToggleMenu) {
            self.ui.toggle_menu(window);
        }

        if self.input.is_pressed(Action::ToggleDebug) {
            self.debug.enabled = !self.debug.enabled;
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

    /// Advance simulation by `delta` seconds.
    pub unsafe fn update(&mut self, dt: f32) {
        if !self.ui.menu_opened() {
            self.camera.handle_input(&self.input, dt);
        }
        self.world.update(dt);
    }

    /// Push UBOs, upload any dirty GPU data, then record and submit a frame.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        let size = window.inner_size();
        let aspect = size.width as f32 / size.height as f32;

        self.blitz.update_camera(self.camera.ubo(aspect));
        self.blitz.update_lighting(self.world.lighting_ubo());
        self.blitz.set_sky_color(self.world.sky_color());

        self.blitz.upload(|container| unsafe {
            if self.world.has_dirty_chunks() {
                self.world.flush_dirty(container);
            }
            if self.ui.dirty {
                self.ui.flush_all(container, (size.width as f32, size.height as f32));
            } else if self.ui.has_dirty_nodes() {
                self.ui.flush_dirty(container);
            }
            if self.debug.enabled {
                self.debug.flush(container, &self.camera, size.width as f32);
            }
            Ok(())
        })?;

        if self.blitz.start_render(window)? {
            self.world.draw(&mut self.blitz, &self.camera)?;
            self.ui.draw(&mut self.blitz);
            self.debug.draw(&mut self.blitz);
            self.blitz.end_render(window)?;
        } else {
            let window_area = window.inner_size();
            self.ui.generate_tree(window_area.width as f32, window_area.height as f32);
            self.ui.dirty = true;
        }

        self.debug.on_frame();
        Ok(())
    }
}
