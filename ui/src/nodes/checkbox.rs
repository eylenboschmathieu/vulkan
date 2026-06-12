use crate::types::{Rgba, Texture};

use super::{InteractionCb, NodeBase};

/// Toggleable checkbox with distinct unselected, selected, hovered, and
/// pressed appearances.
pub struct CheckboxNode {
    pub base:        NodeBase,
    color:           Rgba,        // unselected colour
    selected_color:  Rgba,        // selected colour
    hover_color:     Option<Rgba>,
    pressed_color:   Option<Rgba>,
    texture:         Texture,
    hover_texture:   Option<Texture>,
    pressed_texture: Option<Texture>,
    pub selected:    bool,
    pub interaction: InteractionCb,
}

impl CheckboxNode {
    pub fn new() -> Self {
        Self {
            base:            NodeBase::new(),
            color:           Rgba::new(0.5, 0.5, 0.5, 0.4),
            selected_color:  Rgba::new(0.2, 0.7, 0.3, 0.7),
            hover_color:     None,
            pressed_color:   None,
            texture:         Texture::default(),
            hover_texture:   None,
            pressed_texture: None,
            selected:        false,
            interaction:     InteractionCb::default(),
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_selected_color(&mut self, color: Rgba) { self.selected_color = color; }
    pub fn set_hover_color(&mut self, color: Option<Rgba>) { self.hover_color = color; }
    pub fn set_pressed_color(&mut self, color: Option<Rgba>) { self.pressed_color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }
    pub fn set_hover_texture(&mut self, texture: Option<Texture>) { self.hover_texture = texture; }
    pub fn set_pressed_texture(&mut self, texture: Option<Texture>) { self.pressed_texture = texture; }

    /// The color to render given the node's current hover/press state (as
    /// tracked by [`crate::Ui`]) and its own `selected` state:
    /// `pressed_color` (falling back to `hover_color`) while pressed,
    /// `hover_color` while hovered, otherwise `selected_color` or `color`.
    pub fn display_color(&self, hovered: bool, pressed: bool) -> Rgba {
        let base = if self.selected { self.selected_color } else { self.color };
        if pressed {
            self.pressed_color.or(self.hover_color).unwrap_or(base)
        } else if hovered {
            self.hover_color.unwrap_or(base)
        } else {
            base
        }
    }

    /// The texture to render given the node's current hover/press state (as
    /// tracked by [`crate::Ui`]): `pressed_texture` (falling back to
    /// `hover_texture`) while pressed, `hover_texture` while hovered,
    /// otherwise `texture`. Unlike [`display_color`](Self::display_color),
    /// this doesn't vary with `selected`.
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

impl Default for CheckboxNode {
    fn default() -> Self {
        Self::new()
    }
}
