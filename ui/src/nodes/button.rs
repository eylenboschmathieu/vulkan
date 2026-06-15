use crate::types::{Rgba, Texture};

use super::{InteractionCb, NodeBase};

/// Interactive button. Labelable.
pub struct ButtonNode {
    pub base:        NodeBase,
    color:           Rgba,
    hover_color:     Option<Rgba>,
    pressed_color:   Option<Rgba>,
    focused_color:   Option<Rgba>,
    texture:         Texture,
    hover_texture:   Option<Texture>,
    pressed_texture: Option<Texture>,
    focused_texture: Option<Texture>,
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
            color:           Rgba::new(0.0, 0.0, 0.0, 0.0),
            hover_color:     None,
            pressed_color:   None,
            focused_color:   None,
            texture:         Texture::default(),
            hover_texture:   None,
            pressed_texture: None,
            focused_texture: None,
            interaction:     InteractionCb::default(),
            children:        Vec::new(),
            z_sentinel:      1,
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_hover_color(&mut self, color: Option<Rgba>) { self.hover_color = color; }
    pub fn set_pressed_color(&mut self, color: Option<Rgba>) { self.pressed_color = color; }
    pub fn set_focused_color(&mut self, color: Option<Rgba>) { self.focused_color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }
    pub fn set_hover_texture(&mut self, texture: Option<Texture>) { self.hover_texture = texture; }
    pub fn set_pressed_texture(&mut self, texture: Option<Texture>) { self.pressed_texture = texture; }
    pub fn set_focused_texture(&mut self, texture: Option<Texture>) { self.focused_texture = texture; }

    /// The color to render given the node's current hover/press/focus state
    /// (as tracked by [`crate::Ui`]): `pressed_color` (falling back to
    /// `hover_color`) while pressed, `hover_color` while hovered,
    /// `focused_color` while focused, otherwise `color`.
    pub fn display_color(&self, hovered: bool, pressed: bool, focused: bool) -> Rgba {
        if pressed {
            self.pressed_color.or(self.hover_color).unwrap_or(self.color)
        } else if hovered {
            self.hover_color.unwrap_or(self.color)
        } else if focused {
            self.focused_color.unwrap_or(self.color)
        } else {
            self.color
        }
    }

    /// The texture to render given the node's current hover/press/focus
    /// state (as tracked by [`crate::Ui`]): `pressed_texture` (falling back
    /// to `hover_texture`) while pressed, `hover_texture` while hovered,
    /// `focused_texture` while focused, otherwise `texture`.
    pub fn display_texture(&self, hovered: bool, pressed: bool, focused: bool) -> Texture {
        if pressed {
            self.pressed_texture.or(self.hover_texture).unwrap_or(self.texture)
        } else if hovered {
            self.hover_texture.unwrap_or(self.texture)
        } else if focused {
            self.focused_texture.unwrap_or(self.texture)
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
