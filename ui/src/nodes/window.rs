use crate::types::{Rgba, Texture};

use super::NodeBase;

/// Height of a window's titlebar, in the same units as [`NodeBase::bounds`].
pub const TITLEBAR_HEIGHT: f32 = 24.0;

/// Width of the border between a window's own quad and its [`WindowNode::body`].
pub const WINDOW_BORDER: f32 = 2.0;

/// A floating panel with a titlebar (holding a [`WindowNode::title`] label
/// and a [`WindowNode::close_button`]) and an inset [`WindowNode::body`]
/// panel for content. The window's own quad renders as the border/frame
/// around `body`. Built by [`crate::Ui::create_window`].
pub struct WindowNode {
    pub base: NodeBase,
    pub(crate) color: Rgba,
    pub(crate) texture: Texture,
    /// `PanelNode` spanning the top of the window, child of this node.
    pub titlebar: usize,
    /// `LabelNode` showing the window's title, child of `titlebar`.
    pub title: usize,
    /// `ButtonNode` that hides this window on release, child of `titlebar`.
    pub close_button: usize,
    /// `PanelNode` for content, inset from the window's edges by
    /// [`WINDOW_BORDER`] (and `titlebar`'s height on top), child of this node.
    pub body: usize,
    /// Structural children: always `[titlebar, body]`. Content added by
    /// callers belongs under `body`, not here.
    pub children: Vec<usize>,
    /// Next [`NodeBase::z_index`] to assign to a child raised to the front;
    /// starts at `1` since `0` means "not orderable".
    pub z_sentinel: u32,
}

impl WindowNode {
    pub fn new() -> Self {
        Self {
            base: NodeBase::new(),
            color: Rgba::new(0.0, 0.0, 0.0, 0.0),
            texture: Texture::default(),
            titlebar: 0,
            title: 0,
            close_button: 0,
            body: 0,
            children: Vec::new(),
            z_sentinel: 1,
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }
}

impl Default for WindowNode {
    fn default() -> Self {
        Self::new()
    }
}
