#![allow(dead_code, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps, clippy::type_complexity)]

mod font;
mod input;
mod navigator;
mod nodes;
mod output;
mod types;

use std::{any::Any, hash::Hash, rc::Rc};

use anyhow::{anyhow, Result};

pub use font::{FontAtlas, GlyphInfo};
pub use input::{Key, MouseButton, UiInput};
pub use nodes::{Anchor, ButtonNode, CheckboxNode, ContainerNode, LabelNode, PanelNode, SliderNode, UiNode, UiNodeVariant};
pub use output::{CursorRequest, UiEvent, UiUpdate};
pub use types::{Pos2, Rgba, Texture, TextureId, Vertex, UV};
use navigator::Navigator;
use nodes::*;

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn edges(&self, parent: &Edges) -> Edges {
        Edges {
            left:   parent.left + self.x,
            right:  parent.left + self.x + self.width,
            top:    parent.top  + self.y,
            bottom: parent.top  + self.y + self.height,
        }
    }
}

#[derive(Clone)]
pub struct Edges {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Edges {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left  &&
        x <= self.right &&
        y >= self.top   &&
        y <= self.bottom
    }

    pub fn intersects(&self, other: &Edges) -> bool {
        self.left   < other.right  &&
        self.right  > other.left   &&
        self.top    < other.bottom &&
        self.bottom > other.top
    }
}

// ── UiTree ───────────────────────────────────────────────────────────────────

pub struct UiTree {
    pub nodes: Vec<UiNode>,
}

impl UiTree {
    pub fn new(width: f32, height: f32) -> Self {
        let mut ui_parent = ContainerNode::new();
        ui_parent.base.set_size(width, height);

        Self {
            nodes: vec![UiNode::Container(ui_parent)],
        }
    }

    /// Looks up `idx` and extracts it as a `&T`, erroring instead of panicking
    /// when the index is out of bounds or the node isn't a `T`.
    fn get_node<T: UiNodeVariant>(&self, idx: usize) -> Result<&T> {
        let node = self.nodes.get(idx).ok_or_else(|| anyhow!("UI node index {idx} out of bounds"))?;
        T::from_node(node).ok_or_else(|| anyhow!("UI node {idx} is not a {}", T::NAME))
    }

    /// Mutable counterpart of [`get_node`](Self::get_node).
    fn get_node_mut<T: UiNodeVariant>(&mut self, idx: usize) -> Result<&mut T> {
        let node = self.nodes.get_mut(idx).ok_or_else(|| anyhow!("UI node index {idx} out of bounds"))?;
        T::from_node_mut(node).ok_or_else(|| anyhow!("UI node {idx} is not a {}", T::NAME))
    }

    pub fn add_child(&mut self, mut node: UiNode, parent_idx: usize) -> usize {
        let idx = self.nodes.len();
        node.base_mut().parent = Some(parent_idx);
        self.nodes.push(node);
        self.nodes[parent_idx].base_mut().children.push(idx);
        idx
    }

    pub fn hit_test(&self, mx: f32, my: f32, node_idx: usize, parent_edges: &Edges) -> Option<usize> {
        let node = &self.nodes[node_idx];
        if !node.base().visible { return None; }

        let edges = node.base().resolve(parent_edges, &self.nodes);
        if !edges.contains(mx, my) { return None; }

        for &child_idx in &node.base().children {
            if let Some(hit) = self.hit_test(mx, my, child_idx, &edges) {
                return Some(hit);
            }
        }

        // Containers and labels are transparent to input
        match node {
            UiNode::Container(_) | UiNode::Label(_) => None,
            _ => Some(node_idx),
        }
    }
}

// ── Ui ───────────────────────────────────────────────────────────────────────

pub struct Ui {
    /// Set when the tree has changed in a way `flush_dirty` can't patch
    /// (structural change, or a label's `max_len` growing). Calling either
    /// `flush_all` or `flush_dirty` leaves this `false`.
    dirty: bool,
    quad_count: usize,
    pub font_atlas: Rc<FontAtlas>,

    tree: UiTree,
    /// The node the cursor is currently over, if any (a [`ButtonNode`] or
    /// [`CheckboxNode`]'s hover color/texture applies while this is `Some(idx)`
    /// for its own index).
    hovered_node: Option<usize>,
    /// The node primary was pressed on and the cursor is still over, if any
    /// (a [`ButtonNode`] or [`CheckboxNode`]'s pressed color/texture applies
    /// while this is `Some(idx)` for its own index). Cleared on release or
    /// when the cursor leaves the node while held.
    pressed_node: Option<usize>,
    /// The node currently being dragged (e.g. a slider's thumb), if any.
    dragging_node: Option<usize>,
    /// Nodes needing a vertex patch. Drained to empty by either `flush_all`
    /// or `flush_dirty`.
    dirty_nodes: Vec<usize>,

    /// Type-erased [`Navigator<S>`], set by
    /// [`init_navigation`](Self::init_navigation). `S` is the host's own
    /// screen type — `Ui` doesn't need to know what it is.
    navigator: Option<Box<dyn Any>>,

    // ── Events ────────────────────────────────────────────────────────────
    // Pushed by node callbacks (which only have `&mut Ui`, never `&mut Host`)
    // and drained by the host via `take_events` after each `handle_input` call.
    events: Vec<UiEvent>,
}

/// Builds the quads for a label's text, starting at `(left, baseline_y)` and
/// always emitting exactly `max_len` quads — one per reserved character slot —
/// so a label occupies a constant amount of vertex-buffer space regardless of
/// how long its current text is. Slots with nothing to draw (a character
/// missing from the atlas, or padding past the end of `text`) get a
/// degenerate, zero-area quad, which rasterizes to nothing.
fn label_quads(atlas: &FontAtlas, text: &str, color: Rgba, start_x: f32, baseline_y: f32, max_len: usize) -> Vec<Vertex> {
    let mut verts: Vec<Vertex> = Vec::with_capacity(max_len * 4);
    let mut cursor_x = start_x;
    let mut chars = text.chars();

    for _ in 0..max_len {
        let c = chars.next();
        let glyph = c.and_then(|c| atlas.glyphs.get(&c));

        match glyph {
            Some(g) => {
                let [u0, v0] = g.uv_min;
                let [u1, v1] = g.uv_max;
                let left   = cursor_x + g.bearing_x;
                let right  = left + g.width as f32;
                let top    = baseline_y - g.bearing_y - g.height as f32;
                let bottom = baseline_y - g.bearing_y;

                verts.push(Vertex::new(Pos2 { x: left,  y: top    }, UV::new(u0, v0), color));
                verts.push(Vertex::new(Pos2 { x: right, y: top    }, UV::new(u1, v0), color));
                verts.push(Vertex::new(Pos2 { x: right, y: bottom }, UV::new(u1, v1), color));
                verts.push(Vertex::new(Pos2 { x: left,  y: bottom }, UV::new(u0, v1), color));

                cursor_x += g.advance;
            }
            None => {
                let p = Pos2 { x: cursor_x, y: baseline_y };
                let degenerate = Vertex::new(p, UV::new(0.0, 0.0), color);
                verts.extend_from_slice(&[degenerate; 4]);

                if c.is_some() { cursor_x += 8.0; }
            }
        }
    }

    verts
}

/// Builds the 4 vertices of a quad filling `edges`, sampling `texture`'s UV
/// rect and tinted by `color`.
fn quad_verts(edges: &Edges, color: Rgba, texture: Texture) -> [Vertex; 4] {
    let Texture { uv_min: [u0, v0], uv_max: [u1, v1], .. } = texture;
    [
        Vertex::new(Pos2 { x: edges.left,  y: edges.top    }, UV::new(u0, v0), color),
        Vertex::new(Pos2 { x: edges.right, y: edges.top    }, UV::new(u1, v0), color),
        Vertex::new(Pos2 { x: edges.right, y: edges.bottom }, UV::new(u1, v1), color),
        Vertex::new(Pos2 { x: edges.left,  y: edges.bottom }, UV::new(u0, v1), color),
    ]
}

impl Ui {
    pub fn new(screen_size: (f32, f32), atlas: Rc<FontAtlas>) -> Self {
        Self {
            dirty: true,
            quad_count: 0,
            font_atlas: atlas,
            tree: UiTree::new(screen_size.0, screen_size.1),
            hovered_node: None,
            pressed_node: None,
            dragging_node: None,
            dirty_nodes: Vec::new(),
            navigator: None,
            events: Vec::new(),
        }
    }

    /// Resizes the root container to match the window's new size, and marks
    /// the UI dirty for a full re-flush. The host calls this whenever its
    /// window is resized — every anchor-relative node ultimately resolves
    /// against the root container's bounds.
    pub fn resize(&mut self, screen_size: (f32, f32)) {
        self.tree.nodes[0].base_mut().set_size(screen_size.0, screen_size.1);
        self.dirty = true;
    }

    // ── Node creation helpers ────────────────────────────────────────────────
    // Each wraps a node in its parent, applying only the boilerplate that's
    // the same for every instance (e.g. the white UV rect for solid quads).
    // Everything else — bounds, color, action, text, ... — is configured by
    // the caller afterwards through the returned node's own setters/fields.

    pub fn create_container(&mut self, parent: usize) -> Result<(usize, &mut ContainerNode)> {
        let idx = self.tree.add_child(UiNode::Container(ContainerNode::new()), parent);
        let c = self.tree.get_node_mut::<ContainerNode>(idx)?;
        Ok((idx, c))
    }

    /// The shared UI atlas's white texel, tagged with its texture id — the
    /// default texture for nodes that render a solid color quad.
    fn solid_texture(&self) -> Texture {
        Texture { id: self.font_atlas.texture_id, uv_min: self.font_atlas.white_uv, uv_max: self.font_atlas.white_uv }
    }

    pub fn create_panel(&mut self, parent: usize) -> Result<(usize, &mut PanelNode)> {
        let mut p = PanelNode::new();
        p.set_texture(self.solid_texture());
        let idx = self.tree.add_child(UiNode::Panel(p), parent);
        let p = self.tree.get_node_mut::<PanelNode>(idx)?;
        Ok((idx, p))
    }

    pub fn create_button(&mut self, parent: usize) -> Result<(usize, &mut ButtonNode)> {
        let mut b = ButtonNode::new();
        b.set_texture(self.solid_texture());
        let idx = self.tree.add_child(UiNode::Button(b), parent);
        let b = self.tree.get_node_mut::<ButtonNode>(idx)?;
        Ok((idx, b))
    }

    pub fn create_label(&mut self, parent: usize) -> Result<(usize, &mut LabelNode)> {
        let cap_height = self.font_atlas.cap_height;
        let mut l = LabelNode::new("");
        l.base.set_height(cap_height);
        let idx = self.tree.add_child(UiNode::Label(l), parent);
        let l = self.tree.get_node_mut::<LabelNode>(idx)?;
        Ok((idx, l))
    }

    pub fn create_checkbox(&mut self, parent: usize) -> Result<(usize, &mut CheckboxNode)> {
        let mut c = CheckboxNode::new();
        c.set_texture(self.solid_texture());
        let idx = self.tree.add_child(UiNode::Checkbox(c), parent);
        let c = self.tree.get_node_mut::<CheckboxNode>(idx)?;
        Ok((idx, c))
    }

    /// Also creates the slider's thumb (panel) and value label as children
    /// and wires their indices back into the returned `SliderNode`.
    pub fn create_slider(&mut self, parent: usize) -> Result<(usize, &mut SliderNode)> {
        let mut slider = SliderNode::new();
        slider.panel.set_texture(self.solid_texture());
        let label_text  = slider.display_text(true);
        let label_width = self.label_width(&label_text);
        let slider_idx = self.tree.add_child(UiNode::Slider(slider), parent);

        let (thumb_idx, thumb) = self.create_button(slider_idx)?;
        thumb.base.set_size(16.0, 32.0);
        thumb.set_color(Rgba::new(0.8, 0.8, 0.8, 0.9));
        thumb.set_hover_color(Some(Rgba::new(0.3, 0.6, 1.0, 0.9)));

        let (label_idx, label) = self.create_label(parent)?;
        label.set_text(label_text);
        label.base.set_width(label_width);
        label.base.set_position_anchored_to(Anchor::Right, slider_idx, Anchor::Left, -8.0, 0.0);

        let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
        s.set_thumb(Some(thumb_idx));
        s.set_label(Some(label_idx));
        Ok((slider_idx, s))
    }

    /// The number of quads in the vertex buffer produced by the last
    /// [`flush_all`](Self::flush_all).
    pub fn quad_count(&self) -> usize {
        self.quad_count
    }

    /// The (color, texture) to render for `idx`, accounting for its
    /// hover/press state, or `None` for node types that don't render a quad
    /// of their own (containers, labels — labels are handled separately by
    /// [`label_quads`]).
    fn render_data(&self, idx: usize) -> Option<(Rgba, Texture)> {
        let hovered = self.hovered_node == Some(idx);
        let pressed = self.pressed_node == Some(idx);
        match &self.tree.nodes[idx] {
            UiNode::Panel(p)    => Some((p.color, p.texture)),
            UiNode::Button(b)   => Some((b.display_color(hovered, pressed), b.display_texture(hovered, pressed))),
            UiNode::Checkbox(c) => Some((c.display_color(hovered, pressed), c.display_texture(hovered, pressed))),
            UiNode::Slider(s)   => Some((s.panel.color, s.panel.texture)),
            _ => None,
        }
    }

    /// Rebuilds the entire vertex list from the current tree state, returning
    /// it as [`UiUpdate::Full`] tagged with the UI atlas's texture id. Clears
    /// `dirty` and `dirty_nodes` so subsequent frames can use
    /// [`flush_dirty`](Self::flush_dirty) until the next structural change.
    /// Must be called whenever a node is added, removed, or its `max_len`
    /// grows, since those events shift `vertex_offset` bookkeeping for every
    /// node that follows.
    pub fn flush_all(&mut self) -> UiUpdate {
        self.dirty = false;
        self.dirty_nodes.clear();
        let atlas = &*self.font_atlas;
        let mut verts: Vec<Vertex> = Vec::new();

        let root_edges = self.tree.nodes[0].base().resolve(&Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 }, &self.tree.nodes);
        let mut stack: Vec<(usize, Edges)> = vec![(0, root_edges)];

        while let Some((node_idx, parent_edges)) = stack.pop() {
            let child_count = self.tree.nodes[node_idx].base().children.len();
            for i in 0..child_count {
                let child_idx = self.tree.nodes[node_idx].base().children[i];
                if !self.tree.nodes[child_idx].base().visible { continue; }

                let e = self.tree.nodes[child_idx].base().resolve(&parent_edges, &self.tree.nodes);

                match &self.tree.nodes[child_idx] {
                    UiNode::Label(l) => {
                        let text    = l.text.clone();
                        let color   = l.color;
                        let max_len = l.max_len();

                        self.tree.nodes[child_idx].base_mut().vertex_offset = verts.len();
                        verts.extend(label_quads(atlas, &text, color, e.left, e.bottom, max_len));
                    }
                    _ => {
                        if let Some((color, texture)) = self.render_data(child_idx) {
                            self.tree.nodes[child_idx].base_mut().vertex_offset = verts.len();
                            verts.extend(quad_verts(&e, color, texture));
                        }

                        stack.push((child_idx, e));
                    }
                }
            }
        }

        self.quad_count = verts.len() / 4;
        UiUpdate::Full(atlas.texture_id, verts)
    }

    /// Builds in-place patches for the nodes listed in `dirty_nodes`, returning
    /// them as [`UiUpdate::Partial`] (or [`UiUpdate::None`] if nothing is
    /// dirty). Each patch is keyed by the node's recorded `vertex_offset`. Safe
    /// to call when the tree structure hasn't changed and no node's `max_len`
    /// has grown, since those conditions guarantee every node still occupies
    /// the same slot in the buffer it was assigned during the last
    /// [`flush_all`](Self::flush_all). Drains `dirty_nodes`, so a subsequent
    /// call returns [`UiUpdate::None`] until more nodes are marked dirty;
    /// `dirty` is left untouched (and should already be `false`, or
    /// [`flush_all`](Self::flush_all) should have been called instead).
    pub fn flush_dirty(&mut self) -> UiUpdate {
        let dirty: Vec<usize> = self.dirty_nodes.drain(..).collect();
        if dirty.is_empty() {
            return UiUpdate::None;
        }

        let mut patches: Vec<(usize, Vec<Vertex>)> = Vec::with_capacity(dirty.len());
        for node_idx in dirty {
            match &self.tree.nodes[node_idx] {
                UiNode::Label(l) => {
                    let text    = l.text.clone();
                    let color   = l.color;
                    let max_len = l.max_len();
                    let atlas   = &*self.font_atlas;

                    let e      = self.node_edges(node_idx);
                    let offset = self.tree.nodes[node_idx].base().vertex_offset;
                    let vertices = label_quads(atlas, &text, color, e.left, e.bottom, max_len);

                    patches.push((offset, vertices));
                }
                _ => {
                    if let Some((color, texture)) = self.render_data(node_idx) {
                        let e      = self.node_edges(node_idx);
                        let offset = self.tree.nodes[node_idx].base().vertex_offset;
                        patches.push((offset, quad_verts(&e, color, texture).to_vec()));
                    }
                }
            }
        }

        UiUpdate::Partial(patches)
    }

    /// Returns the vertex update needed this frame: a full rebuild via
    /// [`flush_all`](Self::flush_all) if the tree structure changed, an
    /// in-place patch via [`flush_dirty`](Self::flush_dirty) if only node
    /// state changed, or [`UiUpdate::None`] if nothing changed.
    pub fn flush(&mut self) -> UiUpdate {
        if self.dirty {
            self.flush_all()
        } else {
            self.flush_dirty()
        }
    }

    /// Computes the absolute screen-space [`Edges`] of `node_idx` by walking
    /// its parent chain and resolving each node's position in turn.
    fn node_edges(&self, node_idx: usize) -> Edges {
        let node = &self.tree.nodes[node_idx];
        let parent_edges = match node.base().parent {
            Some(p) => self.node_edges(p),
            None    => Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        };
        node.base().resolve(&parent_edges, &self.tree.nodes)
    }

    /// Sums glyph advances to get the rendered width of `text`.
    fn label_width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.font_atlas.glyphs.get(&c).map_or(8.0, |g| g.advance)).sum()
    }

    /// Resolves a hit on a slider's panel or thumb to the slider's own index.
    fn slider_at(&self, idx: usize) -> Option<usize> {
        match &self.tree.nodes[idx] {
            UiNode::Slider(_) => Some(idx),
            _ => {
                let parent = self.tree.nodes[idx].base().parent?;
                match &self.tree.nodes[parent] {
                    UiNode::Slider(s) if s.get_thumb() == Some(idx) => Some(parent),
                    _ => None,
                }
            }
        }
    }

    /// Repositions the thumb and refreshes the value label to match the
    /// slider's current value. Marks both as dirty for re-rendering. Hosts
    /// call this after changing a slider's value or range from their own code
    /// (e.g. an `on_show` callback that re-syncs the slider to external state).
    pub fn layout_slider(&mut self, slider_idx: usize) -> Result<()> {
        let (thumb_idx, label_idx) = {
            let s = self.tree.get_node::<SliderNode>(slider_idx)?;
            (s.get_thumb(), s.get_label())
        };

        let right_aligned = label_idx
            .map(|idx| self.tree.nodes[idx].base().src_anchor.is_right())
            .unwrap_or(true);
        let text = self.tree.get_node::<SliderNode>(slider_idx)?.display_text(right_aligned);

        if let Some(thumb_idx) = thumb_idx {
            let thumb_width = self.tree.nodes[thumb_idx].base().bounds.width;
            let x_offset = {
                let s = self.tree.get_node::<SliderNode>(slider_idx)?;
                s.thumb_offset(thumb_width)
            };
            let thumb = self.tree.get_node_mut::<ButtonNode>(thumb_idx)?;
            thumb.base.set_position(Anchor::Left, x_offset, 0.0);
            self.dirty_nodes.push(thumb_idx);
        }

        if let Some(label_idx) = label_idx {
            let width = self.label_width(&text);
            self.set_label_text(label_idx, text)?;
            self.tree.get_node_mut::<LabelNode>(label_idx)?.base.set_width(width);
        }

        Ok(())
    }

    /// Replaces a label's text, marking it dirty for an in-place
    /// [`flush_dirty`](Self::flush_dirty) patch, or the whole tree dirty for
    /// a [`flush_all`](Self::flush_all) if its `max_len` grew. Hosts call
    /// this to update a label from their own code (e.g. a per-frame debug
    /// overlay).
    pub fn set_label_text(&mut self, idx: usize, text: impl Into<String>) -> Result<()> {
        let label = self.tree.get_node_mut::<LabelNode>(idx)?;
        if label.set_text(text) {
            self.dirty = true;
        } else {
            self.dirty_nodes.push(idx);
        }
        Ok(())
    }

    /// Recomputes the slider's value from the cursor position relative to
    /// where the drag started, then re-lays-out the thumb and label.
    fn drag_slider(&mut self, slider_idx: usize, cursor: (f32, f32)) -> Result<()> {
        let new_value = {
            let s = self.tree.get_node::<SliderNode>(slider_idx)?;
            let thumb_width = s.get_thumb().map_or(0.0, |idx| self.tree.nodes[idx].base().bounds.width);
            s.value_from_drag(cursor, thumb_width)
        };

        let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
        s.set_value(new_value);

        self.layout_slider(slider_idx)
    }

    /// Looks up `idx` and extracts it as a `&T`, erroring instead of panicking
    /// when the index is out of bounds or the node isn't a `T`. Lets host
    /// callbacks (which only have `&mut Ui`) read widget state without `ui`
    /// knowing what that state means.
    pub fn get_node<T: UiNodeVariant>(&self, idx: usize) -> Result<&T> {
        self.tree.get_node(idx)
    }

    /// Mutable counterpart of [`get_node`](Self::get_node).
    pub fn get_node_mut<T: UiNodeVariant>(&mut self, idx: usize) -> Result<&mut T> {
        self.tree.get_node_mut(idx)
    }

    /// Returns whether `idx` is `ancestor` itself or one of its descendants.
    fn is_or_contains(&self, ancestor: usize, idx: usize) -> bool {
        let mut cur = Some(idx);
        while let Some(i) = cur {
            if i == ancestor { return true; }
            cur = self.tree.nodes[i].base().parent;
        }
        false
    }

    /// Shows or hides `idx`, firing its `on_show`/`on_hide` callback and
    /// marking the whole UI dirty for a full re-flush. If the
    /// currently-hovered node is `idx` or one of its descendants and `idx` is
    /// being hidden, that node's hover state is restored first — otherwise it
    /// would stay visually "stuck" hovered after it can no longer be hit-tested.
    /// This is the generic primitive hosts use to switch between their own
    /// screens, e.g. `ui.set_visible(old, false)?; ui.set_visible(new, true)?;`.
    pub fn set_visible(&mut self, idx: usize, visible: bool) -> Result<()> {
        if !visible
            && let Some(hovered) = self.hovered_node
            && self.is_or_contains(idx, hovered)
        {
            if self.pressed_node == Some(hovered) {
                self.pressed_node = None;
            }
            self.hovered_node = None;
        }

        self.tree.nodes[idx].base_mut().visible = visible;
        self.dirty = true;

        if visible {
            self.fire_callback(idx, |c| &mut c.visibility.on_show)?;
        } else {
            self.fire_callback(idx, |c| &mut c.visibility.on_hide)?;
        }

        Ok(())
    }

    /// Takes a visibility callback out of `node_idx`, invokes it with
    /// `&mut self`, then restores it. The take/restore dance works around
    /// Rust's aliasing rules: the callback is borrowed out of `self.tree`, so
    /// it can't stay borrowed while also receiving `&mut self`.
    fn fire_callback(
        &mut self,
        node_idx: usize,
        select: impl Fn(&mut NodeBase) -> &mut Option<Box<dyn FnMut(&mut Ui)>>,
    ) -> Result<()> {
        let base = self.tree.nodes[node_idx].base_mut();
        let Some(mut callback) = select(base).take() else { return Ok(()) };
        callback(self);
        let base = self.tree.nodes[node_idx].base_mut();
        *select(base) = Some(callback);
        Ok(())
    }

    /// Like [`fire_callback`](Self::fire_callback), but for the
    /// [`InteractionCb`] shared by [`ButtonNode`] and [`CheckboxNode`]. Fired
    /// by [`handle_input`](Self::handle_input) after any built-in behavior
    /// for the node (e.g. a checkbox's selected toggle) has been applied.
    fn fire_interaction(
        &mut self,
        node_idx: usize,
        select: impl Fn(&mut InteractionCb) -> &mut Option<Box<dyn FnMut(&mut Ui)>>,
    ) -> Result<()> {
        let interaction = match &mut self.tree.nodes[node_idx] {
            UiNode::Button(b)   => &mut b.interaction,
            UiNode::Checkbox(c) => &mut c.interaction,
            _ => return Ok(()),
        };
        let Some(mut callback) = select(interaction).take() else { return Ok(()) };
        callback(self);
        let interaction = match &mut self.tree.nodes[node_idx] {
            UiNode::Button(b)   => &mut b.interaction,
            UiNode::Checkbox(c) => &mut c.interaction,
            _ => unreachable!(),
        };
        *select(interaction) = Some(callback);
        Ok(())
    }

    /// Requests that the host exit the application. Called by node callbacks,
    /// which only have access to `&mut Ui`. Polled via
    /// [`take_events`](Self::take_events).
    pub fn request_exit(&mut self) {
        self.events.push(UiEvent::Exit);
    }

    /// Requests that the host apply a cursor change to its window. Called by
    /// node callbacks, which only have access to `&mut Ui`. Polled via
    /// [`take_events`](Self::take_events).
    pub fn request_cursor(&mut self, request: CursorRequest) {
        self.events.push(UiEvent::SetCursor(request));
    }

    /// Drains and returns all [`UiEvent`]s queued since the last call, for the
    /// host to act on.
    pub fn take_events(&mut self) -> Vec<UiEvent> {
        std::mem::take(&mut self.events)
    }

    // ── Navigation ────────────────────────────────────────────────────────

    /// Initializes navigation with `initial` as the current screen. `S` is
    /// the host's own screen type; call this once, before any other
    /// navigation method, and always with the same `S`.
    pub fn init_navigation<S: Copy + Eq + Hash + 'static>(&mut self, initial: S) {
        self.navigator = Some(Box::new(Navigator::<S>::new(initial)));
    }

    /// Borrows the navigator as `Navigator<S>`, erroring if
    /// [`init_navigation`](Self::init_navigation) hasn't been called, or was
    /// called with a different `S`.
    fn navigator<S: Copy + Eq + Hash + 'static>(&self) -> Result<&Navigator<S>> {
        self.navigator.as_ref()
            .and_then(|n| n.downcast_ref())
            .ok_or_else(|| anyhow!("navigation not initialized for this screen type"))
    }

    /// Mutable counterpart of [`navigator`](Self::navigator).
    fn navigator_mut<S: Copy + Eq + Hash + 'static>(&mut self) -> Result<&mut Navigator<S>> {
        self.navigator.as_mut()
            .and_then(|n| n.downcast_mut())
            .ok_or_else(|| anyhow!("navigation not initialized for this screen type"))
    }

    /// Associates `screen` with the Container/Panel node `idx`, so
    /// [`navigate_to_screen`](Self::navigate_to_screen) can show/hide it.
    pub fn register_screen<S: Copy + Eq + Hash + 'static>(&mut self, screen: S, idx: usize) -> Result<()> {
        if !matches!(self.tree.nodes.get(idx), Some(UiNode::Container(_) | UiNode::Panel(_))) {
            return Err(anyhow!("navigation target {idx} is not a Container or Panel"));
        }
        self.navigator_mut::<S>()?.screens.insert(screen, idx);
        Ok(())
    }

    /// Registers `target` as the screen [`navigate_to`](Self::navigate_to)
    /// switches to when called with `trigger_idx` — typically a widget's own
    /// index, set up alongside its `on_release` callback.
    pub fn set_navigation<S: Copy + Eq + Hash + 'static>(&mut self, trigger_idx: usize, target: S) -> Result<()> {
        self.navigator_mut::<S>()?.routes.insert(trigger_idx, target);
        Ok(())
    }

    /// Navigates to the screen registered for `trigger_idx` via
    /// [`set_navigation`](Self::set_navigation). Intended to be called from a
    /// widget's own `on_release`, passing its own index.
    pub fn navigate_to<S: Copy + Eq + Hash + 'static>(&mut self, trigger_idx: usize) -> Result<()> {
        let target = *self.navigator::<S>()?.routes.get(&trigger_idx)
            .ok_or_else(|| anyhow!("no navigation route registered for node {trigger_idx}"))?;
        self.navigate_to_screen(target)
    }

    /// Hides the current screen, shows `target`, and makes it current,
    /// firing both screens' `on_hide`/`on_show` callbacks via
    /// [`set_visible`](Self::set_visible). For navigation not tied to a
    /// widget click, e.g. a host-side keybind.
    pub fn navigate_to_screen<S: Copy + Eq + Hash + 'static>(&mut self, target: S) -> Result<()> {
        let nav = self.navigator::<S>()?;
        if nav.current == target { return Ok(()); }
        let current_idx = *nav.screens.get(&nav.current).ok_or_else(|| anyhow!("current screen not registered"))?;
        let target_idx  = *nav.screens.get(&target).ok_or_else(|| anyhow!("target screen not registered"))?;

        self.set_visible(current_idx, false)?;
        self.set_visible(target_idx, true)?;
        self.navigator_mut::<S>()?.current = target;
        Ok(())
    }

    /// The currently active screen.
    pub fn current_screen<S: Copy + Eq + Hash + 'static>(&self) -> Result<S> {
        Ok(self.navigator::<S>()?.current)
    }

    pub fn handle_input(&mut self, input: &UiInput) -> Result<()> {
        let cursor = input.cursor();

        if let Some(slider_idx) = self.dragging_node {
            if input.primary_held() {
                self.drag_slider(slider_idx, cursor)?;
            } else {
                let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
                s.drag.stop();
                self.dragging_node = None;
            }
            return Ok(());
        }

        let hit = self.tree.hit_test(
            cursor.0, cursor.1, 0,
            &Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        );

        if hit != self.hovered_node {
            if let Some(old) = self.hovered_node {
                if matches!(self.tree.nodes[old], UiNode::Button(_) | UiNode::Checkbox(_)) {
                    self.dirty_nodes.push(old);
                }
                // The cursor left the pressed node before it was released -
                // its pressed appearance no longer applies.
                if self.pressed_node == Some(old) {
                    self.pressed_node = None;
                }
                self.fire_interaction(old, |i| &mut i.on_leave)?;
            }
            if let Some(new) = hit {
                if matches!(self.tree.nodes[new], UiNode::Button(_) | UiNode::Checkbox(_)) {
                    self.dirty_nodes.push(new);
                }
                self.fire_interaction(new, |i| &mut i.on_enter)?;
            }
            self.hovered_node = hit;
        }

        match hit {
            Some(idx) => {
                if input.primary_pressed() {
                    // Track pressed state for the "pressed" color/texture
                    // variants, independent of any host-attached callback.
                    if matches!(self.tree.nodes[idx], UiNode::Button(_) | UiNode::Checkbox(_)) {
                        self.pressed_node = Some(idx);
                        self.dirty_nodes.push(idx);
                    }

                    self.fire_interaction(idx, |i| &mut i.on_pressed)?;

                    if let Some(slider_idx) = self.slider_at(idx) {
                        // Clicking the track itself (not the thumb) jumps the
                        // value to the clicked position before the drag starts.
                        if idx == slider_idx {
                            let new_value = {
                                let s = self.tree.get_node::<SliderNode>(slider_idx)?;
                                let thumb_width = s.get_thumb().map_or(0.0, |t| self.tree.nodes[t].base().bounds.width);
                                let local_x = cursor.0 - self.node_edges(slider_idx).left;
                                s.value_from_track_position(local_x, thumb_width)
                            };
                            let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
                            s.set_value(new_value);
                            self.layout_slider(slider_idx)?;
                        }

                        let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
                        let value = s.value as f32;
                        s.drag.start(cursor, value);
                        self.dragging_node = Some(slider_idx);
                    }
                }

                if input.primary_released() {
                    // Built-in behavior: clicking a checkbox toggles its selected
                    // state, independent of any host-attached callback.
                    if let UiNode::Checkbox(c) = &mut self.tree.nodes[idx] {
                        c.selected = !c.selected;
                        self.dirty_nodes.push(idx);
                    }

                    if self.pressed_node == Some(idx) {
                        self.pressed_node = None;
                        if matches!(self.tree.nodes[idx], UiNode::Button(_)) {
                            self.dirty_nodes.push(idx);
                        }
                    }

                    self.fire_interaction(idx, |i| &mut i.on_release)?;
                }
            }
            // A click landed on nothing the UI owns — the host decides what
            // to do with it (e.g. world interaction / selection).
            None => if input.any_click() {
                self.events.push(UiEvent::Unhandled);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
