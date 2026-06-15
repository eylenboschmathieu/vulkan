use crate::types::{Rgba, Texture};

use super::{Container, NodeBase};

/// Height of a window's titlebar, in the same units as [`NodeBase::bounds`].
pub const TITLEBAR_HEIGHT: f32 = 24.0;

/// Width of the border between a window's own quad and its [`WindowNode::body`].
pub const WINDOW_BORDER: f32 = 2.0;

/// Drag-to-move state for a [`WindowNode`]: the cursor position / window
/// position captured when a drag began, so the window's new position can be
/// computed from a delta without accumulating drift. Whether a drag is
/// active at all is tracked by `Ui::dragging_node`, not here.
#[derive(Default, Clone, Copy)]
pub struct WindowDrag {
    pub start_cursor: (f32, f32),
    pub start_pos:    (f32, f32),
}

impl WindowDrag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, cursor: (f32, f32), pos: (f32, f32)) {
        self.start_cursor = cursor;
        self.start_pos    = pos;
    }
}

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
    /// Structural children (`container.children`): always `[titlebar, body]`.
    /// Content added by callers belongs under `body`, not here.
    pub container: Container,
    /// Whether pressing the titlebar starts a drag-to-move. `false` by
    /// default; see [`WindowNode::set_draggable`].
    pub draggable: bool,
    /// Drag-to-move state, updated by [`crate::Ui::handle_input`].
    pub drag: WindowDrag,
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
            container: Container::new(),
            draggable: false,
            drag: WindowDrag::new(),
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }

    /// Sets whether pressing this window's titlebar starts a drag-to-move.
    pub fn set_draggable(&mut self, draggable: bool) { self.draggable = draggable; }
}

impl Default for WindowNode {
    fn default() -> Self {
        Self::new()
    }
}
