#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{rc::Rc, time::Instant};

use vulkanalia::vk::PresentModeKHR;
use blitz::{Blitz, Container, Pos2, Rgba, UV, VERTEX_2D_RGBA, VertexAllocId};
use winit::window::Window;

use crate::{camera::FpCamera, font::FontAtlas};

const PADDING: f32 = 10.0;

/// Displays FPS and camera position as an on-screen overlay.
pub struct DebugInfo {
    pub enabled: bool,
    atlas: Rc<FontAtlas>,
    vertex_id: VertexAllocId,
    quad_count: usize,
    pub present_mode: PresentModeKHR,
    pub ui_quad_count: usize,

    fps: f32,
    frame_count: u32,
    fps_timer: Instant,
}

impl DebugInfo {
    pub fn new(window: &Window, blitz: &Blitz, atlas: Rc<FontAtlas>) -> Self {
        Self {
            enabled: false,
            atlas,
            vertex_id: blitz.debug_vertex_id(),
            quad_count: 0,
            present_mode: blitz.get_present_mode(),
            ui_quad_count: 0,
            fps: 0.0,
            frame_count: 0,
            fps_timer: Instant::now(),
        }
    }

    pub fn on_frame(&mut self) {
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed();
        if elapsed.as_secs_f32() >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
    }

    pub unsafe fn flush(&mut self, container: &mut Container, camera: &FpCamera, screen_width: f32) {
        let verts = self.generate(camera, screen_width);
        self.quad_count = verts.len() / 4;
        container.stage_vertex_update(self.vertex_id, &verts);
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        if !self.enabled || self.quad_count == 0 { return; }
        blitz.draw_debug_quads(0, self.quad_count, self.atlas.texture_id);
    }

    fn generate(&self, camera: &FpCamera, screen_width: f32) -> Vec<VERTEX_2D_RGBA> {
        let atlas = &*self.atlas;
        let mut verts = Vec::new();
        let white = Rgba::new(1.0, 1.0, 1.0, 1.0);

        // Camera position — top left
        let cam_text = format!("x:{:.1} y:{:.1} z:{:.1}", camera.eye.x, camera.eye.y, camera.eye.z);
        Self::emit_text(&mut verts, atlas, &cam_text, PADDING, PADDING, white);

        // Present Mode - top left
        let present_mode_text = match self.present_mode {
            PresentModeKHR::FIFO => "FIFO",
            PresentModeKHR::FIFO_LATEST_READY => "FIFO_LATEST_READY",
            PresentModeKHR::MAILBOX => "MAILBOX",
            PresentModeKHR::IMMEDIATE => "IMMEDIATE",
            _ => "Error"
        };
        let present_mode_text = format!("Present mode: {}", present_mode_text);
        Self::emit_text(&mut verts, atlas, &present_mode_text, PADDING, 32.0, white);

        // UI quad count
        let ui_quad_count_text = format!("UI quad count: {}", self.ui_quad_count);
        Self::emit_text(&mut verts, atlas, &ui_quad_count_text, PADDING, 54.0, white);

        // FPS — top right
        let fps_text = format!("{:.0} fps", self.fps);
        let fps_width = Self::measure_text(atlas, &fps_text);
        Self::emit_text(&mut verts, atlas, &fps_text, screen_width - fps_width - PADDING, PADDING, white);

        verts
    }

    fn emit_text(verts: &mut Vec<VERTEX_2D_RGBA>, atlas: &FontAtlas, text: &str, x: f32, y: f32, color: Rgba) {
        let mut cursor_x = x;
        let baseline_y   = y + atlas.line_height;

        for c in text.chars() {
            let Some(g) = atlas.glyphs.get(&c) else { cursor_x += 8.0; continue };
            let [u0, v0] = g.uv_min;
            let [u1, v1] = g.uv_max;
            let left     = cursor_x + g.bearing_x;
            let right    = left + g.width as f32;
            let top      = baseline_y - g.bearing_y - g.height as f32;
            let bottom   = baseline_y - g.bearing_y;

            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: left,  y: top    }, UV::new(u0, v0), color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: right, y: top    }, UV::new(u1, v0), color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: right, y: bottom }, UV::new(u1, v1), color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: left,  y: bottom }, UV::new(u0, v1), color));

            cursor_x += g.advance;
        }
    }

    fn measure_text(atlas: &FontAtlas, text: &str) -> f32 {
        text.chars().map(|c| atlas.glyphs.get(&c).map_or(8.0, |g| g.advance)).sum()
    }
}
