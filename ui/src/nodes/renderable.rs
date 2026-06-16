use crate::types::{Rgba, Texture};

/// Baseline fill color and texture shared by every renderable, non-`Group`
/// node type: [`super::PanelNode`], [`super::WindowNode`],
/// [`super::ButtonNode`], [`super::CheckboxNode`], and (via its embedded
/// `PanelNode`) [`super::SliderNode`]'s track. Node-specific variants
/// (hover/pressed/focused/selected) live in [`super::InteractionCb`] or on
/// the node itself, not here.
pub struct Renderable {
    color: Rgba,
    texture: Texture,
}

impl Renderable {
    pub fn new(color: Rgba) -> Self {
        Self { color, texture: Texture::default() }
    }

    pub fn color(&self) -> Rgba { self.color }
    pub fn texture(&self) -> Texture { self.texture }
    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }
}

impl Default for Renderable {
    fn default() -> Self {
        Self::new(Rgba::new(0.0, 0.0, 0.0, 0.0))
    }
}
