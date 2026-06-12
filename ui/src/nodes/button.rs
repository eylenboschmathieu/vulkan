use crate::types::{Rgba, Texture};

use super::{InteractionCb, NodeBase};

/// Interactive button. Labelable.
pub struct ButtonNode {
    pub base:        NodeBase,
    color:           Rgba,
    hover_color:     Option<Rgba>,
    pressed_color:   Option<Rgba>,
    texture:         Texture,
    hover_texture:   Option<Texture>,
    pressed_texture: Option<Texture>,
    pub interaction: InteractionCb,
}

impl ButtonNode {
    pub fn new() -> Self {
        Self {
            base:            NodeBase::new(),
            color:           Rgba::new(0.0, 0.0, 0.0, 0.0),
            hover_color:     None,
            pressed_color:   None,
            texture:         Texture::default(),
            hover_texture:   None,
            pressed_texture: None,
            interaction:     InteractionCb::default(),
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_hover_color(&mut self, color: Option<Rgba>) { self.hover_color = color; }
    pub fn set_pressed_color(&mut self, color: Option<Rgba>) { self.pressed_color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }
    pub fn set_hover_texture(&mut self, texture: Option<Texture>) { self.hover_texture = texture; }
    pub fn set_pressed_texture(&mut self, texture: Option<Texture>) { self.pressed_texture = texture; }

    /// The color to render given the node's current hover/press state (as
    /// tracked by [`crate::Ui`]): `pressed_color` (falling back to
    /// `hover_color`) while pressed, `hover_color` while hovered, otherwise
    /// `color`.
    pub fn display_color(&self, hovered: bool, pressed: bool) -> Rgba {
        if pressed {
            self.pressed_color.or(self.hover_color).unwrap_or(self.color)
        } else if hovered {
            self.hover_color.unwrap_or(self.color)
        } else {
            self.color
        }
    }

    /// The texture to render given the node's current hover/press state (as
    /// tracked by [`crate::Ui`]): `pressed_texture` (falling back to
    /// `hover_texture`) while pressed, `hover_texture` while hovered,
    /// otherwise `texture`.
    pub fn display_texture(&self, hovered: bool, pressed: bool) -> Texture {
        if pressed {
            self.pressed_texture.or(self.hover_texture).unwrap_or(self.texture)
        } else if hovered {
            self.hover_texture.unwrap_or(self.texture)
        } else {
            self.texture
        }
    }
}

impl Default for ButtonNode {
    fn default() -> Self {
        Self::new()
    }
}
