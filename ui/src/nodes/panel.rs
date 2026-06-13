use crate::types::{Rgba, Texture};

use super::NodeBase;

/// Visible background panel. Labelable.
pub struct PanelNode {
    pub base: NodeBase,
    pub(crate) color: Rgba,
    pub(crate) texture: Texture,
    pub children: Vec<usize>,
    /// Next [`NodeBase::z_index`] to assign to a child raised to the front;
    /// starts at `1` since `0` means "not orderable".
    pub z_sentinel: u32,
}

impl PanelNode {
    pub fn new() -> Self {
        Self {
            base: NodeBase::new(),
            color: Rgba::new(0.0, 0.0, 0.0, 0.0),
            texture: Texture::default(),
            children: Vec::new(),
            z_sentinel: 1,
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }
}

impl Default for PanelNode {
    fn default() -> Self {
        Self::new()
    }
}
