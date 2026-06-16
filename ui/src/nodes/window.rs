use anyhow::Result;
use crate::{types::{Rgba, Texture}, Ui};

use super::{Anchor, Container, NodeBase, Renderable, UiNode};

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
    pub(crate) renderable: Renderable,
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
    /// Inserts this window and all its structural children (titlebar, title
    /// label, close button + label, body panel) into the tree, wires their
    /// indices, and sets all default colors and sizes. This is the full
    /// construction logic for [`crate::Ui::create_window`].
    pub fn build(ui: &mut Ui, parent: usize, width: f32, height: f32) -> Result<(usize, &mut Self)> {
        let frame_color = Rgba::new(0.25, 0.25, 0.3, 1.0);

        let window_idx = ui.add_node(UiNode::Window(Self::new()), parent)?;
        let w = ui.get_node_mut::<Self>(window_idx)?;
        w.base.set_size(width, height);
        w.set_color(frame_color);

        let (titlebar_idx, titlebar) = ui.create_panel(window_idx)?;
        titlebar.base.set_position(Anchor::TopLeft, WINDOW_BORDER, WINDOW_BORDER);
        titlebar.base.set_size(width - 2.0 * WINDOW_BORDER, TITLEBAR_HEIGHT);
        titlebar.set_color(frame_color);

        let (title_idx, title) = ui.create_label(titlebar_idx)?;
        title.set_color(Rgba::new(1.0, 1.0, 1.0, 1.0));
        title.base.set_position(Anchor::Left, 0.0, 0.0);

        let (close_idx, close_btn) = ui.create_button(titlebar_idx)?;
        close_btn.base.set_size(TITLEBAR_HEIGHT, TITLEBAR_HEIGHT);
        close_btn.base.set_position(Anchor::TopRight, 0.0, 0.0);
        close_btn.set_color(Rgba::new(1.0, 1.0, 1.0, 0.15));
        close_btn.set_hover_color(Some(Rgba::new(0.8, 0.2, 0.2, 0.7)));
        close_btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| {
            let _ = ui.set_visible(window_idx, false);
        }));

        let close_label_width = ui.label_width("x");
        let (_, close_label) = ui.create_label(close_idx)?;
        close_label.set_text("x");
        close_label.set_color(Rgba::new(1.0, 1.0, 1.0, 1.0));
        close_label.base.set_width(close_label_width);
        close_label.base.set_position(Anchor::Center, 0.0, 0.0);

        let (body_idx, body) = ui.create_panel(window_idx)?;
        body.base.set_position(Anchor::TopLeft, WINDOW_BORDER, TITLEBAR_HEIGHT + 2.0 * WINDOW_BORDER);
        body.base.set_size(width - 2.0 * WINDOW_BORDER, height - TITLEBAR_HEIGHT - 3.0 * WINDOW_BORDER);
        body.set_color(Rgba::new(0.15, 0.15, 0.17, 0.95));
        body.container.clip_children = true;

        let w = ui.get_node_mut::<Self>(window_idx)?;
        w.titlebar = titlebar_idx;
        w.title = title_idx;
        w.close_button = close_idx;
        w.body = body_idx;
        Ok((window_idx, w))
    }

    pub fn new() -> Self {
        Self {
            base: NodeBase::new(),
            renderable: Renderable::default(),
            titlebar: 0,
            title: 0,
            close_button: 0,
            body: 0,
            container: Container::new(),
            draggable: false,
            drag: WindowDrag::new(),
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.renderable.set_color(color); }
    pub fn set_texture(&mut self, texture: Texture) { self.renderable.set_texture(texture); }

    /// Sets whether pressing this window's titlebar starts a drag-to-move.
    pub fn set_draggable(&mut self, draggable: bool) { self.draggable = draggable; }

    /// Repositions this window by `cursor`'s delta from
    /// [`WindowDrag::start_cursor`], relative to [`WindowDrag::start_pos`].
    /// See [`crate::Ui::drag_window`], which additionally clamps the result
    /// to the parent and marks the subtree dirty.
    pub fn drag_to(&mut self, cursor: (f32, f32)) {
        let dx = cursor.0 - self.drag.start_cursor.0;
        let dy = cursor.1 - self.drag.start_cursor.1;
        self.base.bounds.x = self.drag.start_pos.0 + dx;
        self.base.bounds.y = self.drag.start_pos.1 + dy;
    }
}

impl Default for WindowNode {
    fn default() -> Self {
        Self::new()
    }
}
