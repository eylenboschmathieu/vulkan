#![allow(dead_code, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps, clippy::type_complexity)]

mod font;
mod input;
mod layers;
mod navigator;
mod nodes;
mod output;
mod types;

use std::{any::Any, hash::Hash, rc::Rc};

use anyhow::{anyhow, Result};

pub use font::{FontAtlas, GlyphInfo};
pub use input::{Key, MouseButton, UiInput};
pub use nodes::{Anchor, Axis, ButtonNode, CheckboxNode, Container, GroupNode, LabelNode, PanelNode, Scroll, ScrollPanelNode, SliderNode, UiNode, UiNodeVariant, WindowNode, TITLEBAR_HEIGHT, WINDOW_BORDER};
pub use output::{CursorRequest, UiEvent, UiUpdate};
pub use types::{Pos2, Rgba, Texture, TextureId, Vertex, UV};
use layers::LayerOrder;
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

#[derive(Clone, Copy, PartialEq, Debug)]
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

    /// The overlapping region of `self` and `other`. If they don't overlap,
    /// the result is degenerate (`left > right` and/or `top > bottom`), which
    /// callers treat as "clips away everything".
    pub fn intersect(&self, other: &Edges) -> Edges {
        Edges {
            left:   self.left.max(other.left),
            right:  self.right.min(other.right),
            top:    self.top.max(other.top),
            bottom: self.bottom.min(other.bottom),
        }
    }

    /// Shifts all four edges by `(dx, dy)`.
    pub fn translate(&self, dx: f32, dy: f32) -> Edges {
        Edges { left: self.left + dx, right: self.right + dx, top: self.top + dy, bottom: self.bottom + dy }
    }
}

/// A contiguous range of quads in the vertex buffer that share a clip rect,
/// produced by [`Ui::flush_all`] and [`Ui::flush_dirty`] and read by the host
/// via [`Ui::batches`] to issue one draw call per entry (updating its clip
/// rect, e.g. via a push constant, before drawing the range).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DrawBatch {
    /// The clip rect quads in this batch must be rendered within, in
    /// screen-space pixels. `None` means unclipped (renders everywhere).
    pub clip_rect: Option<Edges>,
    /// Index of the first quad in this batch.
    pub first_quad: usize,
    /// Number of quads in this batch.
    pub quad_count: usize,
    /// Index of the `clip_children` node that generated `clip_rect`, or
    /// `None` for unclipped batches. Two batches that coincidentally resolve
    /// to the same clip rect but from different ancestors are kept separate
    /// so that [`Ui::refresh_batch_clip`] can never update the wrong one.
    clip_source: Option<usize>,
}

// ── UiTree ───────────────────────────────────────────────────────────────────

pub struct UiTree {
    pub nodes: Vec<UiNode>,
}

impl UiTree {
    pub fn new(width: f32, height: f32) -> Self {
        let mut ui_parent = GroupNode::new();
        ui_parent.base.set_size(width, height);

        Self {
            nodes: vec![UiNode::Group(ui_parent)],
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

    /// Appends `node` as a new child of `parent_idx`, returning the new
    /// node's index. Errors if `parent_idx` is a leaf node type that can't
    /// have children (see [`UiNode::children_mut`]).
    pub fn add_child(&mut self, mut node: UiNode, parent_idx: usize) -> Result<usize> {
        let idx = self.nodes.len();
        node.base_mut().parent = Some(parent_idx);
        self.nodes.push(node);
        self.nodes[parent_idx].children_mut()
            .ok_or_else(|| anyhow!("UI node {parent_idx} cannot have children"))?
            .push(idx);
        Ok(idx)
    }

    /// Returns `node_idx`'s children sorted for rendering/hit-testing: for
    /// the root, by `(band, z_index)`; for any other node, by `z_index`
    /// alone (its `band` is unused below the root). The sort is stable, so
    /// children with equal keys — including the common all-zero case —
    /// keep insertion order, i.e. today's behavior. Render order is
    /// low-to-high (painter's algorithm: later = on top); hit-testing
    /// iterates this in reverse (topmost first).
    pub fn ordered_children(&self, node_idx: usize) -> Vec<usize> {
        let Some(children) = self.nodes[node_idx].children() else { return Vec::new() };
        let mut ordered = children.to_vec();
        if node_idx == 0 {
            ordered.sort_by_key(|&idx| {
                let base = self.nodes[idx].base();
                (base.band, base.z_index)
            });
        } else {
            ordered.sort_by_key(|&idx| self.nodes[idx].base().z_index);
        }
        ordered
    }

    /// Returns the topmost interactive node under `(mx, my)`, or `None` if
    /// nothing is hit. For a top-level call, pass `node_idx: 0` (root) with
    /// `parent_edges` a zero-sized [`Edges`] (root has no parent to resolve
    /// against). `Group` and `Label` nodes are transparent to input and
    /// never returned themselves. Recurses into `node_idx`'s children
    /// (topmost first, via [`ordered_children`](Self::ordered_children))
    /// even if the cursor is outside `node_idx`'s own bounds, since children
    /// aren't clipped to their parent when rendered.
    /// `clip` is the clip rect inherited from `node_idx`'s `clip_children`
    /// ancestors (`None` = unclipped); pass `None` for a top-level call.
    pub fn hit_test(&self, mx: f32, my: f32, node_idx: usize, parent_edges: &Edges, clip: Option<Edges>) -> Option<usize> {
        let node = &self.nodes[node_idx];
        if !node.base().visible { return None; }

        let edges = node.base().resolve(parent_edges, &self.nodes);

        let child_clip = if node.clip_children() {
            Some(clip.map_or(edges, |c| c.intersect(&edges)))
        } else {
            clip
        };

        let child_edges = match node.scroll() {
            Some(s) => edges.translate(-s.offset.0, -s.offset.1),
            None    => edges,
        };

        // Recurse regardless of whether (mx, my) is within this node's own
        // bounds: children aren't clipped to their parent's bounds when
        // rendered (unless `clip_children` says otherwise), so they shouldn't
        // be for hit-testing either.
        for child_idx in self.ordered_children(node_idx).into_iter().rev() {
            if let Some(hit) = self.hit_test(mx, my, child_idx, &child_edges, child_clip) {
                return Some(hit);
            }
        }

        if !edges.contains(mx, my) { return None; }
        if !clip.is_none_or(|c| c.contains(mx, my)) { return None; }

        // Containers, labels, and non-interactive overlay nodes are transparent to input.
        match node {
            UiNode::Group(_) | UiNode::Label(_) => None,
            _ if !node.base().interactive => None,
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
    /// Draw batches produced by the last [`flush_all`](Self::flush_all),
    /// with clip-rect values kept current by [`flush_dirty`](Self::flush_dirty).
    /// See [`Ui::batches`].
    batches: Vec<DrawBatch>,
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
    /// The node currently being dragged (a slider, via its thumb, or a
    /// draggable window via its titlebar), if any.
    dragging_node: Option<usize>,
    /// The node with keyboard focus, if any — Tab/Shift+Tab move this
    /// between [`UiNode::focusable`] nodes, and Enter/Space activate it.
    focused_node: Option<usize>,
    /// Index of the focus-ring overlay panel, created lazily on the first
    /// `set_focus(Some(_))` call. Lives at root with `band = u32::MAX` so it
    /// always renders on top of everything else.
    pub(crate) focus_ring_idx: Option<usize>,
    /// Set to `true` by `flush_all` once the ring node has been assigned a
    /// vertex slot. Guards `mark_dirty(ring_idx)` calls in `set_focus` so we
    /// never patch a stale or uninitialized `vertex_offset`.
    ring_has_slot: bool,
    /// When `Some(scope)`, Tab cycles only within that node's subtree. Set to
    /// the innermost orderable (`z_index > 0`) ancestor when the user clicks
    /// any descendant of an orderable node; cleared when clicking outside any
    /// orderable node or when the scoped node is hidden. Regardless of scope,
    /// `collect_focusable` never descends into orderable nodes other than the
    /// scope root — those form their own independent Tab scopes.
    tab_scope: Option<usize>,
    /// The node currently capturing raw keyboard input, if any — see
    /// [`start_key_capture`](Self::start_key_capture). While `Some`,
    /// [`handle_input`](Self::handle_input) bypasses hit-testing and the
    /// normal Tab/Enter/Escape handling entirely.
    capturing_node: Option<usize>,
    /// Nodes needing a vertex patch. Drained to empty by either `flush_all`
    /// or `flush_dirty`.
    dirty_nodes: Vec<usize>,

    /// Type-erased [`Navigator<S>`], set by
    /// [`init_navigation`](Self::init_navigation). `S` is the host's own
    /// screen type — `Ui` doesn't need to know what it is.
    navigator: Option<Box<dyn Any>>,

    /// Type-erased [`LayerOrder<L>`], lazily created by the first
    /// [`register_layer`](Self::register_layer) call. `L` is the host's own
    /// layer type — `Ui` doesn't need to know what it is.
    layer_order: Option<Box<dyn Any>>,

    // ── Events ────────────────────────────────────────────────────────────
    // Pushed by node callbacks (which only have `&mut Ui`, never `&mut Host`)
    // and drained by the host via `take_events` after each `handle_input` call.
    events: Vec<UiEvent>,
}

/// Appends a `[first_quad, first_quad + quad_count)` range to `batches`,
/// extending the last batch if it's contiguous and shares `clip`, or pushing
/// a new one otherwise. A no-op if `quad_count == 0`.
///
/// Merging is based on `clip` *value* equality, not on the two ranges
/// Merges require both a matching `clip` value and the same `clip_source`
/// (the `clip_children` ancestor that produced the rect). This prevents two
/// unrelated subtrees that coincidentally resolve to equal clip rects from
/// sharing a batch, which would let [`Ui::refresh_batch_clip`] silently
/// update the wrong subtree's scissor.
fn push_batch(batches: &mut Vec<DrawBatch>, clip: Option<Edges>, clip_source: Option<usize>, first_quad: usize, quad_count: usize) {
    if quad_count == 0 { return; }
    if let Some(last) = batches.last_mut()
        && last.clip_rect == clip && last.clip_source == clip_source
        && last.first_quad + last.quad_count == first_quad {
        last.quad_count += quad_count;
        return;
    }

    batches.push(DrawBatch { clip_rect: clip, clip_source, first_quad, quad_count });
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
            batches: Vec::new(),
            font_atlas: atlas,
            tree: UiTree::new(screen_size.0, screen_size.1),
            hovered_node: None,
            pressed_node: None,
            dragging_node: None,
            focused_node: None,
            focus_ring_idx: None,
            ring_has_slot: false,
            tab_scope: None,
            capturing_node: None,
            dirty_nodes: Vec::new(),
            navigator: None,
            layer_order: None,
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

    pub fn create_group(&mut self, parent: usize) -> Result<(usize, &mut GroupNode)> {
        let idx = self.tree.add_child(UiNode::Group(GroupNode::new()), parent)?;
        let c = self.tree.get_node_mut::<GroupNode>(idx)?;
        Ok((idx, c))
    }

    pub(crate) fn add_node(&mut self, node: UiNode, parent: usize) -> Result<usize> {
        self.tree.add_child(node, parent)
    }

    pub fn create_panel(&mut self, parent: usize) -> Result<(usize, &mut PanelNode)> {
        let idx = self.tree.add_child(UiNode::Panel(PanelNode::new()), parent)?;
        let p = self.tree.get_node_mut::<PanelNode>(idx)?;
        Ok((idx, p))
    }

    pub fn create_button(&mut self, parent: usize) -> Result<(usize, &mut ButtonNode)> {
        let idx = self.tree.add_child(UiNode::Button(ButtonNode::new()), parent)?;
        let b = self.tree.get_node_mut::<ButtonNode>(idx)?;
        Ok((idx, b))
    }

    pub fn create_label(&mut self, parent: usize) -> Result<(usize, &mut LabelNode)> {
        let cap_height = self.font_atlas.cap_height;
        let mut l = LabelNode::new("");
        l.base.set_height(cap_height);
        let idx = self.tree.add_child(UiNode::Label(l), parent)?;
        let l = self.tree.get_node_mut::<LabelNode>(idx)?;
        Ok((idx, l))
    }

    pub fn create_checkbox(&mut self, parent: usize) -> Result<(usize, &mut CheckboxNode)> {
        let idx = self.tree.add_child(UiNode::Checkbox(CheckboxNode::new()), parent)?;
        let c = self.tree.get_node_mut::<CheckboxNode>(idx)?;
        Ok((idx, c))
    }

    /// Also creates the slider's thumb (a [`ButtonNode`]) as a child and
    /// wires its index back into the returned `SliderNode`.
    pub fn create_slider(&mut self, parent: usize, axis: Axis) -> Result<(usize, &mut SliderNode)> {
        SliderNode::build(self, parent, axis)
    }

    /// Creates a floating window of the given size: a border/frame quad with
    /// a titlebar (holding a title label and a close button that hides the
    /// window) and an inset body panel for content. Content should be added
    /// under the returned `WindowNode::body`, e.g. via
    /// `ui.create_panel(window.body)`.
    pub fn create_window(&mut self, parent: usize, width: f32, height: f32) -> Result<(usize, &mut WindowNode)> {
        WindowNode::build(self, parent, width, height)
    }

    /// Creates a scroll panel: a scroll-enabled content [`PanelNode`]
    /// (`clip_children = true`), a [`SliderNode`] scrollbar, and
    /// decrement/increment [`ButtonNode`]s, grouped under one
    /// [`ScrollPanelNode`] so [`Ui::resize_scroll_panel`] can reposition/
    /// resize all four together. `viewport` is the visible content area;
    /// `content_size` is the total (virtual) content size being scrolled
    /// over. The scrollbar's `step_size` and all colors/textures are left at
    /// their defaults for the caller to set via the returned indices.
    pub fn create_scroll_panel(&mut self, parent: usize, axis: Axis, viewport: (f32, f32), scrollbar_width: f32, content_size: (f32, f32)) -> Result<(usize, &mut ScrollPanelNode)> {
        ScrollPanelNode::build(self, parent, axis, viewport, scrollbar_width, content_size)
    }

    /// The number of quads in the vertex buffer produced by the last
    /// [`flush_all`](Self::flush_all).
    pub fn quad_count(&self) -> usize {
        self.quad_count
    }

    /// Draw batches: contiguous quad ranges sharing a clip rect, in the same
    /// order as the vertex buffer. The host should issue one draw call per
    /// batch, applying `clip_rect` (e.g. via a push constant the fragment
    /// shader discards against) before drawing `[first_quad, first_quad +
    /// quad_count)`. Rebuilt by [`flush_all`](Self::flush_all); clip-rect
    /// values are kept current by [`flush_dirty`](Self::flush_dirty).
    pub fn batches(&self) -> &[DrawBatch] {
        &self.batches
    }

    /// The (color, texture) to render for `idx`, accounting for its
    /// hover/press state, or `None` for node types that don't render a quad
    /// of their own (containers, labels — labels are handled separately by
    /// [`LabelNode::quads`]).
    fn render_data(&self, idx: usize) -> Option<(Rgba, Texture)> {
        let hovered = self.hovered_node == Some(idx);
        let pressed = self.pressed_node == Some(idx);
        match &self.tree.nodes[idx] {
            UiNode::Panel(p)    => Some((p.renderable.color(), p.renderable.texture())),
            UiNode::Button(b)   => Some((b.display_color(hovered, pressed), b.display_texture(hovered, pressed))),
            UiNode::Checkbox(c) => Some((c.display_color(hovered, pressed), c.display_texture(hovered, pressed))),
            UiNode::Slider(s)   => Some((s.panel.renderable.color(), s.panel.renderable.texture())),
            UiNode::Window(w)   => Some((w.renderable.color(), w.renderable.texture())),
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
    pub(crate) fn flush_all(&mut self) -> UiUpdate {
        self.dirty = false;
        self.dirty_nodes.clear();
        self.ring_has_slot = false;
        let atlas = &*self.font_atlas;
        let mut verts: Vec<Vertex> = Vec::new();
        let mut batches: Vec<DrawBatch> = Vec::new();

        let root_edges = self.tree.nodes[0].base().resolve(&Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 }, &self.tree.nodes);

        // Each stack entry is a single node still to be processed (own quad,
        // then its children), along with the clip rect inherited from its
        // `clip_children` ancestors (`None` = unclipped). Children are pushed
        // in *reverse* `ordered_children` order so the LIFO stack pops them
        // back into the correct forward order — giving a full pre-order DFS
        // per subtree, so a node's entire subtree (not just its own quad)
        // stays grouped relative to its siblings' subtrees.
        let mut stack: Vec<(usize, Edges, Option<Edges>, Option<usize>)> = self.tree.ordered_children(0).into_iter().rev()
            .map(|idx| (idx, root_edges, None, None))
            .collect();

        while let Some((node_idx, parent_edges, clip, clip_source)) = stack.pop() {
            if !self.tree.nodes[node_idx].base().visible { continue; }

            let e = self.tree.nodes[node_idx].base().resolve(&parent_edges, &self.tree.nodes);

            match &self.tree.nodes[node_idx] {
                UiNode::Label(l) => {
                    let max_len = l.max_len();
                    let quads   = l.quads(atlas, e.left, e.bottom);

                    let quad_start = verts.len() / 4;
                    let slot = verts.len();
                    verts.extend(quads);
                    self.tree.nodes[node_idx].base_mut().vertex_offset = slot;
                    push_batch(&mut batches, clip, clip_source, quad_start, max_len);
                }
                _ => {
                    if let Some((color, texture)) = self.render_data(node_idx) {
                        let quad_start = verts.len() / 4;
                        let slot = verts.len();
                        verts.extend(quad_verts(&e, color, texture));
                        self.tree.nodes[node_idx].base_mut().vertex_offset = slot;
                        push_batch(&mut batches, clip, clip_source, quad_start, 1);
                        if Some(node_idx) == self.focus_ring_idx {
                            self.ring_has_slot = true;
                        }
                    }

                    let (child_clip, child_clip_source) = if self.tree.nodes[node_idx].clip_children() {
                        (Some(clip.map_or(e, |c| c.intersect(&e))), Some(node_idx))
                    } else {
                        (clip, clip_source)
                    };

                    let child_edges = match self.tree.nodes[node_idx].scroll() {
                        Some(s) => e.translate(-s.offset.0, -s.offset.1),
                        None    => e,
                    };

                    for child_idx in self.tree.ordered_children(node_idx).into_iter().rev() {
                        stack.push((child_idx, child_edges, child_clip, child_clip_source));
                    }
                }
            }
        }

        self.quad_count = verts.len() / 4;
        self.batches = batches;
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
    pub(crate) fn flush_dirty(&mut self) -> UiUpdate {
        let dirty: Vec<usize> = self.dirty_nodes.drain(..).collect();
        if dirty.is_empty() {
            return UiUpdate::None;
        }

        let focused_was_dirty = self.focused_node.is_some_and(|f| dirty.contains(&f));

        let mut patches: Vec<(usize, Vec<Vertex>)> = Vec::with_capacity(dirty.len());
        for node_idx in dirty {
            self.refresh_batch_clip(node_idx);
            let e = self.node_edges(node_idx);

            match &self.tree.nodes[node_idx] {
                UiNode::Label(l) => {
                    let offset   = self.tree.nodes[node_idx].base().vertex_offset;
                    let atlas    = &*self.font_atlas;
                    let vertices = l.quads(atlas, e.left, e.bottom);
                    patches.push((offset, vertices));
                }
                _ => {
                    if let Some((color, texture)) = self.render_data(node_idx) {
                        let offset = self.tree.nodes[node_idx].base().vertex_offset;
                        patches.push((offset, quad_verts(&e, color, texture).to_vec()));
                    }
                }
            }
        }

        // If the focused node moved this frame (e.g. its own bounds changed),
        // recompute the ring's local position and patch its vertex. Skip until
        // flush_all has assigned the ring a vertex slot (ring_has_slot guard).
        if focused_was_dirty
            && let Some(focused_idx) = self.focused_node
            && let Some(ring_idx)    = self.focus_ring_idx
            && self.ring_has_slot
        {
            let focused_e  = self.node_edges(focused_idx);
            let parent_idx = self.tree.nodes[ring_idx].base().parent.unwrap_or(0);
            let parent_abs = self.node_edges(parent_idx);
            let (sx, sy)   = self.tree.nodes[parent_idx].scroll()
                .map(|s| (s.offset.0, s.offset.1))
                .unwrap_or((0.0, 0.0));
            {
                let ring = self.tree.nodes[ring_idx].base_mut();
                ring.bounds.x      = focused_e.left   - (parent_abs.left - sx);
                ring.bounds.y      = focused_e.top    - (parent_abs.top  - sy);
                ring.bounds.width  = focused_e.right  - focused_e.left;
                ring.bounds.height = focused_e.bottom - focused_e.top;
            }
            if let Some((color, texture)) = self.render_data(ring_idx) {
                let ring_e = self.node_edges(ring_idx);
                let offset = self.tree.nodes[ring_idx].base().vertex_offset;
                patches.push((offset, quad_verts(&ring_e, color, texture).to_vec()));
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
            Some(p) => {
                let edges = self.node_edges(p);
                match self.tree.nodes[p].scroll() {
                    Some(s) => edges.translate(-s.offset.0, -s.offset.1),
                    None    => edges,
                }
            }
            None    => Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        };
        node.base().resolve(&parent_edges, &self.tree.nodes)
    }

    /// Computes the clip rect `node_idx` inherits from its `clip_children`
    /// ancestors, intersected from the outermost in (mirrors [`node_edges`](Self::node_edges)'s
    /// parent-chain walk). `None` means unclipped.
    fn node_resolved_clip(&self, node_idx: usize) -> Option<Edges> {
        let parent = self.tree.nodes[node_idx].base().parent?;
        let parent_clip = self.node_resolved_clip(parent);
        if self.tree.nodes[parent].clip_children() {
            let parent_edges = self.node_edges(parent);
            Some(parent_clip.map_or(parent_edges, |c| c.intersect(&parent_edges)))
        } else {
            parent_clip
        }
    }

    /// Updates the `clip_rect` of whichever batch in `self.batches` covers
    /// `node_idx`'s vertex range, to reflect its current
    /// [`node_resolved_clip`](Self::node_resolved_clip). Called by
    /// [`flush_dirty`](Self::flush_dirty) for each dirtied node; a no-op if
    /// the node's batch already has the right clip rect (redundant calls from
    /// sibling/descendant nodes in the same batch are harmless).
    fn refresh_batch_clip(&mut self, node_idx: usize) {
        let quad_idx = self.tree.nodes[node_idx].base().vertex_offset / 4;
        let clip = self.node_resolved_clip(node_idx);
        for batch in &mut self.batches {
            if quad_idx >= batch.first_quad && quad_idx < batch.first_quad + batch.quad_count {
                batch.clip_rect = clip;
                break;
            }
        }
    }

    /// Sums glyph advances to get the rendered width of `text`.
    pub fn label_width(&self, text: &str) -> f32 {
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

    /// Resolves a hit on a window's titlebar to the window's own index.
    fn window_titlebar_at(&self, idx: usize) -> Option<usize> {
        let parent = self.tree.nodes[idx].base().parent?;
        match &self.tree.nodes[parent] {
            UiNode::Window(w) if w.titlebar == idx => Some(parent),
            _ => None,
        }
    }

    /// Walks up from `idx` (inclusive) to the nearest ancestor with
    /// scrolling enabled, for routing scroll-wheel input to the right panel.
    fn scrollable_ancestor(&self, idx: usize) -> Option<usize> {
        let mut current = idx;
        loop {
            if self.tree.nodes[current].scroll().is_some() {
                return Some(current);
            }
            current = self.tree.nodes[current].base().parent?;
        }
    }

    /// The pixel distance to scroll `scroll_idx` per wheel "line", per axis.
    /// See [`Scroll::line_step`].
    fn line_scroll_step(&self, scroll_idx: usize) -> (f32, f32) {
        let scrollbar = self.tree.nodes[scroll_idx].scroll().and_then(|s| s.scrollbar);
        let scrollbar = scrollbar.and_then(|idx| self.tree.get_node::<SliderNode>(idx).ok());
        Scroll::line_step(scrollbar)
    }

    /// Marks `idx` and, recursively, all of its descendants dirty for the
    /// next [`flush_dirty`](Self::flush_dirty) patch. Used when a node's
    /// position changes in a way that shifts every descendant's resolved
    /// edges (e.g. dragging a window, or scrolling via
    /// [`PanelNode::set_scroll_offset`]/[`PanelNode::scroll_by`] — call this
    /// after mutating a fetched panel's scroll offset directly).
    ///
    /// Nodes that don't occupy a vertex slot of their own (`Group`s,
    /// whose `render_data` is `None`) are skipped: their `vertex_offset`
    /// stays at its default `0`, so [`refresh_batch_clip`](Self::refresh_batch_clip)
    /// would mistarget batch `0` for them. Their children are still
    /// recursed into and added if those render their own quad.
    pub(crate) fn mark_dirty(&mut self, idx: usize) {
        if matches!(self.tree.nodes[idx], UiNode::Label(_)) || self.render_data(idx).is_some() {
            self.dirty_nodes.push(idx);
        }
        let children: Vec<usize> = self.tree.nodes[idx].children().map(|c| c.to_vec()).unwrap_or_default();
        for child in children {
            self.mark_dirty(child);
        }
    }

    /// Sets whether `idx`'s children (and their whole subtrees) are clipped
    /// to `idx`'s resolved bounds; see [`UiNode::clip_children`]. Only valid
    /// for container-like nodes (`Group`, `Panel`, `ScrollPanel`, `Window`).
    /// Changes draw-batch boundaries, so marks the whole tree dirty for the
    /// next [`flush_all`](Self::flush_all).
    pub(crate) fn set_clip_children(&mut self, idx: usize, clip: bool) -> Result<()> {
        let node = self.tree.nodes.get_mut(idx)
            .ok_or_else(|| anyhow!("UI node index {idx} out of bounds"))?;
        *node.clip_children_mut()
            .ok_or_else(|| anyhow!("UI node {idx} cannot clip children"))? = clip;
        self.dirty = true;
        Ok(())
    }

    /// Sets whether dragging one of `idx`'s children clamps its position so
    /// its resolved edges stay within `idx`'s resolved edges; see
    /// [`UiNode::clamp_children`]. Only valid for container-like nodes
    /// (`Group`, `Panel`, `ScrollPanel`, `Window`).
    pub fn set_clamp_children(&mut self, idx: usize, clamp: bool) -> Result<()> {
        let node = self.tree.nodes.get_mut(idx)
            .ok_or_else(|| anyhow!("UI node index {idx} out of bounds"))?;
        *node.clamp_children_mut()
            .ok_or_else(|| anyhow!("UI node {idx} cannot clamp children"))? = clamp;
        Ok(())
    }

    /// Repositions the thumb to match the slider's current value, and
    /// resizes the step buttons (if any) to stay square with the track's
    /// current cross-axis extent. Marks them dirty for re-rendering. Hosts
    /// call this after changing a slider's value, range, or track size from
    /// their own code (e.g. an `on_show` callback that re-syncs the slider to
    /// external state, or after resizing the track set up by
    /// [`Ui::create_slider`]).
    pub fn layout_slider(&mut self, slider_idx: usize) -> Result<()> {
        SliderNode::layout(self, slider_idx)
    }

    /// After `scroll_idx`'s offset changes via scroll-wheel input, syncs its
    /// [`Scroll::scrollbar`] slider (if any) to match: sets the slider's
    /// value to the offset component along the slider's own [`Axis`], and
    /// re-lays-out its thumb. A no-op if scrolling isn't enabled or no
    /// scrollbar is set.
    fn sync_scrollbar(&mut self, scroll_idx: usize) -> Result<()> {
        let Some(scroll) = self.tree.nodes[scroll_idx].scroll() else { return Ok(()) };
        let Some(slider_idx) = scroll.scrollbar else { return Ok(()) };
        let offset = scroll.offset;

        let axis = self.tree.get_node::<SliderNode>(slider_idx)?.axis();
        let value = match axis {
            Axis::Horizontal => offset.0,
            Axis::Vertical   => offset.1,
        };

        let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
        s.set_value(value.round() as u32);
        self.layout_slider(slider_idx)
    }

    /// Lays out a [`ScrollPanelNode`]'s content panel, scrollbar track, and
    /// step buttons for the given `content_size`, deriving the viewport from
    /// the frame's current `base.bounds` minus its (fixed) `scrollbar_width`
    /// along `axis`. Shared by [`Ui::create_scroll_panel`] and
    /// [`Ui::resize_scroll_panel`].
    pub(crate) fn layout_scroll_panel(&mut self, frame_idx: usize, content_size: (f32, f32)) -> Result<()> {
        let (axis, scrollbar_width, content_idx, scrollbar_idx, dec_idx, inc_idx, frame_size) = {
            let f = self.get_node::<ScrollPanelNode>(frame_idx)?;
            (f.axis, f.scrollbar_width, f.content_idx, f.scrollbar_idx, f.dec_idx, f.inc_idx, (f.base.bounds.width, f.base.bounds.height))
        };

        let viewport = match axis {
            Axis::Vertical   => (frame_size.0 - scrollbar_width, frame_size.1),
            Axis::Horizontal => (frame_size.0, frame_size.1 - scrollbar_width),
        };

        let content = self.get_node_mut::<PanelNode>(content_idx)?;
        content.base.bounds = Rect { x: 0.0, y: 0.0, width: viewport.0, height: viewport.1 };
        content.set_content_size(content_size);

        // The track spans the full main-axis extent; the dec/inc buttons sit
        // on top of its two ends (drawn after it, so already on top), inset
        // by half the thumb padding so the track's color frames them the
        // same way it frames the thumb.
        let track_bounds = match axis {
            Axis::Vertical   => Rect { x: viewport.0, y: 0.0, width: scrollbar_width, height: viewport.1 },
            Axis::Horizontal => Rect { x: 0.0, y: viewport.1, width: viewport.0, height: scrollbar_width },
        };
        self.get_node_mut::<SliderNode>(scrollbar_idx)?.panel.base.bounds = track_bounds;

        let padding_half = SCROLLBAR_THUMB_PADDING / 2.0;
        let button_size  = (scrollbar_width - SCROLLBAR_THUMB_PADDING).max(0.0);
        self.get_node_mut::<ButtonNode>(dec_idx)?.base.set_size(button_size, button_size);
        self.get_node_mut::<ButtonNode>(inc_idx)?.base.set_size(button_size, button_size);
        match axis {
            Axis::Vertical => {
                self.get_node_mut::<ButtonNode>(dec_idx)?.base.set_position_anchored_to(Anchor::Top, scrollbar_idx, Anchor::Top, 0.0, padding_half);
                self.get_node_mut::<ButtonNode>(inc_idx)?.base.set_position_anchored_to(Anchor::Bottom, scrollbar_idx, Anchor::Bottom, 0.0, -padding_half);
            }
            Axis::Horizontal => {
                self.get_node_mut::<ButtonNode>(dec_idx)?.base.set_position_anchored_to(Anchor::Left, scrollbar_idx, Anchor::Left, padding_half, 0.0);
                self.get_node_mut::<ButtonNode>(inc_idx)?.base.set_position_anchored_to(Anchor::Right, scrollbar_idx, Anchor::Right, -padding_half, 0.0);
            }
        }

        let (bar_extent, content_extent, viewport_extent) = match axis {
            Axis::Vertical   => (track_bounds.height, content_size.1, viewport.1),
            Axis::Horizontal => (track_bounds.width, content_size.0, viewport.0),
        };
        let max_offset = (content_extent - viewport_extent).max(0.0);
        // Keep the thumb's travel clear of the dec/inc buttons' slots
        // (`scrollbar_width` each) plus the same small gap used elsewhere.
        let track_padding = scrollbar_width + padding_half;
        let scrollbar = self.get_node_mut::<SliderNode>(scrollbar_idx)?;
        scrollbar.set_min_max(0, max_offset.round() as u32);
        scrollbar.set_track_padding(track_padding);

        if let Some(thumb_idx) = self.get_node::<SliderNode>(scrollbar_idx)?.get_thumb() {
            let usable_extent = (bar_extent - 2.0 * track_padding).max(0.0);
            let thumb_extent = (usable_extent * usable_extent / content_extent.max(1.0)).max(button_size);
            let thumb = self.get_node_mut::<ButtonNode>(thumb_idx)?;
            match axis {
                Axis::Vertical   => { thumb.base.set_height(thumb_extent); thumb.base.set_width(button_size); }
                Axis::Horizontal => { thumb.base.set_width(thumb_extent); thumb.base.set_height(button_size); }
            }
        }

        self.mark_dirty(frame_idx);
        self.sync_scrollbar(content_idx)
    }

    /// Re-lays-out a scroll panel's content/scrollbar/step buttons after its
    /// overall size has changed via `frame.base.set_size(...)` (the normal
    /// [`NodeBase::set_size`] resize API — there's no scroll-panel-specific
    /// size setter) and/or its `content_size` has changed. Re-derives the
    /// viewport from the frame's (already-updated) `base.bounds` and its
    /// fixed `scrollbar_width`, resizes/re-clamps the content panel,
    /// repositions the scrollbar track and step buttons, and recomputes the
    /// scrollbar's range and thumb size.
    pub(crate) fn resize_scroll_panel(&mut self, frame_idx: usize, content_size: (f32, f32)) -> Result<()> {
        self.layout_scroll_panel(frame_idx, content_size)
    }

    /// Sets a scroll panel's offset (see [`PanelNode::set_scroll_offset`]),
    /// then marks it dirty for the next [`flush_dirty`](Self::flush_dirty)
    /// and syncs its [`Scroll::scrollbar`] (if any) to match. Hosts should
    /// call this instead of mutating a fetched [`PanelNode`]'s scroll state
    /// directly, so the dirty-marking and scrollbar sync can't be forgotten.
    pub(crate) fn set_scroll_offset(&mut self, idx: usize, offset: (f32, f32)) -> Result<()> {
        let panel = self.tree.get_node_mut::<PanelNode>(idx)?;
        panel.set_scroll_offset(offset);
        self.mark_dirty(idx);
        self.sync_scrollbar(idx)
    }

    /// Scrolls `node_idx`'s nearest scrollable ancestor just enough to make
    /// `node_idx` fully visible within the viewport. A no-op if the node is
    /// already fully visible, has no scrollable ancestor, or its
    /// [`NodeBase::cached_edges`] have not yet been written by a
    /// [`flush_all`](Self::flush_all) call.
    fn scroll_into_view(&mut self, node_idx: usize) -> Result<()> {
        let Some(scroll_idx) = self.scrollable_ancestor(node_idx) else { return Ok(()) };
        let node_e = self.node_edges(node_idx);
        let viewport = self.node_edges(scroll_idx);

        let dx = if node_e.left < viewport.left {
            node_e.left - viewport.left
        } else if node_e.right > viewport.right {
            node_e.right - viewport.right
        } else {
            0.0
        };

        let dy = if node_e.top < viewport.top {
            node_e.top - viewport.top
        } else if node_e.bottom > viewport.bottom {
            node_e.bottom - viewport.bottom
        } else {
            0.0
        };

        if dx != 0.0 || dy != 0.0 {
            self.scroll_by(scroll_idx, (dx, dy))?;
        }
        Ok(())
    }

    /// Adjusts a scroll panel's offset by `(dx, dy)`, clamped (see
    /// [`PanelNode::scroll_by`]). See [`Ui::set_scroll_offset`] re:
    /// dirty-marking and scrollbar sync.
    pub(crate) fn scroll_by(&mut self, idx: usize, delta: (f32, f32)) -> Result<()> {
        let panel = self.tree.get_node_mut::<PanelNode>(idx)?;
        panel.scroll_by(delta);
        self.mark_dirty(idx);
        self.sync_scrollbar(idx)
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

    /// Sets a window's frame color: both its own (border) quad and its
    /// titlebar, which share a color by convention. Marks both dirty for an
    /// in-place [`flush_dirty`](Self::flush_dirty) patch.
    pub(crate) fn set_window_color(&mut self, idx: usize, color: Rgba) -> Result<()> {
        let titlebar_idx = self.get_node::<WindowNode>(idx)?.titlebar;
        self.get_node_mut::<WindowNode>(idx)?.set_color(color);
        self.get_node_mut::<PanelNode>(titlebar_idx)?.set_color(color);
        self.dirty_nodes.push(idx);
        self.dirty_nodes.push(titlebar_idx);
        Ok(())
    }

    /// Sets a window's background color, i.e. its [`WindowNode::body`] panel.
    /// Marks it dirty for an in-place [`flush_dirty`](Self::flush_dirty) patch.
    pub(crate) fn set_window_background_color(&mut self, idx: usize, color: Rgba) -> Result<()> {
        let body_idx = self.get_node::<WindowNode>(idx)?.body;
        self.get_node_mut::<PanelNode>(body_idx)?.set_color(color);
        self.dirty_nodes.push(body_idx);
        Ok(())
    }

    /// Recomputes the slider's value from the cursor position relative to
    /// where the drag started (see [`SliderNode::drag_to`]), then re-lays-out
    /// the thumb. Fires `on_value_changed` if the value changed.
    fn drag_slider(&mut self, slider_idx: usize, cursor: (f32, f32)) -> Result<()> {
        SliderNode::apply_drag(self, slider_idx, cursor)
    }

    /// Adjusts a slider's value by one [`SliderNode::step_size`] (see
    /// [`SliderNode::step`]) — up if `increase`, down otherwise — then
    /// re-lays-out the thumb and fires `on_value_changed` if the value
    /// changed. Wired to a scroll panel's dec/inc buttons (see
    /// [`Ui::create_scroll_panel`]); also callable directly by hosts, e.g.
    /// for keyboard shortcuts.
    pub(crate) fn step_slider(&mut self, slider_idx: usize, increase: bool) -> Result<()> {
        let changed = self.tree.get_node_mut::<SliderNode>(slider_idx)?.step(increase);

        self.layout_slider(slider_idx)?;

        if changed {
            self.fire_slider_value_changed(slider_idx)?;
        }

        Ok(())
    }

    /// Jumps a slider to its minimum (`to_max = false`) or maximum (`to_max =
    /// true`) value, then re-lays-out the thumb and fires `on_value_changed`
    /// if the value changed. Used by Ctrl+Arrow key on a focused slider.
    pub(crate) fn jump_slider(&mut self, slider_idx: usize, to_max: bool) -> Result<()> {
        let target = {
            let s = self.tree.get_node::<SliderNode>(slider_idx)?;
            if to_max { s.max_value() } else { s.min_value() }
        };
        let old = self.tree.get_node::<SliderNode>(slider_idx)?.value;
        self.tree.get_node_mut::<SliderNode>(slider_idx)?.set_value(target);
        self.layout_slider(slider_idx)?;
        if self.tree.get_node::<SliderNode>(slider_idx)?.value != old {
            self.fire_slider_value_changed(slider_idx)?;
        }
        Ok(())
    }

    /// Like [`fire_callback`](Self::fire_callback), but for
    /// [`SliderNode::on_value_changed`]. Fired by [`drag_slider`](Self::drag_slider)
    /// and the track-click handler in [`handle_input`](Self::handle_input)
    /// when an interactive change actually moves the value.
    pub(crate) fn fire_slider_value_changed(&mut self, slider_idx: usize) -> Result<()> {
        let UiNode::Slider(s) = &mut self.tree.nodes[slider_idx] else { return Ok(()) };
        let Some(mut callback) = s.on_value_changed.take() else { return Ok(()) };
        callback(self);
        let UiNode::Slider(s) = &mut self.tree.nodes[slider_idx] else { unreachable!() };
        s.on_value_changed = Some(callback);
        Ok(())
    }

    /// Repositions a window by the cursor's delta from where the drag
    /// started, then marks the window and its whole subtree dirty so every
    /// descendant's quad is repatched at its new resolved position. If the
    /// window's parent has [`UiNode::clamp_children`] set, the new position
    /// is clamped so the window's resolved edges stay within its parent's
    /// resolved edges (or its `content_size`, if the parent is a scroll
    /// panel — see [`clamp_to_parent`](Self::clamp_to_parent)).
    fn drag_window(&mut self, window_idx: usize, cursor: (f32, f32)) -> Result<()> {
        self.tree.get_node_mut::<WindowNode>(window_idx)?.drag_to(cursor);

        let parent = self.tree.nodes[window_idx].base().parent;
        if parent.is_some_and(|p| self.tree.nodes[p].clamp_children()) {
            self.clamp_to_parent(window_idx);
        }

        self.mark_dirty(window_idx);
        Ok(())
    }

    /// Shifts `idx`'s position so its resolved edges stay within its
    /// parent's resolved edges, shrinking-to-fit (anchored at the parent's
    /// top-left) if `idx` is larger than its parent along an axis. A no-op
    /// if `idx` is the root (no parent). If the parent is a scroll panel
    /// (see [`UiNode::scroll`]), clamps to its full `content_size` instead
    /// of its viewport — `idx`'s resolved edges are already in the content's
    /// scrolled coordinate space, so the content rect is the parent's
    /// (offset-translated) top-left extended by `content_size`.
    fn clamp_to_parent(&mut self, idx: usize) {
        let Some(parent) = self.tree.nodes[idx].base().parent else { return };
        let parent_edges = self.node_edges(parent);
        let parent_edges = match self.tree.nodes[parent].scroll() {
            Some(scroll) => {
                let origin = parent_edges.translate(-scroll.offset.0, -scroll.offset.1);
                Edges {
                    left:   origin.left,
                    top:    origin.top,
                    right:  origin.left + scroll.content_size.0,
                    bottom: origin.top + scroll.content_size.1,
                }
            }
            None => parent_edges,
        };
        let edges = self.node_edges(idx);

        let width  = edges.right  - edges.left;
        let height = edges.bottom - edges.top;

        let left = if width <= parent_edges.right - parent_edges.left {
            edges.left.clamp(parent_edges.left, parent_edges.right - width)
        } else {
            parent_edges.left
        };
        let top = if height <= parent_edges.bottom - parent_edges.top {
            edges.top.clamp(parent_edges.top, parent_edges.bottom - height)
        } else {
            parent_edges.top
        };

        // `resolve` is linear in `bounds.x`/`bounds.y` with slope 1, so a
        // shift in resolved edges maps 1:1 onto `bounds`.
        let base = self.tree.nodes[idx].base_mut();
        base.bounds.x += left - edges.left;
        base.bounds.y += top  - edges.top;
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

        // A focused/capturing node that's about to be hidden would otherwise
        // keep its index in `focused_node`/`capturing_node` with a now-stale
        // `vertex_offset` (the next `flush_all` skips invisible subtrees
        // without updating it) - a later `set_focus` could then misdirect
        // `refresh_batch_clip` at whatever batch that stale offset now falls
        // into. Clear them up front instead, same as `hovered_node` above.
        if !visible {
            if let Some(focused) = self.focused_node
                && self.is_or_contains(idx, focused)
            {
                self.set_focus(None);
            }
            if let Some(capturing) = self.capturing_node
                && self.is_or_contains(idx, capturing)
            {
                self.capturing_node = None;
            }
            if let Some(scope) = self.tab_scope
                && self.is_or_contains(idx, scope)
            {
                self.tab_scope = None;
            }
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

    /// Like [`fire_interaction`](Self::fire_interaction), but for
    /// [`InteractionCb::on_key_capture`], which takes the captured key name
    /// as an extra argument.
    fn fire_key_capture(&mut self, node_idx: usize, key: &str) -> Result<()> {
        let interaction = match &mut self.tree.nodes[node_idx] {
            UiNode::Button(b)   => &mut b.interaction,
            UiNode::Checkbox(c) => &mut c.interaction,
            _ => return Ok(()),
        };
        let Some(mut callback) = interaction.on_key_capture.take() else { return Ok(()) };
        callback(self, key);
        let interaction = match &mut self.tree.nodes[node_idx] {
            UiNode::Button(b)   => &mut b.interaction,
            UiNode::Checkbox(c) => &mut c.interaction,
            _ => unreachable!(),
        };
        interaction.on_key_capture = Some(callback);
        Ok(())
    }

    /// Changes [`focused_node`](Self::focused_node) to `new`. Repositions the
    /// focus-ring overlay over the new node (creating it lazily on first use),
    /// or hides it when `new` is `None`. No-op if `new` is already focused.
    fn set_focus(&mut self, new: Option<usize>) {
        if self.focused_node == new {
            return;
        }
        self.focused_node = new;

        if let Some(idx) = new {
            let _ = self.scroll_into_view(idx);

            // The ring lives as a sibling of the focused node (same parent).
            // This gives it the correct clip ancestry and z-order: it renders
            // after the focused node in insertion order (z_index=0, added last),
            // so it overlays the focused node but is occluded by any higher-z
            // siblings such as nested windows.
            let focused_parent = self.tree.nodes[idx].base().parent.unwrap_or(0);

            // Lazily allocate the ring node on the first focus event.
            // Parent and children-list wiring happen in the block below.
            if self.focus_ring_idx.is_none() {
                let ring_idx = self.tree.nodes.len();
                let mut node = UiNode::Panel(PanelNode::new());
                node.base_mut().interactive = false;
                if let UiNode::Panel(p) = &mut node {
                    p.renderable.set_color(Rgba::new(0.0, 0.0, 0.0, 0.0));
                }
                self.tree.nodes.push(node);
                self.focus_ring_idx = Some(ring_idx);
            }

            if let Some(ring_idx) = self.focus_ring_idx {
                // Remove the ring from wherever it currently lives.
                if let Some(old_parent) = self.tree.nodes[ring_idx].base().parent {
                    if let Some(ch) = self.tree.nodes[old_parent].children_mut() {
                        ch.retain(|&c| c != ring_idx);
                    }
                }

                // Insert ring immediately after the focused node so it renders
                // directly on top of its button. Siblings with z_index >= 1
                // (sub-windows) sort after all z=0 nodes → they correctly
                // occlude the ring regardless of unregistered z=0 sub-windows.
                let n = self.tree.nodes[focused_parent].children().map(|ch| ch.len()).unwrap_or(0);
                let insert_pos = self.tree.nodes[focused_parent].children()
                    .and_then(|ch| ch.iter().position(|&c| c == idx))
                    .map(|p| p + 1)
                    .unwrap_or(n);
                if let Some(ch) = self.tree.nodes[focused_parent].children_mut() {
                    ch.insert(insert_pos, ring_idx);
                }
                self.tree.nodes[ring_idx].base_mut().parent = Some(focused_parent);
                // Vertex order changed → always rebuild.
                self.dirty = true;

                // Express the ring's position as LOCAL coords relative to
                // focused_parent's reference point (parent edges adjusted for
                // scroll), so it automatically tracks the focused node when
                // the parent moves or scrolls.
                let focused_e  = self.node_edges(idx);
                let parent_abs = self.node_edges(focused_parent);
                let (sx, sy)   = self.tree.nodes[focused_parent].scroll()
                    .map(|s| (s.offset.0, s.offset.1))
                    .unwrap_or((0.0, 0.0));
                {
                    let ring = self.tree.nodes[ring_idx].base_mut();
                    ring.bounds.x      = focused_e.left   - (parent_abs.left - sx);
                    ring.bounds.y      = focused_e.top    - (parent_abs.top  - sy);
                    ring.bounds.width  = focused_e.right  - focused_e.left;
                    ring.bounds.height = focused_e.bottom - focused_e.top;
                }
                if let UiNode::Panel(p) = &mut self.tree.nodes[ring_idx] {
                    p.renderable.set_color(Rgba::new(0.3, 0.6, 1.0, 0.35));
                }
                // dirty=true above means flush_all is coming; it positions the
                // ring from ring.bounds — no mark_dirty needed here.
            }
        } else if let Some(ring_idx) = self.focus_ring_idx {
            if let UiNode::Panel(p) = &mut self.tree.nodes[ring_idx] {
                p.renderable.set_color(Rgba::new(0.0, 0.0, 0.0, 0.0));
            }
            if self.ring_has_slot {
                self.mark_dirty(ring_idx);
            }
        }
    }

    /// All [`UiNode::focusable`] nodes, in Tab order: a depth-first walk from
    /// the root via [`ordered_children`](Self::ordered_children) (the same
    /// order used for rendering/hit-testing), skipping a node — and its
    /// whole subtree — if `!base().visible` (matching
    /// [`hit_test`](UiTree::hit_test)'s early return).
    fn focusable_nodes(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let root = match self.tab_scope {
            Some(scope) if self.tree.nodes[scope].base().visible => scope,
            _ => 0,
        };
        self.collect_focusable(root, &mut out);
        out
    }

    fn collect_focusable(&self, idx: usize, out: &mut Vec<usize>) {
        if !self.tree.nodes[idx].base().visible {
            return;
        }
        if self.tree.nodes[idx].focusable() {
            out.push(idx);
        }
        for child in self.tree.ordered_children(idx) {
            // Orderable nodes (z_index > 0) and Window nodes each form their
            // own Tab scope — never descend into them from outside. Tab enters
            // one only when it IS the scope root (started from `tab_scope`).
            // This prevents the close button of a nested window from appearing
            // in the parent window's Tab order.
            if self.tree.nodes[child].base().z_index > 0
                || matches!(self.tree.nodes[child], UiNode::Window(_))
            {
                continue;
            }
            self.collect_focusable(child, out);
        }
    }

    /// Moves keyboard focus to the next [`UiNode::focusable`] node in Tab
    /// order (see [`focusable_nodes`](Self::focusable_nodes)), wrapping
    /// around to the first. If nothing is currently focused (or the focused
    /// node is no longer focusable/visible), focuses the first one. No-op if
    /// there are no focusable nodes.
    pub(crate) fn focus_next(&mut self) {
        let nodes = self.focusable_nodes();
        if nodes.is_empty() {
            return;
        }
        let next = match self.focused_node.and_then(|idx| nodes.iter().position(|&n| n == idx)) {
            Some(pos) => nodes[(pos + 1) % nodes.len()],
            None => nodes[0],
        };
        self.set_focus(Some(next));
    }

    /// Like [`focus_next`](Self::focus_next), but moves to the previous
    /// focusable node, wrapping around to the last. If nothing is currently
    /// focused (or the focused node is no longer focusable/visible), focuses
    /// the last one.
    pub(crate) fn focus_prev(&mut self) {
        let nodes = self.focusable_nodes();
        if nodes.is_empty() {
            return;
        }
        let prev = match self.focused_node.and_then(|idx| nodes.iter().position(|&n| n == idx)) {
            Some(pos) => nodes[(pos + nodes.len() - 1) % nodes.len()],
            None => nodes[nodes.len() - 1],
        };
        self.set_focus(Some(prev));
    }

    /// Activates the focused node as if it had been clicked: toggles
    /// [`CheckboxNode::selected`] (and marks it dirty) if `idx` is a
    /// checkbox, then fires `on_pressed` followed by `on_release`. Used by
    /// [`handle_input`](Self::handle_input) for Enter/Space activation of
    /// [`focused_node`](Self::focused_node).
    fn activate_focused(&mut self, idx: usize) -> Result<()> {
        if let UiNode::Checkbox(c) = &mut self.tree.nodes[idx] {
            c.selected = !c.selected;
            self.dirty_nodes.push(idx);
        }
        self.fire_interaction(idx, |i| &mut i.on_pressed)?;
        self.fire_interaction(idx, |i| &mut i.on_release)?;
        Ok(())
    }

    /// Enters key-capture mode for `idx`, and gives it keyboard focus. Until
    /// [`end_key_capture`](Self::end_key_capture) is called (or the user
    /// presses Escape, which cancels capture automatically),
    /// [`handle_input`](Self::handle_input) bypasses hit-testing and
    /// Tab/Enter/Escape handling, instead delivering
    /// [`UiInput::captured_key`] to `idx`'s
    /// [`InteractionCb::on_key_capture`].
    pub(crate) fn start_key_capture(&mut self, idx: usize) {
        self.capturing_node = Some(idx);
        self.set_focus(Some(idx));
    }

    /// Exits key-capture mode, restoring normal input handling. Call this
    /// from an `on_key_capture` callback once it's done with the key, or it
    /// happens automatically when the user presses Escape.
    pub(crate) fn end_key_capture(&mut self) {
        if let Some(idx) = self.capturing_node.take()
            && matches!(self.tree.nodes[idx], UiNode::Button(_) | UiNode::Checkbox(_)) {
            self.dirty_nodes.push(idx);
        }
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

    /// Associates `screen` with the Group/Panel node `idx`, so
    /// [`navigate_to_screen`](Self::navigate_to_screen) can show/hide it.
    pub fn register_screen<S: Copy + Eq + Hash + 'static>(&mut self, screen: S, idx: usize) -> Result<()> {
        if !matches!(self.tree.nodes.get(idx), Some(UiNode::Group(_) | UiNode::Panel(_))) {
            return Err(anyhow!("navigation target {idx} is not a Group or Panel"));
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

    // ── Layering / z-order ───────────────────────────────────────────────────

    /// Borrows the layer order as `LayerOrder<L>`, lazily creating it on
    /// first use. `L` is the host's own layer type; all
    /// [`register_layer`](Self::register_layer) calls for the lifetime of
    /// this `Ui` must use the same `L`.
    fn layer_order_mut<L: Copy + Eq + Hash + 'static>(&mut self) -> Result<&mut LayerOrder<L>> {
        if self.layer_order.is_none() {
            self.layer_order = Some(Box::new(LayerOrder::<L>::new()));
        }
        self.layer_order.as_mut()
            .and_then(|l| l.downcast_mut())
            .ok_or_else(|| anyhow!("layer ordering already initialized for a different layer type"))
    }

    /// Assigns the Group/Panel/Slider node `idx` (a direct child of the
    /// root) to `layer`'s band. Bands are assigned in registration order:
    /// the first distinct `layer` value seen across all `register_layer`
    /// calls becomes band `0` (bottom-most), the next becomes `1`, and so
    /// on — e.g. registering normal content before a debug overlay's layer
    /// ensures the overlay always renders and hit-tests on top, regardless
    /// of `z_index`. `L` is the host's own layer type, independent of the
    /// `S` used for [`init_navigation`](Self::init_navigation).
    pub fn register_layer<L: Copy + Eq + Hash + 'static>(&mut self, idx: usize, layer: L) -> Result<()> {
        if self.tree.nodes[idx].base().parent != Some(0) {
            return Err(anyhow!("layer target {idx} is not a direct child of the root"));
        }
        let band = self.layer_order_mut::<L>()?.band(layer);
        self.tree.nodes[idx].base_mut().band = band;
        self.dirty = true;
        Ok(())
    }

    /// Reassigns `idx`'s [`z_index`](NodeBase::z_index) from its parent's
    /// `z_sentinel`, placing it above all of its current siblings.
    fn bump_z_index(&mut self, idx: usize) -> Result<()> {
        let parent = self.tree.nodes[idx].base().parent
            .ok_or_else(|| anyhow!("node {idx} has no parent"))?;
        let sentinel = self.tree.nodes[parent].z_sentinel_mut()
            .ok_or_else(|| anyhow!("parent {parent} of node {idx} cannot have children"))?;
        let z = *sentinel;
        *sentinel += 1;
        self.tree.nodes[idx].base_mut().z_index = z;
        Ok(())
    }

    /// Marks `idx` as participating in z-ordering among its siblings,
    /// placing it above any sibling registered so far. Call once per node
    /// during setup, in the desired initial back-to-front order — later
    /// calls to [`raise`](Self::raise) (on press) will only affect nodes
    /// registered this way; siblings that are never registered keep
    /// [`z_index`](NodeBase::z_index) `0` and always sort below registered
    /// ones.
    pub fn register_orderable(&mut self, idx: usize) -> Result<()> {
        self.bump_z_index(idx)
    }

    /// Raises `idx` to the front of its parent's orderable siblings, and
    /// propagates the same operation up through every container-like
    /// ancestor to the root — each level only competes with its own
    /// siblings (CSS stacking-context semantics). A no-op at any level whose
    /// node has [`z_index`](NodeBase::z_index) `0`, i.e. was never
    /// [`register_orderable`](Self::register_orderable)d: such nodes are
    /// never reordered, so e.g. a debug overlay that never registers stays
    /// fixed regardless of clicks elsewhere. Called automatically by
    /// [`handle_input`](Self::handle_input) on press.
    fn raise(&mut self, idx: usize) -> Result<()> {
        let mut current = idx;
        let mut innermost_window: Option<usize> = None;
        loop {
            if self.tree.nodes[current].base().z_index > 0 {
                self.bump_z_index(current)?;
                self.dirty = true;
            }
            // Windows are Tab scope boundaries regardless of z_index — a
            // sub-window that isn't registered as orderable still scopes Tab.
            if matches!(self.tree.nodes[current], UiNode::Window(_)) && innermost_window.is_none() {
                innermost_window = Some(current);
            }
            match self.tree.nodes[current].base().parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        self.tab_scope = innermost_window;
        Ok(())
    }

    pub fn handle_input(&mut self, input: &UiInput) -> Result<()> {
        let cursor = input.cursor();

        if let Some(idx) = self.capturing_node {
            if input.key_pressed(Key::Escape) {
                self.end_key_capture();
            } else if let Some(key) = input.captured_key() {
                self.fire_key_capture(idx, key)?;
            }
            return Ok(());
        }

        if let Some(dragging_idx) = self.dragging_node {
            if input.primary_held() {
                match &self.tree.nodes[dragging_idx] {
                    UiNode::Slider(_) => self.drag_slider(dragging_idx, cursor)?,
                    UiNode::Window(_) => self.drag_window(dragging_idx, cursor)?,
                    _ => {}
                }
            } else {
                self.dragging_node = None;
            }
            return Ok(());
        }

        let hit = self.tree.hit_test(
            cursor.0, cursor.1, 0,
            &Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
            None,
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

        let scroll_delta = input.scroll_delta();
        if scroll_delta != (0.0, 0.0)
            && let Some(hovered) = self.hovered_node
            && let Some(scroll_idx) = self.scrollable_ancestor(hovered)
        {
            let (step_x, step_y) = self.line_scroll_step(scroll_idx);
            self.scroll_by(scroll_idx, (scroll_delta.0 * step_x, scroll_delta.1 * step_y))?;
        }

        match hit {
            Some(idx) => {
                if input.primary_pressed() {
                    self.raise(idx)?;
                    self.set_focus(None);

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
                            let (new_value, old_value) = {
                                let s = self.tree.get_node::<SliderNode>(slider_idx)?;
                                let axis = s.axis();
                                let thumb_extent = s.get_thumb().map_or(0.0, |t| match axis {
                                    Axis::Horizontal => self.tree.nodes[t].base().bounds.width,
                                    Axis::Vertical   => self.tree.nodes[t].base().bounds.height,
                                });
                                let edges = self.node_edges(slider_idx);
                                let local_pos = match axis {
                                    Axis::Horizontal => cursor.0 - edges.left,
                                    Axis::Vertical   => cursor.1 - edges.top,
                                };
                                (s.value_from_track_position(local_pos, thumb_extent), s.value)
                            };
                            let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
                            s.set_value(new_value);
                            self.layout_slider(slider_idx)?;

                            if self.tree.get_node::<SliderNode>(slider_idx)?.value != old_value {
                                self.fire_slider_value_changed(slider_idx)?;
                            }

                            // The thumb just teleported under the cursor -
                            // reflect that in its hover state immediately,
                            // rather than waiting for the next pointer move.
                            if let Some(thumb_idx) = self.tree.get_node::<SliderNode>(slider_idx)?.get_thumb()
                                && self.hovered_node != Some(thumb_idx)
                            {
                                self.hovered_node = Some(thumb_idx);
                                self.dirty_nodes.push(thumb_idx);
                            }
                        }

                        let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
                        let value = s.value as f32;
                        s.drag.start(cursor, value);
                        self.dragging_node = Some(slider_idx);
                    }

                    if let Some(window_idx) = self.window_titlebar_at(idx) {
                        let w = self.tree.get_node_mut::<WindowNode>(window_idx)?;
                        if w.draggable {
                            let start_pos = (w.base.bounds.x, w.base.bounds.y);
                            w.drag.start(cursor, start_pos);
                            self.dragging_node = Some(window_idx);
                        }
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
            None => {
                if input.primary_pressed() {
                    self.tab_scope = None;
                }
                if input.any_click() {
                    self.events.push(UiEvent::Unhandled);
                }
            }
        }

        if input.key_pressed(Key::Tab) {
            if input.key_held(Key::Shift) {
                self.focus_prev();
            } else {
                self.focus_next();
            }
        }

        if (input.key_pressed(Key::Enter) || input.key_pressed(Key::Space))
            && let Some(idx) = self.focused_node
        {
            self.activate_focused(idx)?;
        }

        if input.key_pressed(Key::PageUp) || input.key_pressed(Key::PageDown) {
            // Prefer the hovered scroll panel; fall back to the focused node's
            // nearest scrollable ancestor (e.g. scrollbar slider has focus).
            let scroll_idx = self.hovered_node
                .and_then(|h| self.scrollable_ancestor(h))
                .or_else(|| self.focused_node.and_then(|f| self.scrollable_ancestor(f)));
            if let Some(scroll_idx) = scroll_idx {
                let page_up = input.key_pressed(Key::PageUp);
                let scrollbar_axis = {
                    let scrollbar = self.tree.nodes[scroll_idx].scroll().and_then(|s| s.scrollbar);
                    scrollbar.and_then(|sb| self.tree.get_node::<SliderNode>(sb).ok()).map(|s| s.axis())
                };
                let bounds = self.tree.nodes[scroll_idx].base().bounds;
                let delta = match scrollbar_axis {
                    Some(Axis::Horizontal) => if page_up { (-bounds.width, 0.0) } else { (bounds.width, 0.0) },
                    _                      => if page_up { (0.0, -bounds.height) } else { (0.0, bounds.height) },
                };
                self.scroll_by(scroll_idx, delta)?;
            }
        }

        // Arrow keys step the focused slider; Ctrl+Arrow jumps to the end.
        // ArrowRight/ArrowDown = increase, ArrowLeft/ArrowUp = decrease — works
        // for both horizontal and vertical sliders intuitively.
        if let Some(idx) = self.focused_node
            && matches!(self.tree.nodes[idx], UiNode::Slider(_))
        {
            let right = input.key_pressed(Key::ArrowRight);
            let left  = input.key_pressed(Key::ArrowLeft);
            let down  = input.key_pressed(Key::ArrowDown);
            let up    = input.key_pressed(Key::ArrowUp);
            let ctrl  = input.key_held(Key::Control);

            let increase = right || down;
            let decrease = left  || up;

            if increase || decrease {
                if ctrl {
                    self.jump_slider(idx, increase)?;
                } else {
                    self.step_slider(idx, increase)?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
