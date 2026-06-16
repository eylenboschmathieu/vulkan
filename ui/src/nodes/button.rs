use crate::types::{Rgba, Texture};

use super::{InteractionCb, NodeBase, Renderable};

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

    pub fn set_color(&mut self, color: Rgba) { self.renderable.set_color(color); }
    pub fn set_hover_color(&mut self, color: Option<Rgba>) { self.interaction.hover_color = color; }
    pub fn set_pressed_color(&mut self, color: Option<Rgba>) { self.interaction.pressed_color = color; }
    pub fn set_focused_color(&mut self, color: Option<Rgba>) { self.interaction.focused_color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.renderable.set_texture(texture); }
    pub fn set_hover_texture(&mut self, texture: Option<Texture>) { self.interaction.hover_texture = texture; }
    pub fn set_pressed_texture(&mut self, texture: Option<Texture>) { self.interaction.pressed_texture = texture; }
    pub fn set_focused_texture(&mut self, texture: Option<Texture>) { self.interaction.focused_texture = texture; }

    /// The color to render given the node's current hover/press/focus state
    /// (as tracked by [`crate::Ui`]): `pressed_color` (falling back to
    /// `hover_color`) while pressed, `hover_color` while hovered,
    /// `focused_color` while focused, otherwise `color`.
    pub fn display_color(&self, hovered: bool, pressed: bool, focused: bool) -> Rgba {
        let base = self.renderable.color();
        if pressed {
            self.interaction.pressed_color.or(self.interaction.hover_color).unwrap_or(base)
        } else if hovered {
            self.interaction.hover_color.unwrap_or(base)
        } else if focused {
            self.interaction.focused_color.unwrap_or(base)
        } else {
            base
        }
    }

    /// The texture to render given the node's current hover/press/focus
    /// state (as tracked by [`crate::Ui`]): `pressed_texture` (falling back
    /// to `hover_texture`) while pressed, `hover_texture` while hovered,
    /// `focused_texture` while focused, otherwise `texture`.
    pub fn display_texture(&self, hovered: bool, pressed: bool, focused: bool) -> Texture {
        let base = self.renderable.texture();
        if pressed {
            self.interaction.pressed_texture.or(self.interaction.hover_texture).unwrap_or(base)
        } else if hovered {
            self.interaction.hover_texture.unwrap_or(base)
        } else if focused {
            self.interaction.focused_texture.unwrap_or(base)
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
