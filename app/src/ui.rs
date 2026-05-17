#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use cgmath::{vec2, vec3};
use blitz::{Blitz, Container, Vertex_2D_Color, VertexBufferId};

const HOTBAR_SLOTS: usize = 10;
const SLOT_SIZE: f32 = 48.0;
const SLOT_GAP: f32 = 4.0;
const SLOT_MARGIN_BOTTOM: f32 = 20.0;

#[derive(Debug)]
pub struct Ui {
    vertex_id: VertexBufferId,
    hotbar_size: (u32, u32),
}

impl Ui {
    pub fn new(blitz: &Blitz) -> Self {
        Self { vertex_id: blitz.ui_vertex_id(), hotbar_size: (0, 0) }
    }

    pub fn is_dirty(&self, size: (u32, u32)) -> bool {
        self.hotbar_size != size
    }

    pub unsafe fn flush(&mut self, container: &mut Container, size: (u32, u32)) {
        if self.hotbar_size != size {
            self.hotbar_size = size;
            let verts = Self::hotbar_verts(size.0, size.1);
            container.stage_vertex_update(self.vertex_id, &verts);
        }
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        blitz.draw_ui_quads(0, HOTBAR_SLOTS);
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
}
