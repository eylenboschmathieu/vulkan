use crate::types::{Rgba, Texture};

use super::{InteractionCb, NodeBase, Renderable};

/// Toggleable checkbox with distinct unselected, selected, hovered, and
/// pressed appearances.
pub struct CheckboxNode {
    pub base:        NodeBase,
    renderable:      Renderable,
    selected_color:  Rgba,        // selected colour
    pub selected:    bool,
    pub interaction: InteractionCb,
}

impl CheckboxNode {
    pub fn new() -> Self {
        Self {
            base:            NodeBase::new(),
            renderable:      Renderable::new(Rgba::new(0.5, 0.5, 0.5, 0.4)),
            selected_color:  Rgba::new(0.2, 0.7, 0.3, 0.7),
            selected:        false,
            interaction:     InteractionCb::default(),
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.renderable.set_color(color); }
    pub fn set_selected_color(&mut self, color: Rgba) { self.selected_color = color; }
    pub fn set_hover_color(&mut self, color: Option<Rgba>) { self.interaction.hover_color = color; }
    pub fn set_pressed_color(&mut self, color: Option<Rgba>) { self.interaction.pressed_color = color; }
    pub fn set_focused_color(&mut self, color: Option<Rgba>) { self.interaction.focused_color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.renderable.set_texture(texture); }
    pub fn set_hover_texture(&mut self, texture: Option<Texture>) { self.interaction.hover_texture = texture; }
    pub fn set_pressed_texture(&mut self, texture: Option<Texture>) { self.interaction.pressed_texture = texture; }
    pub fn set_focused_texture(&mut self, texture: Option<Texture>) { self.interaction.focused_texture = texture; }

    /// The color to render given the node's current hover/press/focus state
    /// (as tracked by [`crate::Ui`]) and its own `selected` state:
    /// `pressed_color` (falling back to `hover_color`) while pressed,
    /// `hover_color` while hovered, `focused_color` while focused, otherwise
    /// `selected_color` or `color`.
    pub fn display_color(&self, hovered: bool, pressed: bool, focused: bool) -> Rgba {
        let base = if self.selected { self.selected_color } else { self.renderable.color() };
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
    /// `focused_texture` while focused, otherwise `texture`. Unlike
    /// [`display_color`](Self::display_color), this doesn't vary with
    /// `selected`.
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

impl Default for CheckboxNode {
    fn default() -> Self {
        Self::new()
    }
}
