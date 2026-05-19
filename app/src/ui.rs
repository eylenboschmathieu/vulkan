#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use blitz::{Blitz, Container, Pos2, Rgba, Vertex_2D_RGBA, VertexBufferId};

const HOTBAR_SLOTS:    usize = 10;
const SLOT_SIZE:       f32   = 48.0;
const SLOT_GAP:        f32   = 4.0;
const SLOT_MARGIN_BOTTOM: f32 = 20.0;

const XH_SIZE:      f32 = 16.0; // half-length of each arm
const XH_THICKNESS: f32 = 2.0;  // half-thickness of each arm

const TOTAL_QUADS: usize = HOTBAR_SLOTS + 2; // hotbar + crosshair (horizontal + vertical)

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
            let verts = Self::generate_ui(size.0, size.1);
            container.stage_vertex_update(self.vertex_id, &verts);
        }
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        blitz.draw_ui_quads(0, TOTAL_QUADS);
    }

    fn generate_ui(screen_width: u32, screen_height: u32) -> Vec<Vertex_2D_RGBA> {
        let mut verts = Vec::with_capacity(TOTAL_QUADS * 4);

        // Hotbar
        let total_w = HOTBAR_SLOTS as f32 * SLOT_SIZE + (HOTBAR_SLOTS - 1) as f32 * SLOT_GAP;
        let x0 = (screen_width as f32 - total_w) / 2.0;
        let y0 = screen_height as f32 - SLOT_SIZE - SLOT_MARGIN_BOTTOM;
        let slot_color = Rgba::new(0.25, 0.25, 0.25, 1.0);
        for i in 0..HOTBAR_SLOTS {
            let x = x0 + i as f32 * (SLOT_SIZE + SLOT_GAP);
            verts.push(Vertex_2D_RGBA::new(Pos2::new(x,             y0),             slot_color));
            verts.push(Vertex_2D_RGBA::new(Pos2::new(x + SLOT_SIZE, y0),             slot_color));
            verts.push(Vertex_2D_RGBA::new(Pos2::new(x + SLOT_SIZE, y0 + SLOT_SIZE), slot_color));
            verts.push(Vertex_2D_RGBA::new(Pos2::new(x,             y0 + SLOT_SIZE), slot_color));
        }

        // Crosshair
        let cx = screen_width  as f32 / 2.0;
        let cy = screen_height as f32 / 2.0;
        let xh_color = Rgba::new(1.0, 1.0, 1.0, 0.1);

        // Horizontal bar
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx - XH_SIZE,      cy - XH_THICKNESS), xh_color));
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx + XH_SIZE,      cy - XH_THICKNESS), xh_color));
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx + XH_SIZE,      cy + XH_THICKNESS), xh_color));
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx - XH_SIZE,      cy + XH_THICKNESS), xh_color));

        // Vertical bar
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx - XH_THICKNESS, cy - XH_SIZE),      xh_color));
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx + XH_THICKNESS, cy - XH_SIZE),      xh_color));
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx + XH_THICKNESS, cy + XH_SIZE),      xh_color));
        verts.push(Vertex_2D_RGBA::new(Pos2::new(cx - XH_THICKNESS, cy + XH_SIZE),      xh_color));

        verts
    }
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
