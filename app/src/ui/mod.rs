#![allow(dead_code, unsafe_op_in_unsafe_fn)]

mod input;
mod nodes;

use std::rc::Rc;

use anyhow::{anyhow, Result};
use blitz::{Blitz, Container, Pos2, Rgba, UV, VERTEX_2D_RGBA, VertexAllocId};
use winit::{dpi::{LogicalPosition, PhysicalSize}, window::{CursorGrabMode, Window}};

use crate::font::FontAtlas;
pub use input::UiInput;
use nodes::*;

const HOTBAR_SLOTS:       usize = 10;
const SLOT_SIZE:          f32   = 48.0;
const SLOT_GAP:           f32   = 4.0;
const SLOT_MARGIN_BOTTOM: f32   = 20.0;

const XH_SIZE:      f32 = 16.0;
const XH_THICKNESS: f32 = 2.0;

#[derive(PartialEq, Clone, Copy)]
enum MenuState {
    World,
    Title,
    Main,
    GameOptions,
    SystemOptions,
    Keybinds,
}

/// Settings staged in the UI and applied when the user hits Accept.
#[derive(Clone)]
pub struct PendingSettings {
    pub vsync: bool,
    pub fps_cap: u32,
}

impl Default for PendingSettings {
    fn default() -> Self {
        Self { vsync: true, fps_cap: 60 }
    }
}

#[derive(Clone)]
pub enum UiAction {
    CloseMenu,
    ExitApp,
    BackToMain,
    OpenKeybinds,
    OpenGameOptions,
    OpenSystemOptions,
    ToggleVsync,
    ApplySettings,
}

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
    pub fn default(area: PhysicalSize<u32>) -> Self {
        let mut ui_parent = ContainerNode::new();
        ui_parent.base.set_size(area.width as f32, area.height as f32);

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
    pub dirty: bool,
    quad_count: usize,
    vertex_id: VertexAllocId,
    pub font_atlas: Rc<FontAtlas>,
    hotbar_size: (u32, u32),
    mouse_store: (f32, f32),

    tree: UiTree,
    state: MenuState,
    hovered_node: Option<usize>,
    dragging_slider: Option<usize>,
    dirty_nodes: Vec<usize>,

    // Sub-menu container node indices
    menu_container:   usize,
    game_container:   usize,
    system_container: usize,
    keybind_container:  usize,
    world_container:    usize,
    title_container:    usize,

    // Indices of the System Options controls, so Accept can read their
    // live (possibly unsaved) values directly when staging `pending`.
    vsync_checkbox_idx: usize,
    fps_slider_idx: usize,

    pub pending: PendingSettings,
}

/// Builds the quads for a label's text, starting at `(left, baseline_y)` and
/// always emitting exactly `max_len` quads — one per reserved character slot —
/// so a label occupies a constant amount of vertex-buffer space regardless of
/// how long its current text is. Slots with nothing to draw (a character
/// missing from the atlas, or padding past the end of `text`) get a
/// degenerate, zero-area quad, which rasterizes to nothing.
fn label_quads(atlas: &FontAtlas, text: &str, color: Rgba, start_x: f32, baseline_y: f32, max_len: usize) -> Vec<VERTEX_2D_RGBA> {
    let mut verts: Vec<VERTEX_2D_RGBA> = Vec::with_capacity(max_len * 4);
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

                verts.push(VERTEX_2D_RGBA::new(Pos2 { x: left,  y: top    }, UV::new(u0, v0), color));
                verts.push(VERTEX_2D_RGBA::new(Pos2 { x: right, y: top    }, UV::new(u1, v0), color));
                verts.push(VERTEX_2D_RGBA::new(Pos2 { x: right, y: bottom }, UV::new(u1, v1), color));
                verts.push(VERTEX_2D_RGBA::new(Pos2 { x: left,  y: bottom }, UV::new(u0, v1), color));

                cursor_x += g.advance;
            }
            None => {
                let p = Pos2 { x: cursor_x, y: baseline_y };
                let degenerate = VERTEX_2D_RGBA::new(p, UV::new(0.0, 0.0), color);
                verts.extend_from_slice(&[degenerate; 4]);

                if c.is_some() { cursor_x += 8.0; }
            }
        }
    }

    verts
}

impl Ui {
    pub fn new(window: &Window, blitz: &Blitz, atlas: Rc<FontAtlas>) -> Result<Self> {
        let area = window.inner_size();
        let mouse = ((area.width / 2) as f32, (area.height / 2) as f32);

        let mut this = Self {
            dirty: true,
            quad_count: 0,
            vertex_id: blitz.ui_vertex_id(),
            font_atlas: atlas,
            hotbar_size: (0, 0),
            mouse_store: mouse,
            state: MenuState::Title,
            hovered_node: None,
            dragging_slider: None,
            dirty_nodes: Vec::new(),
            tree: UiTree::default(area),

            menu_container:     0,
            game_container:     0,
            system_container:   0,
            keybind_container:  0,
            world_container:    0,
            title_container:    0,
            vsync_checkbox_idx: 0,
            fps_slider_idx:     0,
            pending:            PendingSettings::default(),
        };
        this.generate_tree(area.width as f32, area.height as f32)?;
        Ok(this)
    }

    // ── Node creation helpers ────────────────────────────────────────────────
    // Each wraps a node in its parent, applying only the boilerplate that's
    // the same for every instance (e.g. the white UV rect for solid quads).
    // Everything else — bounds, color, action, text, ... — is configured by
    // the caller afterwards through the returned node's own setters/fields.

    fn create_container(&mut self, parent: usize) -> Result<(usize, &mut ContainerNode)> {
        let idx = self.tree.add_child(UiNode::Container(ContainerNode::new()), parent);
        let c = self.tree.get_node_mut::<ContainerNode>(idx)?;
        Ok((idx, c))
    }

    fn create_panel(&mut self, parent: usize) -> Result<(usize, &mut PanelNode)> {
        let white = self.font_atlas.white_uv;
        let mut p = PanelNode::new();
        p.uv_min = white;
        p.uv_max = white;
        let idx = self.tree.add_child(UiNode::Panel(p), parent);
        let p = self.tree.get_node_mut::<PanelNode>(idx)?;
        Ok((idx, p))
    }

    fn create_button(&mut self, parent: usize) -> Result<(usize, &mut ButtonNode)> {
        let white = self.font_atlas.white_uv;
        let mut b = ButtonNode::new();
        b.uv_min = white;
        b.uv_max = white;
        let idx = self.tree.add_child(UiNode::Button(b), parent);
        let b = self.tree.get_node_mut::<ButtonNode>(idx)?;
        Ok((idx, b))
    }

    fn create_label(&mut self, parent: usize) -> Result<(usize, &mut LabelNode)> {
        let cap_height = self.font_atlas.cap_height;
        let mut l = LabelNode::new("");
        l.base.set_height(cap_height);
        let idx = self.tree.add_child(UiNode::Label(l), parent);
        let l = self.tree.get_node_mut::<LabelNode>(idx)?;
        Ok((idx, l))
    }

    fn create_checkbox(&mut self, parent: usize) -> Result<(usize, &mut CheckboxNode)> {
        let white = self.font_atlas.white_uv;
        let mut c = CheckboxNode::new();
        c.uv_min = white;
        c.uv_max = white;
        let idx = self.tree.add_child(UiNode::Checkbox(c), parent);
        let c = self.tree.get_node_mut::<CheckboxNode>(idx)?;
        Ok((idx, c))
    }

    /// Also creates the slider's thumb (panel) and value label as children
    /// and wires their indices back into the returned `SliderNode`.
    fn create_slider(&mut self, parent: usize) -> Result<(usize, &mut SliderNode)> {
        let white = self.font_atlas.white_uv;
        let mut slider = SliderNode::new();
        slider.panel.uv_min = white;
        slider.panel.uv_max = white;
        let label_text  = slider.display_text();
        let label_width = self.label_width(&label_text);
        let slider_idx = self.tree.add_child(UiNode::Slider(slider), parent);

        let (thumb_idx, thumb) = self.create_panel(slider_idx)?;
        thumb.base.set_size(16.0, 32.0);
        thumb.set_color(Rgba::new(0.8, 0.8, 0.8, 0.9));

        let (label_idx, label) = self.create_label(parent)?;
        label.set_text(label_text);
        label.base.set_width(label_width);
        label.base.set_position_anchored_to(Anchor::Right, slider_idx, Anchor::Left, -8.0, 0.0);

        let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
        s.set_thumb(Some(thumb_idx));
        s.set_label(Some(label_idx));
        Ok((slider_idx, s))
    }

    // Used for the debug menu to indicate quad size of the ui
    pub fn quad_count(&self) -> usize {
        self.quad_count
    }

    /// Rebuilds the entire vertex buffer from the current tree state. Clears
    /// `dirty` and `dirty_nodes` so subsequent frames can use `flush_dirty`
    /// until the next structural change. Must be called whenever a node is
    /// added, removed, or its `max_len` grows, since those events shift
    /// `vertex_offset` bookkeeping for every node that follows.
    pub unsafe fn flush_all(&mut self, container: &mut Container, screen: (f32, f32)) {
        self.dirty = false;
        self.dirty_nodes.clear();
        let atlas = &*self.font_atlas;
        let mut verts: Vec<VERTEX_2D_RGBA> = Vec::new();

        // Crosshair
        if self.state == MenuState::World {
            let cx = screen.0 / 2.0;
            let cy = screen.1 / 2.0;
            let xh_color = Rgba::new(1.0, 1.0, 1.0, 0.1);
            let w = UV::new(atlas.white_uv[0], atlas.white_uv[1]);

            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_SIZE,      y: cy - XH_THICKNESS }, w, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_SIZE,      y: cy - XH_THICKNESS }, w, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_SIZE,      y: cy + XH_THICKNESS }, w, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_SIZE,      y: cy + XH_THICKNESS }, w, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_THICKNESS, y: cy - XH_SIZE      }, w, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_THICKNESS, y: cy - XH_SIZE      }, w, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_THICKNESS, y: cy + XH_SIZE      }, w, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_THICKNESS, y: cy + XH_SIZE      }, w, xh_color));
        }

        let root_edges = self.tree.nodes[0].base().resolve(&Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 }, &self.tree.nodes);
        let mut stack: Vec<(usize, Edges)> = vec![(0, root_edges)];

        while !stack.is_empty() {
            let (node_idx, parent_edges) = stack.pop().unwrap();
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
                        let render_data = match &self.tree.nodes[child_idx] {
                            UiNode::Panel(p)  => Some((p.color, p.uv_min, p.uv_max)),
                            UiNode::Button(b)   => Some((b.color,             b.uv_min, b.uv_max)),
                            UiNode::Checkbox(c) => Some((c.display_color(), c.uv_min, c.uv_max)),
                            UiNode::Slider(s)   => Some((s.panel.color,       s.panel.uv_min, s.panel.uv_max)),
                            _ => None,
                        };

                        if let Some((color, [u0, v0], [u1, v1])) = render_data {
                            self.tree.nodes[child_idx].base_mut().vertex_offset = verts.len();
                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.left,  y: e.top    }, UV::new(u0, v0), color));
                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.right, y: e.top    }, UV::new(u1, v0), color));
                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.right, y: e.bottom }, UV::new(u1, v1), color));
                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.left,  y: e.bottom }, UV::new(u0, v1), color));
                        }

                        stack.push((child_idx, e));
                    }
                }
            }
        }

        self.quad_count = verts.len() / 4;
        container.stage_vertex_update(self.vertex_id, &verts);
    }
    
    /// Updates only the nodes listed in `dirty_nodes`, overwriting their quads
    /// in-place at their recorded `vertex_offset`. Safe to call when the tree
    /// structure hasn't changed and no node's `max_len` has grown, since those
    /// conditions guarantee every node still occupies the same slot in the
    /// buffer it was assigned during the last `flush_all`.
    pub unsafe fn flush_dirty(&mut self, container: &mut Container) {
        let dirty: Vec<usize> = self.dirty_nodes.drain(..).collect();
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

                    container.stage_vertex_update_at(self.vertex_id, offset, &vertices);
                }
                _ => {
                    let render_data = match &self.tree.nodes[node_idx] {
                        UiNode::Panel(p)  => Some((p.color, p.uv_min, p.uv_max)),
                        UiNode::Button(b)   => Some((b.color,             b.uv_min, b.uv_max)),
                        UiNode::Checkbox(c) => Some((c.display_color(), c.uv_min, c.uv_max)),
                        UiNode::Slider(s)   => Some((s.panel.color,       s.panel.uv_min, s.panel.uv_max)),
                        _                 => None,
                    };

                    if let Some((color, [u0, v0], [u1, v1])) = render_data {
                        let e      = self.node_edges(node_idx);
                        let offset = self.tree.nodes[node_idx].base().vertex_offset;
                        let vertices = [
                            VERTEX_2D_RGBA::new(Pos2 { x: e.left,  y: e.top    }, UV::new(u0, v0), color),
                            VERTEX_2D_RGBA::new(Pos2 { x: e.right, y: e.top    }, UV::new(u1, v0), color),
                            VERTEX_2D_RGBA::new(Pos2 { x: e.right, y: e.bottom }, UV::new(u1, v1), color),
                            VERTEX_2D_RGBA::new(Pos2 { x: e.left,  y: e.bottom }, UV::new(u0, v1), color),
                        ];
                        container.stage_vertex_update_at(self.vertex_id, offset, &vertices);
                    }
                }
            }
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
    /// slider's current value. Marks both as dirty for re-rendering.
    fn layout_slider(&mut self, slider_idx: usize) -> Result<()> {
        let (text, thumb_idx, label_idx) = {
            let s = self.tree.get_node::<SliderNode>(slider_idx)?;
            (s.display_text(), s.get_thumb(), s.get_label())
        };

        if let Some(thumb_idx) = thumb_idx {
            let thumb_width = self.tree.nodes[thumb_idx].base().bounds.width;
            let x_offset = {
                let s = self.tree.get_node::<SliderNode>(slider_idx)?;
                s.thumb_offset(thumb_width)
            };
            let thumb = self.tree.get_node_mut::<PanelNode>(thumb_idx)?;
            thumb.base.set_position(Anchor::Left, x_offset, 0.0);
            self.dirty_nodes.push(thumb_idx);
        }

        if let Some(label_idx) = label_idx {
            let label = self.tree.get_node_mut::<LabelNode>(label_idx)?;
            if label.set_text(text) {
                self.dirty = true;
            } else {
                self.dirty_nodes.push(label_idx);
            }
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

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        blitz.draw_ui_quads(0, self.quad_count, self.font_atlas.texture_id);
    }

    pub fn has_dirty_nodes(&self) -> bool {
        !self.dirty_nodes.is_empty()
    }

    pub fn generate_tree(&mut self, screen_width: f32, screen_height: f32) -> Result<()> {
        self.dirty = true;

        let mut ui_parent = ContainerNode::new();
        ui_parent.base.set_size(screen_width, screen_height);

        self.tree = UiTree {
            nodes: vec![UiNode::Container(ui_parent)],
        };
        self.hovered_node = None;
        self.dirty_nodes.clear();

        let panel_color        = Rgba::new(0.8, 0.8, 0.8, 0.2);
        let button_color       = Rgba::new(0.5, 0.5, 0.5, 0.4);
        let button_hover_color = Rgba::new(0.65, 0.65, 0.65, 0.5);
        let row_color          = Rgba { x: 0.0, y: 0.0, z: 0.0, w: 0.2 };
        let panel_w            = screen_width / 2.0;
        let menu_rect          = Rect { x: 0.0, y: 0.0, width: panel_w, height: screen_height };

        // ── Main menu ────────────────────────────────────────────────────────
        let (main_idx, panel) = self.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.menu_container = main_idx;

        let (_, label) = self.create_label(main_idx)?;
        label.set_text("Main Menu");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (resume_idx, btn) = self.create_button(main_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::CloseMenu);
        let (_, label) = self.create_label(resume_idx)?;
        label.set_text("Resume");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::OpenGameOptions);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("Game Options");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::OpenSystemOptions);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("System Options");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::OpenKeybinds);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("Keybinds");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 584.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::ExitApp);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("Quit");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── Game Options ─────────────────────────────────────────────────────
        let (game_idx, panel) = self.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.game_container = game_idx;

        let (_, label) = self.create_label(game_idx)?;
        label.set_text("Game Options");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (b_idx, btn) = self.create_button(game_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::BackToMain);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── System Options ───────────────────────────────────────────────────
        let (sys_idx, panel) = self.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.system_container = sys_idx;

        let (_, label) = self.create_label(sys_idx)?;
        label.set_text("System Options");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        // V-Sync row
        let (row_idx, panel) = self.create_panel(sys_idx)?;
        panel.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        panel.color       = row_color;
        let (_, label) = self.create_label(row_idx)?;
        label.set_text("V-Sync");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        let vsync_selected = self.pending.vsync;
        let (vsync_checkbox_idx, checkbox) = self.create_checkbox(row_idx)?;
        checkbox.base.set_position(Anchor::Right, -8.0, 0.0);
        checkbox.base.set_size(32.0, 32.0);
        checkbox.selected               = vsync_selected;
        checkbox.hover_color            = Some(button_hover_color);
        checkbox.interaction.on_release = Some(UiAction::ToggleVsync);
        self.vsync_checkbox_idx = vsync_checkbox_idx;

        // Slider row
        let (slider_row_idx, panel) = self.create_panel(sys_idx)?;
        panel.base.bounds = Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 };
        panel.color       = row_color;
        let (_, label) = self.create_label(slider_row_idx)?;
        label.set_text("Framerate");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        let fps_cap = self.pending.fps_cap;
        let (slider_idx, slider) = self.create_slider(slider_row_idx)?;
        slider.panel.base.set_position(Anchor::Right, -8.0, 0.0);
        slider.set_min_max(60, 999);
        slider.step_size = 8;
        slider.set_value(fps_cap);
        self.fps_slider_idx = slider_idx;
        self.layout_slider(slider_idx)?;

        let (b_idx, btn) = self.create_button(sys_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::ApplySettings);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("Accept");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(sys_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::BackToMain);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // Re-syncs the V-Sync checkbox and frame rate slider from `pending`
        // whenever this menu is shown, so they always reflect the values
        // staged when the settings menu was opened.
        self.tree.nodes[sys_idx].base_mut().visibility.on_show = Some(Box::new(move |ui| {
            if let Ok(checkbox) = ui.tree.get_node_mut::<CheckboxNode>(vsync_checkbox_idx) {
                checkbox.selected = ui.pending.vsync;
                ui.dirty_nodes.push(vsync_checkbox_idx);
            }
            if let Ok(slider) = ui.tree.get_node_mut::<SliderNode>(slider_idx) {
                slider.set_value(ui.pending.fps_cap);
            }
            let _ = ui.layout_slider(slider_idx);
        }));

        // ── Keybinds ─────────────────────────────────────────────────────────
        let (keybind_idx, panel) = self.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.keybind_container = keybind_idx;

        let (_, label) = self.create_label(keybind_idx)?;
        label.set_text("Keybinds");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (b_idx, btn) = self.create_button(keybind_idx)?;
        btn.base.bounds            = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::BackToMain);
        let (_, label) = self.create_label(b_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── World UI ─────────────────────────────────────────────────────────
        let (world_idx, world) = self.create_container(0)?;
        world.base.set_size(screen_width, screen_height);
        self.world_container = world_idx;

        let total_w = HOTBAR_SLOTS as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
        let (hotbar_idx, hotbar) = self.create_panel(world_idx)?;
        hotbar.base.set_position(Anchor::Bottom, 0.0, -SLOT_MARGIN_BOTTOM);
        hotbar.base.set_size(total_w, SLOT_SIZE + SLOT_GAP);
        hotbar.color = panel_color;

        for i in 0..HOTBAR_SLOTS {
            let x = i as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
            let (_, slot) = self.create_button(hotbar_idx)?;
            slot.base.set_position(Anchor::TopLeft, x, SLOT_GAP / 2.0);
            slot.base.set_size(SLOT_SIZE, SLOT_SIZE);
            slot.color = Rgba::new(0.0, 0.0, 0.0, 0.6);
        }

        // ── Title screen ─────────────────────────────────────────────────────
        let (title_idx, title) = self.create_panel(0)?;
        title.base.set_size(screen_width, screen_height);
        title.color        = Rgba::new(0.0, 0.0, 0.0, 1.0);
        title.base.visible = false;
        self.title_container = title_idx;

        let (_, label) = self.create_label(title_idx)?;
        label.set_text("Playground");
        label.base.set_position(Anchor::Top, 0.0, 80.0);

        let (start_idx, start_btn) = self.create_button(title_idx)?;
        start_btn.base.set_position(Anchor::Center, 0.0, 0.0);
        start_btn.base.set_size(200.0, 48.0);
        start_btn.color                  = Rgba::new(1.0, 1.0, 1.0, 1.0);
        start_btn.hover_color            = Some(Rgba::new(0.2, 0.5, 1.0, 1.0));
        start_btn.interaction.on_release = Some(UiAction::CloseMenu);
        let (_, label) = self.create_label(start_idx)?;
        label.set_text("Start");
        label.base.set_position(Anchor::Left, 64.0, 0.0);

        let (quit_idx, quit_btn) = self.create_button(title_idx)?;
        quit_btn.base.set_position(Anchor::Center, 0.0, 64.0);
        quit_btn.base.set_size(200.0, 48.0);
        quit_btn.color                  = Rgba::new(1.0, 1.0, 1.0, 1.0);
        quit_btn.hover_color            = Some(Rgba::new(0.2, 0.5, 1.0, 1.0));
        quit_btn.interaction.on_release = Some(UiAction::ExitApp);
        let (_, label) = self.create_label(quit_idx)?;
        label.set_text("Quit");
        label.base.set_position(Anchor::Left, 64.0, 0.0);

        // Reapply visibility in case the tree was rebuilt mid-session
        if self.state != MenuState::World {
            let visible_idx = self.container_for(self.state);
            self.tree.nodes[visible_idx].base_mut().visible = true;
            self.tree.nodes[self.world_container].base_mut().visible = false;
        }

        Ok(())
    }

    /// Maps a [`MenuState`] to the index of the container node it displays.
    fn container_for(&self, state: MenuState) -> usize {
        match state {
            MenuState::World         => self.world_container,
            MenuState::Title         => self.title_container,
            MenuState::Main          => self.menu_container,
            MenuState::GameOptions   => self.game_container,
            MenuState::SystemOptions => self.system_container,
            MenuState::Keybinds      => self.keybind_container,
        }
    }

    /// Takes a visibility callback out of `node_idx`, invokes it with
    /// `&mut self`, then restores it. The take/restore dance works around
    /// Rust's aliasing rules: the callback is borrowed out of `self.tree`, so
    /// it can't stay borrowed while also receiving `&mut self`.
    ///
    /// Operates on `NodeBase` rather than a concrete node type since menu
    /// screens are backed by different node types (`PanelNode`, `ContainerNode`).
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

    /// Switches between the world view and the menu, firing `on_hide`/`on_show`
    /// on the containers being left/entered.
    pub fn toggle_menu(&mut self, window: &Window) -> Result<()> {
        self.dirty = true;
        if self.state == MenuState::World {
            let old_idx = self.world_container;
            let new_idx = self.menu_container;

            self.tree.nodes[old_idx].base_mut().visible = false;
            self.fire_callback(old_idx, |c| &mut c.visibility.on_hide)?;

            self.tree.nodes[new_idx].base_mut().visible = true;
            self.state = MenuState::Main;
            self.fire_callback(new_idx, |c| &mut c.visibility.on_show)?;

            window.set_cursor_grab(CursorGrabMode::None)
                .expect("Failed to free cursor");
            window.set_cursor_position(LogicalPosition::new(self.mouse_store.0, self.mouse_store.1))
                .expect("Failed to set cursor position");
            window.set_cursor_visible(true);
        } else {
            if let Some(old) = self.hovered_node.take() {
                if let UiNode::Button(b) = &mut self.tree.nodes[old] {
                    if let Some(hc) = b.hover_color.as_mut() {
                        std::mem::swap(&mut b.color, hc);
                    }
                }
            }

            let old_idx = self.container_for(self.state);
            let new_idx = self.world_container;

            self.tree.nodes[old_idx].base_mut().visible = false;
            self.fire_callback(old_idx, |c| &mut c.visibility.on_hide)?;

            self.tree.nodes[new_idx].base_mut().visible = true;
            self.state = MenuState::World;
            self.fire_callback(new_idx, |c| &mut c.visibility.on_show)?;

            window.set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .expect("Failed to grab cursor");
            window.set_cursor_visible(false);
        }
        Ok(())
    }

    pub fn menu_opened(&self) -> bool {
        self.state != MenuState::World
    }

    pub fn is_title_screen(&self) -> bool {
        self.state == MenuState::Title
    }

    /// Resets pending settings to the currently applied values so the settings
    /// menu always shows the real state when opened. Per-menu containers pick
    /// these up themselves via their `on_show` callback.
    pub fn sync_pending(&mut self, settings: PendingSettings) {
        self.pending = settings;
    }

    pub fn handle_input(&mut self, input: &UiInput) -> Result<Option<UiAction>> {
        if self.state == MenuState::World { return Ok(None); }

        let cursor = input.cursor();

        if let Some(slider_idx) = self.dragging_slider {
            if input.primary_held() {
                self.drag_slider(slider_idx, cursor)?;
            } else {
                let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
                s.drag.stop();
                self.dragging_slider = None;
            }
            return Ok(None);
        }

        let hit = self.tree.hit_test(
            cursor.0, cursor.1, 0,
            &Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        );

        if hit != self.hovered_node {
            if let Some(old) = self.hovered_node {
                match &mut self.tree.nodes[old] {
                    UiNode::Button(b) => {
                        if let Some(hc) = b.hover_color.as_mut() {
                            std::mem::swap(&mut b.color, hc);
                            self.dirty_nodes.push(old);
                        }
                    }
                    UiNode::Checkbox(c) => {
                        c.hovered = false;
                        self.dirty_nodes.push(old);
                    }
                    _ => {}
                }
            }
            if let Some(new) = hit {
                match &mut self.tree.nodes[new] {
                    UiNode::Button(b) => {
                        if let Some(hc) = b.hover_color.as_mut() {
                            std::mem::swap(&mut b.color, hc);
                            self.dirty_nodes.push(new);
                        }
                    }
                    UiNode::Checkbox(c) => {
                        c.hovered = true;
                        self.dirty_nodes.push(new);
                    }
                    _ => {}
                }
            }
            self.hovered_node = hit;
        }

        if let Some(idx) = hit {
            if input.primary_pressed() {
                if let Some(slider_idx) = self.slider_at(idx) {
                    let s = self.tree.get_node_mut::<SliderNode>(slider_idx)?;
                    let value = s.value as f32;
                    s.drag.start(cursor, value);
                    self.dragging_slider = Some(slider_idx);
                }
            }

            let action = if input.primary_released() {
                match &self.tree.nodes[idx] {
                    UiNode::Button(b)   => b.interaction.on_release.clone(),
                    UiNode::Checkbox(c) => c.interaction.on_release.clone(),
                    _ => None,
                }
            } else { None };

            if let Some(action) = action {
                match action {
                    UiAction::OpenKeybinds      => self.navigate(MenuState::Keybinds)?,
                    UiAction::OpenGameOptions   => self.navigate(MenuState::GameOptions)?,
                    UiAction::OpenSystemOptions => self.navigate(MenuState::SystemOptions)?,
                    UiAction::BackToMain        => self.navigate(MenuState::Main)?,
                    UiAction::ToggleVsync       => {
                        if let UiNode::Checkbox(c) = &mut self.tree.nodes[idx] {
                            c.selected = !c.selected;
                        }
                        self.dirty_nodes.push(idx);
                    },
                    UiAction::ApplySettings     => {
                        self.pending.vsync   = self.tree.get_node::<CheckboxNode>(self.vsync_checkbox_idx)?.selected;
                        self.pending.fps_cap = self.tree.get_node::<SliderNode>(self.fps_slider_idx)?.value;
                        self.navigate(MenuState::Main)?;
                        return Ok(Some(UiAction::ApplySettings));
                    }
                    UiAction::CloseMenu | UiAction::ExitApp => return Ok(Some(action)),
                }
            }
        }

        Ok(None)
    }

    /// Switches the visible menu screen, firing `on_hide` on the screen being
    /// left and `on_show` on the screen being entered.
    fn navigate(&mut self, new_state: MenuState) -> Result<()> {
        // Swap the hovered button's color back before the layout changes.
        // Do NOT push to dirty_nodes — flush_all supersedes flush_dirty here,
        // and stale vertex_offset values from the old layout would corrupt the new buffer.
        if let Some(old) = self.hovered_node.take() {
            match &mut self.tree.nodes[old] {
                UiNode::Button(b) => {
                    if let Some(hc) = b.hover_color.as_mut() {
                        std::mem::swap(&mut b.color, hc);
                    }
                }
                UiNode::Checkbox(c) => c.hovered = false,
                _ => {}
            }
        }

        let old_idx = self.container_for(self.state);
        let new_idx = self.container_for(new_state);

        self.tree.nodes[old_idx].base_mut().visible = false;
        self.fire_callback(old_idx, |c| &mut c.visibility.on_hide)?;

        self.tree.nodes[new_idx].base_mut().visible = true;
        self.state = new_state;
        self.fire_callback(new_idx, |c| &mut c.visibility.on_show)?;

        self.dirty = true;
        Ok(())
    }
}
