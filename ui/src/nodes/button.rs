use crate::types::{Rgba, Texture};

use super::{Anchor, InteractionCb, NodeBase, Renderable};

/// Interactive button. Labelable.
pub struct ButtonNode {
    pub base:        NodeBase,
    renderable:      Renderable,
    pub interaction: InteractionCb,
    pub children: Vec<usize>,
    /// Next [`NodeBase::z_index`] to assign to a child raised to the front;
    /// starts at `1` since `0` means "not orderable".
    pub z_sentinel: u32,
}

impl ButtonNode {
    pub fn new() -> Self {
        Self {
            base:            NodeBase::new(),
            renderable:      Renderable::default(),
            interaction:     InteractionCb::default(),
            children:        Vec::new(),
            z_sentinel:      1,
        }
    }

    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) { self.base.set_position(anchor, x, y); }
    pub fn set_size(&mut self, width: f32, height: f32) { self.base.set_size(width, height); }

    pub fn set_color(&mut self, color: Rgba) { self.renderable.set_color(color); self.base.mark_dirty(); }
    pub fn set_hover_color(&mut self, color: Option<Rgba>) { self.interaction.hover_color = color; self.base.mark_dirty(); }
    pub fn set_pressed_color(&mut self, color: Option<Rgba>) { self.interaction.pressed_color = color; self.base.mark_dirty(); }
    pub fn set_texture(&mut self, texture: Texture) { self.renderable.set_texture(texture); self.base.mark_dirty(); }
    pub fn set_hover_texture(&mut self, texture: Option<Texture>) { self.interaction.hover_texture = texture; self.base.mark_dirty(); }
    pub fn set_pressed_texture(&mut self, texture: Option<Texture>) { self.interaction.pressed_texture = texture; self.base.mark_dirty(); }

    /// The color to render given the node's current hover/press state:
    /// `pressed_color` (falling back to `hover_color`) while pressed,
    /// `hover_color` while hovered, otherwise `color`. Focus is shown by the
    /// dedicated focus-ring overlay, not a per-node color.
    pub fn display_color(&self, hovered: bool, pressed: bool) -> Rgba {
        let base = self.renderable.color();
        if pressed {
            self.interaction.pressed_color.or(self.interaction.hover_color).unwrap_or(base)
        } else if hovered {
            self.interaction.hover_color.unwrap_or(base)
        } else {
            base
        }
    }

    /// The texture to render given the node's current hover/press state.
    pub fn display_texture(&self, hovered: bool, pressed: bool) -> Texture {
        let base = self.renderable.texture();
        if pressed {
            self.interaction.pressed_texture.or(self.interaction.hover_texture).unwrap_or(base)
        } else if hovered {
            self.interaction.hover_texture.unwrap_or(base)
        } else {
            base
        }
    }
}

impl Default for ButtonNode {
    fn default() -> Self {
        Self::new()
    }
}
