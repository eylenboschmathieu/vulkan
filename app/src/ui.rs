#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use std::rc::Rc;

use blitz::{Blitz, Container, Pos2, Rgba, UV, VERTEX_2D_RGBA, VertexAllocId};
use winit::{dpi::{LogicalPosition, PhysicalSize}, window::{CursorGrabMode, Window}};

use crate::{font::FontAtlas, input::{Action, InputManager}};

const HOTBAR_SLOTS:       usize = 10;
const SLOT_SIZE:          f32   = 48.0;
const SLOT_GAP:           f32   = 4.0;
const SLOT_MARGIN_BOTTOM: f32   = 20.0;

const XH_SIZE:      f32 = 16.0;
const XH_THICKNESS: f32 = 2.0;

#[derive(PartialEq, Debug, Clone, Copy)]
enum MenuState {
    World,
    Title,
    Main,
    GameOptions,
    SystemOptions,
    Keybinds,
}

/// Settings staged in the UI and applied when the user hits Accept.
#[derive(Debug, Clone)]
pub struct PendingSettings {
    pub vsync: bool,
}

impl Default for PendingSettings {
    fn default() -> Self {
        Self { vsync: true }
    }
}

/// Binds a [`CheckboxNode`] to a specific setting in [`PendingSettings`].
/// The checkbox's selected state is kept in sync automatically.
#[derive(Debug, Clone, Copy)]
pub enum SettingKey {
    Vsync,
}

#[derive(Clone, Debug)]
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

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone)]
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

// ── Node types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Clip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,    Top,    TopRight,
    Left,       Center, Right,
    BottomLeft, Bottom, BottomRight,
}

impl Anchor {
    /// Returns the (x, y) fractional position of this anchor within a rect.
    /// e.g. TopLeft = (0, 0), Center = (0.5, 0.5), BottomRight = (1, 1).
    fn fractions(self) -> (f32, f32) {
        match self {
            Anchor::TopLeft     => (0.0, 0.0),
            Anchor::Top         => (0.5, 0.0),
            Anchor::TopRight    => (1.0, 0.0),
            Anchor::Left        => (0.0, 0.5),
            Anchor::Center      => (0.5, 0.5),
            Anchor::Right       => (1.0, 0.5),
            Anchor::BottomLeft  => (0.0, 1.0),
            Anchor::Bottom      => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }
}

#[derive(Debug)]
pub struct NodeBase {
    pub bounds:      Rect,
    pub src_anchor:  Anchor,        // Attachment point on this node
    pub target:      Option<usize>, // Node to anchor to; None = parent
    pub dst_anchor:  Anchor,        // Attachment point on the target node
    pub parent:        Option<usize>,
    pub children:      Vec<usize>,
    pub visible:       bool,
    pub vertex_offset: usize,
}

impl NodeBase {
    pub fn new() -> Self {
        Self {
            bounds:      Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            src_anchor:  Anchor::TopLeft,
            target:      None,
            dst_anchor:  Anchor::TopLeft,
            parent:      None,
            children:    Vec::new(),
            visible:       true,
            vertex_offset: 0,
        }
    }

    /// Positions the node symmetrically relative to its parent: both the
    /// attachment point on this node and the reference point on the parent
    /// use the same anchor, so e.g. `Center + (0, 0)` truly centres the node.
    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) {
        self.src_anchor         = anchor;
        self.dst_anchor = anchor;
        self.target        = None;
        self.bounds.x      = x;
        self.bounds.y      = y;
    }

    /// Positions the node relative to an arbitrary sibling or ancestor.
    /// `src_anchor` is the attachment point on this node; `dst_anchor` is the
    /// reference point on the target node.
    pub fn set_position_anchored_to(&mut self, src_anchor: Anchor, target: usize, dst_anchor: Anchor, x: f32, y: f32) {
        self.src_anchor         = src_anchor;
        self.target        = Some(target);
        self.dst_anchor = dst_anchor;
        self.bounds.x      = x;
        self.bounds.y      = y;
    }

    pub fn set_width(&mut self, width: f32) {
        self.bounds.width = width;
    }

    pub fn set_height(&mut self, height: f32) {
        self.bounds.height = height;
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.bounds.width  = width;
        self.bounds.height = height;
    }

    /// Computes the node's screen-space [`Edges`].
    /// When `target` is `None` the node is positioned relative to `parent_edges`.
    /// When `target` is `Some(idx)` the node is positioned relative to that
    /// node's absolute edges, computed by walking its parent chain.
    pub fn resolve(&self, parent_edges: &Edges, nodes: &[UiNode]) -> Edges {
        let ref_edges = match self.target {
            None      => parent_edges.clone(),
            Some(idx) => node_absolute_edges(idx, nodes),
        };
        let (px, py) = self.src_anchor.fractions();
        let (tx, ty) = self.dst_anchor.fractions();
        let ref_w    = ref_edges.right  - ref_edges.left;
        let ref_h    = ref_edges.bottom - ref_edges.top;
        let ref_x    = ref_edges.left + tx * ref_w  + self.bounds.x;
        let ref_y    = ref_edges.top  + ty * ref_h  + self.bounds.y;
        let left     = ref_x - px * self.bounds.width;
        let top      = ref_y - py * self.bounds.height;
        Edges { left, right: left + self.bounds.width, top, bottom: top + self.bounds.height }
    }
}

/// Computes the absolute screen-space edges of `idx` by walking its parent chain.
fn node_absolute_edges(idx: usize, nodes: &[UiNode]) -> Edges {
    let node = &nodes[idx];
    let parent_edges = match node.base().parent {
        None    => Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        Some(p) => node_absolute_edges(p, nodes),
    };
    node.base().resolve(&parent_edges, nodes)
}

/// Invisible grouping node — children only, no quad rendered.
#[derive(Debug)]
pub struct ContainerNode {
    pub base: NodeBase,
}

impl ContainerNode {
    pub fn new() -> Self {
        Self { base: NodeBase::new() }
    }
}

/// Visible background panel. Labelable.
#[derive(Debug)]
pub struct PanelNode {
    pub base: NodeBase,
    pub color: Rgba,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

impl PanelNode {
    pub fn new() -> Self {
        Self {
            base: NodeBase::new(),
            color: Rgba::new(0.0, 0.0, 0.0, 0.0),
            uv_min: [0.0, 0.0],
            uv_max: [0.0, 0.0],
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
}

/// Holds the four interaction callbacks shared by any interactive node type.
#[derive(Debug, Default)]
pub struct Interaction {
    pub on_pressed: Option<UiAction>,
    pub on_release: Option<UiAction>,
    pub on_enter:   Option<UiAction>,
    pub on_leave:   Option<UiAction>,
}

/// Interactive button. Labelable.
#[derive(Debug)]
pub struct ButtonNode {
    pub base:        NodeBase,
    pub color:       Rgba,
    pub hover_color: Option<Rgba>,
    pub uv_min:      [f32; 2],
    pub uv_max:      [f32; 2],
    pub interaction: Interaction,
}

impl ButtonNode {
    pub fn new() -> Self {
        Self {
            base:        NodeBase::new(),
            color:       Rgba::new(0.0, 0.0, 0.0, 0.0),
            hover_color: None,
            uv_min:      [0.0, 0.0],
            uv_max:      [0.0, 0.0],
            interaction: Interaction::default(),
        }
    }
}

/// Toggleable checkbox with distinct unselected, selected, and hovered colours.
#[derive(Debug)]
pub struct CheckboxNode {
    pub base:           NodeBase,
    pub color:          Rgba,        // unselected colour
    pub selected_color: Rgba,        // selected colour
    pub hover_color:    Option<Rgba>,
    pub uv_min:         [f32; 2],
    pub uv_max:         [f32; 2],
    pub selected:       bool,
    pub hovered:        bool,
    pub setting:        Option<SettingKey>,
    pub interaction:    Interaction,
}

impl CheckboxNode {
    pub fn new() -> Self {
        Self {
            base:           NodeBase::new(),
            color:          Rgba::new(0.5, 0.5, 0.5, 0.4),
            selected_color: Rgba::new(0.2, 0.7, 0.3, 0.7),
            hover_color:    None,
            uv_min:         [0.0, 0.0],
            uv_max:         [0.0, 0.0],
            selected:       false,
            setting:        None,
            hovered:        false,
            interaction:    Interaction::default(),
        }
    }

    pub fn display_color(&self) -> Rgba {
        if self.hovered {
            self.hover_color.unwrap_or(if self.selected { self.selected_color } else { self.color })
        } else if self.selected {
            self.selected_color
        } else {
            self.color
        }
    }
}

/// Text label. Not interactive, not labelable itself.
#[derive(Debug)]
pub struct LabelNode {
    pub base: NodeBase,
    pub text: String,
    pub color: Rgba,
}

impl LabelNode {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            base: NodeBase::new(),
            text: text.into(),
            color: Rgba::new(0.0, 0.0, 0.0, 1.0),
        }
    }
}

// ── UiNode enum ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum UiNode {
    Container(ContainerNode),
    Panel(PanelNode),
    Button(ButtonNode),
    Checkbox(CheckboxNode),
    Label(LabelNode),
}

impl UiNode {
    pub fn base(&self) -> &NodeBase {
        match self {
            UiNode::Container(n) => &n.base,
            UiNode::Panel(n)     => &n.base,
            UiNode::Button(n)    => &n.base,
            UiNode::Checkbox(n)  => &n.base,
            UiNode::Label(n)     => &n.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut NodeBase {
        match self {
            UiNode::Container(n) => &mut n.base,
            UiNode::Panel(n)     => &mut n.base,
            UiNode::Button(n)    => &mut n.base,
            UiNode::Checkbox(n)  => &mut n.base,
            UiNode::Label(n)     => &mut n.base,
        }
    }
}

// ── UiTree ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct UiTree {
    pub nodes: Vec<UiNode>,
    pub root: usize,
}

impl UiTree {
    pub fn default(area: PhysicalSize<u32>) -> Self {
        let mut ui_parent = ContainerNode::new();
        ui_parent.base.set_size(area.width as f32, area.height as f32);

        Self {
            root: 0,
            nodes: vec![UiNode::Container(ui_parent)],
        }
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

#[derive(Debug)]
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
    dirty_nodes: Vec<usize>,

    // Sub-menu container node indices
    menu_container:   usize,
    game_container:   usize,
    system_container: usize,
    keybind_container:  usize,
    world_container:    usize,
    title_container:    usize,

    pub pending: PendingSettings,
}

impl Ui {
    pub fn new(window: &Window, blitz: &Blitz, atlas: Rc<FontAtlas>) -> Self {
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
            dirty_nodes: Vec::new(),
            tree: UiTree::default(area),

            menu_container:     0,
            game_container:     0,
            system_container:   0,
            keybind_container:  0,
            world_container:    0,
            title_container:    0,
            pending:            PendingSettings::default(),
        };
        this.generate_tree(area.width as f32, area.height as f32);
        this
    }

    // ── Node creation helpers ────────────────────────────────────────────────
    // Each wraps a node in its parent, applying only the boilerplate that's
    // the same for every instance (e.g. the white UV rect for solid quads).
    // Everything else — bounds, color, action, text, ... — is configured by
    // the caller afterwards through the returned node's own setters/fields.

    fn create_container(&mut self, parent: usize) -> (usize, &mut ContainerNode) {
        let idx = self.tree.add_child(UiNode::Container(ContainerNode::new()), parent);
        let UiNode::Container(c) = &mut self.tree.nodes[idx] else { unreachable!() };
        (idx, c)
    }

    fn create_panel(&mut self, parent: usize) -> (usize, &mut PanelNode) {
        let white = self.font_atlas.white_uv;
        let mut p = PanelNode::new();
        p.uv_min = white;
        p.uv_max = white;
        let idx = self.tree.add_child(UiNode::Panel(p), parent);
        let UiNode::Panel(p) = &mut self.tree.nodes[idx] else { unreachable!() };
        (idx, p)
    }

    fn create_button(&mut self, parent: usize) -> (usize, &mut ButtonNode) {
        let white = self.font_atlas.white_uv;
        let mut b = ButtonNode::new();
        b.uv_min = white;
        b.uv_max = white;
        let idx = self.tree.add_child(UiNode::Button(b), parent);
        let UiNode::Button(b) = &mut self.tree.nodes[idx] else { unreachable!() };
        (idx, b)
    }

    fn create_label(&mut self, parent: usize) -> (usize, &mut LabelNode) {
        let cap_height = self.font_atlas.cap_height;
        let mut l = LabelNode::new("");
        l.base.set_height(cap_height);
        let idx = self.tree.add_child(UiNode::Label(l), parent);
        let UiNode::Label(l) = &mut self.tree.nodes[idx] else { unreachable!() };
        (idx, l)
    }

    fn create_checkbox(&mut self, parent: usize) -> (usize, &mut CheckboxNode) {
        let white = self.font_atlas.white_uv;
        let mut c = CheckboxNode::new();
        c.uv_min = white;
        c.uv_max = white;
        let idx = self.tree.add_child(UiNode::Checkbox(c), parent);
        let UiNode::Checkbox(c) = &mut self.tree.nodes[idx] else { unreachable!() };
        (idx, c)
    }

    pub fn quad_count(&self) -> usize {
        self.quad_count
    }

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
                        let mut cursor_x = e.left;
                        let baseline_y   = e.bottom;
                        let color        = l.color;
                        let text         = l.text.clone();

                        self.tree.nodes[child_idx].base_mut().vertex_offset = verts.len();

                        for c in text.chars() {
                            let Some(g) = atlas.glyphs.get(&c) else { cursor_x += 8.0; continue };
                            let [u0, v0] = g.uv_min;
                            let [u1, v1] = g.uv_max;
                            let left     = cursor_x + g.bearing_x;
                            let right    = left + g.width as f32;
                            let top      = baseline_y - g.bearing_y - g.height as f32;
                            let bottom   = baseline_y - g.bearing_y;

                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: left,  y: top    }, UV::new(u0, v0), color));
                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: right, y: top    }, UV::new(u1, v0), color));
                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: right, y: bottom }, UV::new(u1, v1), color));
                            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: left,  y: bottom }, UV::new(u0, v1), color));

                            cursor_x += g.advance;
                        }
                    }
                    _ => {
                        let render_data = match &self.tree.nodes[child_idx] {
                            UiNode::Panel(p)  => Some((p.color, p.uv_min, p.uv_max)),
                            UiNode::Button(b)   => Some((b.color,             b.uv_min, b.uv_max)),
                            UiNode::Checkbox(c) => Some((c.display_color(), c.uv_min, c.uv_max)),
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
    
    pub unsafe fn flush_dirty(&mut self, container: &mut Container) {
        let dirty: Vec<usize> = self.dirty_nodes.drain(..).collect();
        for node_idx in dirty {
            let render_data = match &self.tree.nodes[node_idx] {
                UiNode::Panel(p)  => Some((p.color, p.uv_min, p.uv_max)),
                UiNode::Button(b)   => Some((b.color,             b.uv_min, b.uv_max)),
                UiNode::Checkbox(c) => Some((c.display_color(), c.uv_min, c.uv_max)),
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

    fn node_edges(&self, node_idx: usize) -> Edges {
        let node = &self.tree.nodes[node_idx];
        let parent_edges = match node.base().parent {
            Some(p) => self.node_edges(p),
            None    => Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        };
        node.base().resolve(&parent_edges, &self.tree.nodes)
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        blitz.draw_ui_quads(0, self.quad_count, self.font_atlas.texture_id);
    }

    pub fn has_dirty_nodes(&self) -> bool {
        !self.dirty_nodes.is_empty()
    }

    pub fn generate_tree(&mut self, screen_width: f32, screen_height: f32) {
        self.dirty = true;

        let mut ui_parent = ContainerNode::new();
        ui_parent.base.set_size(screen_width, screen_height);

        self.tree = UiTree {
            root: 0,
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
        let (main_idx, panel) = self.create_panel(0);
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.menu_container = main_idx;

        let (_, label) = self.create_label(main_idx);
        label.text = "Main Menu".to_string();
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (resume_idx, btn) = self.create_button(main_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::CloseMenu);
        let (_, label) = self.create_label(resume_idx);
        label.text = "Resume".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::OpenGameOptions);
        let (_, label) = self.create_label(b_idx);
        label.text = "Game Options".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::OpenSystemOptions);
        let (_, label) = self.create_label(b_idx);
        label.text = "System Options".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::OpenKeybinds);
        let (_, label) = self.create_label(b_idx);
        label.text = "Keybinds".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(main_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 584.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::ExitApp);
        let (_, label) = self.create_label(b_idx);
        label.text = "Quit".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── Game Options ─────────────────────────────────────────────────────
        let (game_idx, panel) = self.create_panel(0);
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.game_container = game_idx;

        let (_, label) = self.create_label(game_idx);
        label.text = "Game Options".to_string();
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (b_idx, btn) = self.create_button(game_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::BackToMain);
        let (_, label) = self.create_label(b_idx);
        label.text = "Back".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── System Options ───────────────────────────────────────────────────
        let (sys_idx, panel) = self.create_panel(0);
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.system_container = sys_idx;

        let (_, label) = self.create_label(sys_idx);
        label.text = "System Options".to_string();
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        // V-Sync row
        let (row_idx, panel) = self.create_panel(sys_idx);
        panel.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        panel.color       = row_color;
        let (_, label) = self.create_label(row_idx);
        label.text = "V-Sync".to_string();
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        let vsync_selected = self.pending.vsync;
        let (_, checkbox) = self.create_checkbox(row_idx);
        checkbox.base.set_position(Anchor::Right, -8.0, 0.0);
        checkbox.base.set_size(32.0, 32.0);
        checkbox.selected               = vsync_selected;
        checkbox.hover_color            = Some(button_hover_color);
        checkbox.setting                = Some(SettingKey::Vsync);
        checkbox.interaction.on_release = Some(UiAction::ToggleVsync);

        let (b_idx, btn) = self.create_button(sys_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::ApplySettings);
        let (_, label) = self.create_label(b_idx);
        label.text = "Accept".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (b_idx, btn) = self.create_button(sys_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::BackToMain);
        let (_, label) = self.create_label(b_idx);
        label.text = "Back".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── Keybinds ─────────────────────────────────────────────────────────
        let (keybind_idx, panel) = self.create_panel(0);
        panel.base.bounds  = menu_rect;
        panel.color        = panel_color;
        panel.base.visible = false;
        self.keybind_container = keybind_idx;

        let (_, label) = self.create_label(keybind_idx);
        label.text = "Keybinds".to_string();
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (b_idx, btn) = self.create_button(keybind_idx);
        btn.base.bounds            = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.color                  = button_color;
        btn.hover_color            = Some(button_hover_color);
        btn.interaction.on_release = Some(UiAction::BackToMain);
        let (_, label) = self.create_label(b_idx);
        label.text = "Back".to_string();
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── World UI ─────────────────────────────────────────────────────────
        let (world_idx, world) = self.create_container(0);
        world.base.set_size(screen_width, screen_height);
        self.world_container = world_idx;

        let total_w = HOTBAR_SLOTS as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
        let (hotbar_idx, hotbar) = self.create_panel(world_idx);
        hotbar.base.set_position(Anchor::Bottom, 0.0, -SLOT_MARGIN_BOTTOM);
        hotbar.base.set_size(total_w, SLOT_SIZE + SLOT_GAP);
        hotbar.color = panel_color;

        for i in 0..HOTBAR_SLOTS {
            let x = i as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
            let (_, slot) = self.create_button(hotbar_idx);
            slot.base.set_position(Anchor::TopLeft, x, SLOT_GAP / 2.0);
            slot.base.set_size(SLOT_SIZE, SLOT_SIZE);
            slot.color = Rgba::new(0.0, 0.0, 0.0, 0.6);
        }

        // ── Title screen ─────────────────────────────────────────────────────
        let (title_idx, title) = self.create_panel(0);
        title.base.set_size(screen_width, screen_height);
        title.color        = Rgba::new(0.0, 0.0, 0.0, 1.0);
        title.base.visible = false;
        self.title_container = title_idx;

        let (_, label) = self.create_label(title_idx);
        label.text = "Playground".to_string();
        label.base.set_position(Anchor::Top, 0.0, 80.0);

        let (start_idx, start_btn) = self.create_button(title_idx);
        start_btn.base.set_position(Anchor::Center, 0.0, 0.0);
        start_btn.base.set_size(200.0, 48.0);
        start_btn.color                  = Rgba::new(1.0, 1.0, 1.0, 1.0);
        start_btn.hover_color            = Some(Rgba::new(0.2, 0.5, 1.0, 1.0));
        start_btn.interaction.on_release = Some(UiAction::CloseMenu);
        let (_, label) = self.create_label(start_idx);
        label.text = "Start".to_string();
        label.base.set_position(Anchor::Left, 64.0, 0.0);

        let (quit_idx, quit_btn) = self.create_button(title_idx);
        quit_btn.base.set_position(Anchor::Center, 0.0, 64.0);
        quit_btn.base.set_size(200.0, 48.0);
        quit_btn.color                  = Rgba::new(1.0, 1.0, 1.0, 1.0);
        quit_btn.hover_color            = Some(Rgba::new(0.2, 0.5, 1.0, 1.0));
        quit_btn.interaction.on_release = Some(UiAction::ExitApp);
        let (_, label) = self.create_label(quit_idx);
        label.text = "Quit".to_string();
        label.base.set_position(Anchor::Left, 64.0, 0.0);

        // Reapply visibility in case the tree was rebuilt mid-session
        if self.state != MenuState::World {
            let visible_idx = match self.state {
                MenuState::World         => self.world_container,
                MenuState::Title         => self.title_container,
                MenuState::Main          => self.menu_container,
                MenuState::GameOptions   => self.game_container,
                MenuState::SystemOptions => self.system_container,
                MenuState::Keybinds      => self.keybind_container,
            };
            self.tree.nodes[visible_idx].base_mut().visible = true;
            self.tree.nodes[self.world_container].base_mut().visible = false;
        }
    }

    pub fn toggle_menu(&mut self, window: &Window) {
        self.dirty = true;
        if self.state == MenuState::World {
            self.state = MenuState::Main;
            self.tree.nodes[self.menu_container].base_mut().visible = true;
            self.tree.nodes[self.world_container].base_mut().visible = false;
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

            let current_idx = match self.state {
                MenuState::World         => self.world_container,
                MenuState::Title         => self.title_container,
                MenuState::Main          => self.menu_container,
                MenuState::GameOptions   => self.game_container,
                MenuState::SystemOptions => self.system_container,
                MenuState::Keybinds      => self.keybind_container,
            };
            self.tree.nodes[current_idx].base_mut().visible = false;
            self.tree.nodes[self.world_container].base_mut().visible = true;
            self.state = MenuState::World;
            window.set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .expect("Failed to grab cursor");
            window.set_cursor_visible(false);
        }
    }

    pub fn menu_opened(&self) -> bool {
        self.state != MenuState::World
    }

    pub fn is_title_screen(&self) -> bool {
        self.state == MenuState::Title
    }

    /// Resets pending settings to the currently applied values so the settings
    /// menu always shows the real state when opened.
    pub fn sync_pending(&mut self, settings: PendingSettings) {
        self.pending = settings;
        self.sync_nodes_from_pending();
    }

    /// Walks all nodes and syncs any checkbox bound via [`SettingKey`] to the
    /// corresponding value in [`pending`].
    fn sync_nodes_from_pending(&mut self) {
        for node in &mut self.tree.nodes {
            if let UiNode::Checkbox(c) = node {
                if let Some(key) = c.setting {
                    c.selected = match key {
                        SettingKey::Vsync => self.pending.vsync,
                    };
                }
            }
        }
    }

    pub fn handle_input(&mut self, input: &InputManager) -> Option<UiAction> {
        if self.state == MenuState::World { return None; }

        let cursor = input.cursor();
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
            let action = if input.is_released(Action::PrimaryAction) {
                match &self.tree.nodes[idx] {
                    UiNode::Button(b)   => b.interaction.on_release.clone(),
                    UiNode::Checkbox(c) => c.interaction.on_release.clone(),
                    _ => None,
                }
            } else { None };

            if let Some(action) = action {
                match action {
                    UiAction::OpenKeybinds      => self.navigate(MenuState::Keybinds),
                    UiAction::OpenGameOptions   => self.navigate(MenuState::GameOptions),
                    UiAction::OpenSystemOptions => self.navigate(MenuState::SystemOptions),
                    UiAction::BackToMain        => self.navigate(MenuState::Main),
                    UiAction::ToggleVsync       => {
                        self.pending.vsync = !self.pending.vsync;
                        if let UiNode::Checkbox(c) = &mut self.tree.nodes[idx] {
                            c.selected = self.pending.vsync;
                        }
                        self.dirty_nodes.push(idx);
                    },
                    UiAction::ApplySettings     => {
                        self.navigate(MenuState::Main);
                        return Some(UiAction::ApplySettings);
                    }
                    UiAction::CloseMenu | UiAction::ExitApp => return Some(action),
                }
            }
        }

        None
    }

    fn navigate(&mut self, new_state: MenuState) {
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

        let idx_for = |state: MenuState| match state {
            MenuState::World         => self.world_container,
            MenuState::Title         => self.title_container,
            MenuState::Main          => self.menu_container,
            MenuState::GameOptions   => self.game_container,
            MenuState::SystemOptions => self.system_container,
            MenuState::Keybinds      => self.keybind_container,
        };
        self.tree.nodes[idx_for(self.state)].base_mut().visible = false;
        self.tree.nodes[idx_for(new_state)].base_mut().visible = true;
        self.state = new_state;
        self.sync_nodes_from_pending();
        self.dirty = true;
    }
}
