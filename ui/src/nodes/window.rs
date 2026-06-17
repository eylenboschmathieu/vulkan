use anyhow::Result;
use crate::{Rect, types::{Rgba, Texture}, Ui};

use super::{Anchor, Axis, Container, NodeBase, Renderable, ScrollPanelNode, TabBody, TabPanelNode, UiNode};

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

/// Which body node [`crate::Ui::create_window`] should create below the
/// titlebar. [`WindowNode::body`] is set to the created node's index in all
/// cases except [`WindowBody::None`].
pub enum WindowBody {
    /// Creates a default [`super::PanelNode`] body (`clip_children = true`),
    /// inset by [`WINDOW_BORDER`] on all sides below the titlebar.
    Panel,
    /// Creates a [`super::TabPanelNode`] that fills the body area. Configure
    /// it and add tabs via the index stored in [`WindowNode::body`].
    TabPanel {
        tab_height:       f32,
        scrollbar_height: f32,
        /// Body type of the tab panel itself (plain panel or scroll panel).
        tab_body: TabBody,
    },
    /// Creates a [`super::ScrollPanelNode`] that fills the body area. Access
    /// it via the index stored in [`WindowNode::body`]. The viewport is
    /// derived from the body area minus `scrollbar_width`.
    ScrollPanel {
        axis:            Axis,
        scrollbar_width: f32,
        content_size:    (f32, f32),
    },
    /// No body node is created; [`WindowNode::body`] is left as `0`. Use
    /// [`WindowNode::body_rect`] when a completely custom body is needed.
    None,
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
    /// The body node, inset from the window's edges by [`WINDOW_BORDER`] (and
    /// `titlebar`'s height on top), child of this node. Its concrete type
    /// depends on the [`WindowBody`] variant: [`super::PanelNode`] for
    /// `Panel`, [`super::TabPanelNode`] for `TabPanel`, and
    /// [`super::ScrollPanelNode`] for `ScrollPanel`. `0` when the window was
    /// built with [`WindowBody::None`].
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
    /// The position and size a body node should occupy inside a window of the
    /// given dimensions: inset by [`WINDOW_BORDER`] on all sides, below the
    /// titlebar. Pass this rect's `x`/`y` to [`NodeBase::set_position`] and
    /// its `width`/`height` to [`NodeBase::set_size`] on whatever node you
    /// want to act as the window body.
    pub fn body_rect(width: f32, height: f32) -> Rect {
        Rect {
            x:      WINDOW_BORDER,
            y:      TITLEBAR_HEIGHT + 2.0 * WINDOW_BORDER,
            width:  width  - 2.0 * WINDOW_BORDER,
            height: height - TITLEBAR_HEIGHT - 3.0 * WINDOW_BORDER,
        }
    }

    /// Builds the window frame + titlebar (title label, close button). Sets
    /// `titlebar`, `title`, and `close_button`; `body` is left as `0`.
    fn build_frame(ui: &mut Ui, parent: usize, width: f32, height: f32) -> Result<usize> {
        let frame_color = Rgba::new(0.25, 0.25, 0.3, 1.0);

        let window_idx = ui.add_node(UiNode::Window(Self::new()), parent)?;
        {
            let w = ui.get_node_mut::<Self>(window_idx)?;
            w.base.set_size(width, height);
            w.set_color(frame_color);
        }

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

        let w = ui.get_node_mut::<Self>(window_idx)?;
        w.titlebar = titlebar_idx;
        w.title    = title_idx;
        w.close_button = close_idx;

        Ok(window_idx)
    }

    /// Inserts this window and its structural children into the tree, wires
    /// their indices, and sets all default colors and sizes. The `body`
    /// parameter controls whether a default body panel is created (see
    /// [`WindowBody`]). This is the construction logic for
    /// [`crate::Ui::create_window`].
    pub fn build(ui: &mut Ui, parent: usize, width: f32, height: f32, body: WindowBody) -> Result<(usize, &mut Self)> {
        let window_idx = Self::build_frame(ui, parent, width, height)?;

        let body_rect = Self::body_rect(width, height);
        let body_idx = match body {
            WindowBody::Panel => {
                let (idx, b) = ui.create_panel(window_idx)?;
                b.base.set_position(Anchor::TopLeft, body_rect.x, body_rect.y);
                b.base.set_size(body_rect.width, body_rect.height);
                b.set_color(Rgba::new(0.15, 0.15, 0.17, 0.95));
                b.container.clip_children = true;
                Some(idx)
            }
            WindowBody::TabPanel { tab_height, scrollbar_height, tab_body } => {
                let (tp_idx, _) = TabPanelNode::build(ui, window_idx, body_rect.width, body_rect.height, tab_height, scrollbar_height, tab_body)?;
                ui.get_node_mut::<TabPanelNode>(tp_idx)?.group.base.set_position(Anchor::TopLeft, body_rect.x, body_rect.y);
                Some(tp_idx)
            }
            WindowBody::ScrollPanel { axis, scrollbar_width, content_size } => {
                let viewport = match axis {
                    Axis::Vertical   => (body_rect.width - scrollbar_width, body_rect.height),
                    Axis::Horizontal => (body_rect.width, body_rect.height - scrollbar_width),
                };
                let (sp_idx, sp) = ScrollPanelNode::build(ui, window_idx, axis, viewport, scrollbar_width, content_size)?;
                sp.base.set_position(Anchor::TopLeft, body_rect.x, body_rect.y);
                Some(sp_idx)
            }
            WindowBody::None => None,
        };
        if let Some(idx) = body_idx {
            ui.get_node_mut::<Self>(window_idx)?.body = idx;
        }

        Ok((window_idx, ui.get_node_mut::<Self>(window_idx)?))
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
    /// Draggable windows are implicitly raised to the front of their siblings
    /// on press (see [`Ui::raise`]) without a prior [`Ui::register_orderable`]
    /// call.
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
