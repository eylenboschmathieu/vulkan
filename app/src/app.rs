#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::time::Instant;

use cgmath::point3;
use anyhow::Result;
use log::*;
use vulkanalia::vk::PresentModeKHR;
use winit::{dpi::LogicalPosition, event::ElementState, window::{CursorGrabMode, Window}};

use blitz::VertexAllocId;
use ui::{CursorRequest, MouseButton, UiEvent, UiInput, UiUpdate};

use crate::{camera::FpCamera, font::FontManager, input::{Action, Input, InputManager}, screens::{Screen, Screens}, world::World};

pub enum AppEvent {
    Exit,
}


// Our Vulkan app.
pub struct App {
    blitz: blitz::Blitz,
    input: InputManager,
    camera: FpCamera,
    world: World,
    ui: ui::Ui,
    ui_vertex_id: VertexAllocId,
    screens: Screens,

    debug_enabled: bool,
    fps: f32,
    frame_count: u32,
    fps_timer: Instant,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;
        let fonts = FontManager::new(&mut blitz)?;

        let input = InputManager::new(window);

        let world = World::new(&mut blitz)?;

        let area = window.inner_size();
        let screen_size = (area.width as f32, area.height as f32);
        let mut ui = ui::Ui::new(screen_size, fonts.ui_atlas);
        let screens = Screens::build(&mut ui, screen_size)?;
        let ui_vertex_id = blitz.ui_vertex_id();

        let camera = FpCamera::new(point3(0.0, 2.0, 0.0), 0.0, 0.0);

        info!("+ App");
        Ok(Self {
            blitz, input, camera, world, ui, ui_vertex_id, screens,
            debug_enabled: false,
            fps: 0.0,
            frame_count: 0,
            fps_timer: Instant::now(),
        })
    }

    /// Signal that the window was resized so the swapchain is rebuilt next frame.
    pub fn request_resize(&mut self) {
        self.blitz.request_resize();
    }

    /// Update the state of keyboard or mouse buttons
    pub fn button_update<T: Into<Input>>(&mut self, button: T, state: ElementState) {
        self.input.button_update(button, state);
    }

    /// Apply raw mouse delta to the camera. Ignored while a menu is open.
    pub fn mouse_motion(&mut self, delta: (f32, f32)) {
        if self.screens.current() == Screen::World {
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

        // Keep `pending` in sync with the live settings, except while System
        // Options is open and the user may have unsaved edits.
        if self.screens.current() != Screen::SystemOptions {
            self.screens.pending.set(crate::screens::PendingSettings {
                vsync: self.blitz.vsync(),
                fps_cap: self.blitz.fps_cap(),
            });
        }

        if self.input.is_pressed(Action::ToggleMenu) && self.screens.current() != Screen::Title {
            let target = if self.screens.current() == Screen::World { Screen::Main } else { Screen::World };
            self.screens.nav_request.set(Some(target));
        }

        if self.input.is_pressed(Action::ToggleDebug) {
            self.debug_enabled = !self.debug_enabled;
            if let Err(e) = self.screens.set_debug_visible(&mut self.ui, self.debug_enabled) {
                error!("Debug overlay visibility error: {e}");
            }
        }

        let ui_input = UiInput::new(self.input.cursor())
            .with_mouse_button(
                MouseButton::Primary,
                self.input.is_held(Action::PrimaryAction),
                self.input.is_pressed(Action::PrimaryAction),
                self.input.is_released(Action::PrimaryAction),
            );

        if let Err(e) = self.ui.handle_input(&ui_input) {
            error!("UI input error: {e}");
        }

        if let Some(target) = self.screens.nav_request.take()
            && let Err(e) = self.screens.go_to(&mut self.ui, target)
        {
            error!("Screen navigation error: {e}");
        }

        if self.screens.settings_dirty.take() {
            self.apply_settings();
        }

        for event in self.ui.take_events() {
            match event {
                UiEvent::Exit => return Some(AppEvent::Exit),
                UiEvent::SetCursor(CursorRequest::Lock) => {
                    window.set_cursor_grab(CursorGrabMode::Locked)
                        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                        .expect("Failed to grab cursor");
                    window.set_cursor_visible(false);
                }
                UiEvent::SetCursor(CursorRequest::Free { x, y }) => {
                    window.set_cursor_grab(CursorGrabMode::None)
                        .expect("Failed to free cursor");
                    window.set_cursor_position(LogicalPosition::new(x, y))
                        .expect("Failed to set cursor position");
                    window.set_cursor_visible(true);
                }
                UiEvent::Unhandled => self.world.handle_input(&self.input, &self.camera),
            }
        }

        self.input.state.clear();
        None
    }

    fn apply_settings(&mut self) {
        let pending = self.screens.pending.get();
        self.blitz.set_vsync(pending.vsync);
        self.blitz.set_fps_limit(Some(pending.fps_cap));
    }

    /// Advance simulation by `delta` seconds.
    pub unsafe fn update(&mut self, dt: f32) {
        if self.screens.current() == Screen::World {
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

        if self.debug_enabled {
            let cam_text   = format!("x:{:.1} y:{:.1} z:{:.1}", self.camera.eye.x, self.camera.eye.y, self.camera.eye.z);
            let mode_text  = format!("Present mode: {}", present_mode_str(self.blitz.get_present_mode()));
            let quad_count = self.ui.quad_count();
            if let Err(e) = self.screens.update_debug(&mut self.ui, cam_text, mode_text, quad_count, self.fps) {
                error!("Debug overlay update error: {e}");
            }
        }

        self.blitz.upload(|container| unsafe {
            if self.world.has_dirty_chunks() {
                self.world.flush_dirty(container);
            }
            match self.ui.flush() {
                UiUpdate::Full(_texture_id, verts) => container.stage_vertex_update(self.ui_vertex_id, &verts),
                UiUpdate::Partial(patches) => for (offset, verts) in patches {
                    container.stage_vertex_update_at(self.ui_vertex_id, offset, &verts);
                },
                UiUpdate::None => {}
            }
            Ok(())
        })?;

        if self.blitz.start_render(window)? {
            if self.screens.current() != Screen::Title {
                self.world.draw(&mut self.blitz, &self.camera)?;
            }
            self.blitz.draw_ui_quads(0, self.ui.quad_count(), self.ui.font_atlas.texture_id.0 as usize);
            self.blitz.end_render(window)?;
        } else {
            let window_area = window.inner_size();
            self.ui.resize((window_area.width as f32, window_area.height as f32));
        }

        self.on_frame();
        Ok(())
    }

    /// Updates the rolling FPS estimate once per second.
    fn on_frame(&mut self) {
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed();
        if elapsed.as_secs_f32() >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
    }
}

fn present_mode_str(mode: PresentModeKHR) -> &'static str {
    match mode {
        PresentModeKHR::FIFO => "FIFO",
        PresentModeKHR::FIFO_LATEST_READY => "FIFO_LATEST_READY",
        PresentModeKHR::MAILBOX => "MAILBOX",
        PresentModeKHR::IMMEDIATE => "IMMEDIATE",
        _ => "Error",
    }
}
