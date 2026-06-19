use crate::types::{Rgba, Texture};

use super::{Anchor, InteractionCb, NodeBase, Renderable};

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

    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) { self.base.set_position(anchor, x, y); }
    pub fn set_size(&mut self, width: f32, height: f32) { self.base.set_size(width, height); }

    pub fn set_color(&mut self, color: Rgba) { self.renderable.set_color(color); self.base.mark_dirty(); }
    pub fn set_selected_color(&mut self, color: Rgba) { self.selected_color = color; self.base.mark_dirty(); }
    pub fn set_hover_color(&mut self, color: Option<Rgba>) { self.interaction.hover_color = color; self.base.mark_dirty(); }
    pub fn set_pressed_color(&mut self, color: Option<Rgba>) { self.interaction.pressed_color = color; self.base.mark_dirty(); }
    pub fn set_texture(&mut self, texture: Texture) { self.renderable.set_texture(texture); self.base.mark_dirty(); }
    pub fn set_hover_texture(&mut self, texture: Option<Texture>) { self.interaction.hover_texture = texture; self.base.mark_dirty(); }
    pub fn set_pressed_texture(&mut self, texture: Option<Texture>) { self.interaction.pressed_texture = texture; self.base.mark_dirty(); }

    pub fn set_selected(&mut self, selected: bool) {
        if self.selected != selected {
            self.selected = selected;
            self.base.mark_dirty();
        }
    }

    /// The color to render given the node's current hover/press state and its
    /// own `selected` state. Focus is shown by the dedicated focus-ring overlay.
    pub fn display_color(&self, hovered: bool, pressed: bool) -> Rgba {
        let base = if self.selected { self.selected_color } else { self.renderable.color() };
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

impl Default for CheckboxNode {
    fn default() -> Self {
        Self::new()
    }
}
