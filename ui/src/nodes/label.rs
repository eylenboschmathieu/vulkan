use crate::font::FontAtlas;
use crate::types::{Pos2, Rgba, Vertex, UV};

use super::NodeBase;

/// Text label. Not interactive, not labelable itself.
pub struct LabelNode {
    pub base: NodeBase,
    pub text: String,
    pub(crate) color: Rgba,
    /// Longest `text` has ever been, in chars. Rendering always reserves this
    /// many quads (padding unused slots with degenerate ones), so the label's
    /// vertex allocation stays constant across [`crate::Ui::flush_dirty`]
    /// updates and only needs to grow — never shrink — via
    /// [`crate::Ui::flush_all`].
    max_len: usize,
}

impl LabelNode {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let max_len = text.chars().count();
        Self {
            base: NodeBase::new(),
            text,
            color: Rgba::new(0.0, 0.0, 0.0, 1.0),
            max_len,
        }
    }

    pub fn max_len(&self) -> usize {
        self.max_len
    }

    pub fn color(&self) -> Rgba { self.color }
    pub fn set_color(&mut self, color: Rgba) { self.color = color; self.base.mark_dirty(); }

    /// Builds this label's quads for rendering, starting at `(start_x,
    /// baseline_y)` and always emitting exactly [`LabelNode::max_len`] quads
    /// — one per reserved character slot — so the label occupies a constant
    /// amount of vertex-buffer space regardless of how long `text` currently
    /// is. Slots with nothing to draw (a character missing from `atlas`, or
    /// padding past the end of `text`) get a degenerate, zero-area quad,
    /// which rasterizes to nothing.
    pub fn quads(&self, atlas: &FontAtlas, start_x: f32, baseline_y: f32) -> Vec<Vertex> {
        let mut verts: Vec<Vertex> = Vec::with_capacity(self.max_len * 4);
        let mut cursor_x = start_x;
        let mut chars = self.text.chars();

        for _ in 0..self.max_len {
            let c = chars.next();
            let glyph = c.and_then(|c| atlas.glyphs.get(&c));

            match glyph {
                Some(g) => {
                    let [u0, v0] = g.uv_min;
                    let [u1, v1] = g.uv_max;
                    let left   = cursor_x + g.bearing_x;
                    let right  = left + g.width as f32;
                    let top    = baseline_y - g.bearing_y - g.height as f32;
                    let bottom = baseline_y - g.bearing_y;

                    verts.push(Vertex::new(Pos2 { x: left,  y: top    }, UV::new(u0, v0), self.color));
                    verts.push(Vertex::new(Pos2 { x: right, y: top    }, UV::new(u1, v0), self.color));
                    verts.push(Vertex::new(Pos2 { x: right, y: bottom }, UV::new(u1, v1), self.color));
                    verts.push(Vertex::new(Pos2 { x: left,  y: bottom }, UV::new(u0, v1), self.color));

                    cursor_x += g.advance;
                }
                None => {
                    let p = Pos2 { x: cursor_x, y: baseline_y };
                    let degenerate = Vertex::new(p, UV::new(0.0, 0.0), self.color);
                    verts.extend_from_slice(&[degenerate; 4]);

                    if c.is_some() { cursor_x += 8.0; }
                }
            }
        }

        verts
    }

    /// Replaces the text, growing `max_len` if it's now the longest this
    /// label has ever held. Schedules a full tree rebuild when `max_len`
    /// grows (so [`crate::Ui::flush_all`] reserves the larger allocation);
    /// otherwise queues an in-place patch via the global dirty list.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        let len = self.text.chars().count();
        if len > self.max_len {
            self.max_len = len;
            self.base.mark_full_dirty();
        } else {
            self.base.mark_dirty();
        }
    }
}
