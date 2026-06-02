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
    None,
    Main,
    GameOptions,
    SystemOptions,
    Keybinds,
}

#[derive(Clone, Debug)]
pub enum UiAction {
    Test(String),
    CloseMenu,
    ExitApp,
    BackToMain,
    OpenKeybinds,
    OpenGameOptions,
    OpenSystemOptions,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct NodeBase {
    pub bounds: Rect,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub visible: bool,
    pub vertex_offset: usize,
}

impl NodeBase {
    pub fn new(bounds: Rect) -> Self {
        Self { bounds, parent: None, children: Vec::new(), visible: true, vertex_offset: 0 }
    }
}

/// Invisible grouping node — children only, no quad rendered.
#[derive(Debug)]
pub struct ContainerNode {
    pub base: NodeBase,
}

impl ContainerNode {
    pub fn new(bounds: Rect) -> Self {
        Self { base: NodeBase::new(bounds) }
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
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: NodeBase::new(bounds),
            color: Rgba::new(0.0, 0.0, 0.0, 0.0),
            uv_min: [0.0, 0.0],
            uv_max: [0.0, 0.0],
        }
    }
}

/// Interactive button. Labelable.
#[derive(Debug)]
pub struct ButtonNode {
    pub base: NodeBase,
    pub color: Rgba,
    pub hover_color: Option<Rgba>,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub on_pressed: Option<UiAction>,
    pub on_release: Option<UiAction>,
    pub on_enter:   Option<UiAction>,
    pub on_leave:   Option<UiAction>,
}

impl ButtonNode {
    pub fn new(bounds: Rect) -> Self {
        Self {
            base: NodeBase::new(bounds),
            color: Rgba::new(0.0, 0.0, 0.0, 0.0),
            hover_color: None,
            uv_min: [0.0, 0.0],
            uv_max: [0.0, 0.0],
            on_pressed: None,
            on_release: None,
            on_enter:   None,
            on_leave:   None,
        }
    }
}

/// Interactive checkbox button. Labelable.
#[derive(Debug)]
pub struct CheckboxNode {
    container: ContainerNode,
    button: ButtonNode,
    selected_color: Rgba,
    selected: bool,
}

impl CheckboxNode {
    pub fn new(bounds: Rect) -> Self {
        let button_size = bounds.height;
        Self {
            container: ContainerNode::new(bounds),
            button: ButtonNode::new(Rect {x: 0.0, y: 0.0, width: button_size, height: button_size }),
            selected_color: Rgba::new(0.0, 0.0, 1.0, 0.5),
            selected: false,
        }
    }

    // If text is positioned left, set the button on the right and vica versa.
    pub fn set_text_position(left: bool) { // TODO

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
    pub fn new(bounds: Rect, text: impl Into<String>) -> Self {
        Self {
            base: NodeBase::new(bounds),
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
    Label(LabelNode),
}

impl UiNode {
    pub fn base(&self) -> &NodeBase {
        match self {
            UiNode::Container(n) => &n.base,
            UiNode::Panel(n)     => &n.base,
            UiNode::Button(n)    => &n.base,
            UiNode::Label(n)     => &n.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut NodeBase {
        match self {
            UiNode::Container(n) => &mut n.base,
            UiNode::Panel(n)     => &mut n.base,
            UiNode::Button(n)    => &mut n.base,
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
        Self {
            root: 0,
            nodes: vec![UiNode::Container(ContainerNode::new(
                Rect { x: 0.0, y: 0.0, width: area.width as f32, height: area.height as f32 },
            ))],
        }
    }

    pub fn add_child(&mut self, mut node: UiNode, parent_idx: usize) -> usize {
        let idx = self.nodes.len();
        node.base_mut().parent = Some(parent_idx);
        self.nodes.push(node);
        self.nodes[parent_idx].base_mut().children.push(idx);
        idx
    }

    /// Adds a text label as a child of a Panel or Button. Panics for other node types.
    pub fn add_label(&mut self, parent_idx: usize, x: f32, y: f32, text: impl Into<String>) -> usize {
        assert!(
            matches!(&self.nodes[parent_idx], UiNode::Container(_) | UiNode::Panel(_) | UiNode::Button(_)),
            "add_label: only Panel and Button nodes can contain labels"
        );
        self.add_child(UiNode::Label(LabelNode::new(Rect { x, y, width: 0.0, height: 0.0 }, text)), parent_idx)
    }

    pub fn hit_test(&self, mx: f32, my: f32, node_idx: usize, parent_edges: &Edges) -> Option<usize> {
        let node = &self.nodes[node_idx];
        if !node.base().visible { return None; }

        let edges = node.base().bounds.edges(parent_edges);
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
    main_container:   usize,
    game_container:   usize,
    system_container: usize,
    keybind_container: usize,
    hotbar_container: usize,
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
            state: MenuState::None,
            hovered_node: None,
            dirty_nodes: Vec::new(),
            tree: UiTree::default(area),

            main_container:    0,
            game_container:    0,
            system_container:  0,
            keybind_container: 0,
            hotbar_container:  0,
        };
        this.generate_tree(area.width as f32, area.height as f32);
        this
    }

    pub unsafe fn flush_all(&mut self, container: &mut Container, screen: (f32, f32)) {
        self.dirty = false;
        let atlas = &*self.font_atlas;
        let mut verts: Vec<VERTEX_2D_RGBA> = Vec::new();

        // Crosshair
        if self.state == MenuState::None {
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

        let root_edges = self.tree.nodes[0].base().bounds.edges(&Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 });
        let mut stack: Vec<(usize, Edges)> = vec![(0, root_edges)];

        while !stack.is_empty() {
            let (node_idx, parent_edges) = stack.pop().unwrap();
            let child_count = self.tree.nodes[node_idx].base().children.len();
            for i in 0..child_count {
                let child_idx = self.tree.nodes[node_idx].base().children[i];
                if !self.tree.nodes[child_idx].base().visible { continue; }

                let e = self.tree.nodes[child_idx].base().bounds.edges(&parent_edges);

                match &self.tree.nodes[child_idx] {
                    UiNode::Label(l) => {
                        let mut cursor_x = e.left;
                        let baseline_y   = e.top + atlas.line_height;
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
                            UiNode::Button(b) => Some((b.color, b.uv_min, b.uv_max)),
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
                UiNode::Button(b) => Some((b.color, b.uv_min, b.uv_max)),
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
        node.base().bounds.edges(&parent_edges)
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        blitz.draw_ui_quads(0, self.quad_count, self.font_atlas.texture_id);
    }

    pub fn has_dirty_nodes(&self) -> bool {
        !self.dirty_nodes.is_empty()
    }

    pub fn generate_tree(&mut self, screen_width: f32, screen_height: f32) {
        self.tree = UiTree {
            root: 0,
            nodes: vec![UiNode::Container(ContainerNode::new(
                Rect { x: 0.0, y: 0.0, width: screen_width, height: screen_height },
            ))],
        };
        self.hovered_node = None;
        self.dirty_nodes.clear();

        let white              = self.font_atlas.white_uv;
        let panel_color        = Rgba::new(0.8, 0.8, 0.8, 0.2);
        let button_color       = Rgba::new(0.5, 0.5, 0.5, 0.4);
        let button_hover_color = Rgba::new(0.65, 0.65, 0.65, 0.5);
        let panel_w            = screen_width / 2.0;

        let make_panel = |visible: bool| -> PanelNode {
            let mut p = PanelNode::new(Rect { x: 0.0, y: 0.0, width: panel_w, height: screen_height });
            p.color       = panel_color;
            p.uv_min      = white;
            p.uv_max      = white;
            p.base.visible = visible;
            p
        };

        let make_button = |rect: Rect, action: UiAction| -> ButtonNode {
            let mut b = ButtonNode::new(rect);
            b.on_release  = Some(action);
            b.color       = button_color;
            b.hover_color = Some(button_hover_color);
            b.uv_min      = white;
            b.uv_max      = white;
            b
        };

        // ── Main menu ────────────────────────────────────────────────────────
        let main_idx = self.tree.add_child(UiNode::Panel(make_panel(false)), 0);
        self.main_container = main_idx;
        self.tree.add_label(main_idx, 64.0, 120.0, "Main Menu");

        let resume_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 }, UiAction::CloseMenu)), main_idx);
        self.tree.add_label(resume_idx, 64.0, 0.0, "Resume");

        let b_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 }, UiAction::OpenGameOptions)), main_idx);
        self.tree.add_label(b_idx, 64.0, 0.0, "Game Options");

        let b_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 }, UiAction::OpenSystemOptions)), main_idx);
        self.tree.add_label(b_idx, 64.0, 0.0, "System Options");

        let b_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 }, UiAction::OpenKeybinds)), main_idx);
        self.tree.add_label(b_idx, 64.0, 0.0, "Keybinds");

        let b_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 584.0, width: 400.0, height: 48.0 }, UiAction::ExitApp)), main_idx);
        self.tree.add_label(b_idx, 64.0, 0.0, "Quit");

        // ── Game Options ─────────────────────────────────────────────────────
        let game_idx = self.tree.add_child(UiNode::Panel(make_panel(false)), 0);
        self.game_container = game_idx;
        self.tree.add_label(game_idx, 64.0, 120.0, "Game Options");

        let b_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 }, UiAction::BackToMain)), game_idx);
        self.tree.add_label(b_idx, 64.0, 0.0, "Back");

        // ── System Options ───────────────────────────────────────────────────
        let sys_idx = self.tree.add_child(UiNode::Panel(make_panel(false)), 0);
        self.system_container = sys_idx;
        self.tree.add_label(sys_idx, 64.0, 120.0, "System Options");

        let b_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 }, UiAction::BackToMain)), sys_idx);
        self.tree.add_label(b_idx, 64.0, 0.0, "Back");

        // ── Keybinds ─────────────────────────────────────────────────────────
        let keybind_idx = self.tree.add_child(UiNode::Panel(make_panel(false)), 0);
        self.keybind_container = keybind_idx;
        self.tree.add_label(keybind_idx, 64.0, 120.0, "Keybinds");

        let b_idx = self.tree.add_child(UiNode::Button(make_button(
            Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 }, UiAction::BackToMain)), keybind_idx);
        self.tree.add_label(b_idx, 64.0, 0.0, "Back");

        // ── Hotbar ───────────────────────────────────────────────────────────
        let total_w = HOTBAR_SLOTS as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
        let x0      = (screen_width - total_w) / 2.0;
        let y0      = screen_height - SLOT_SIZE - SLOT_MARGIN_BOTTOM - SLOT_GAP;
        let mut hotbar = PanelNode::new(Rect { x: x0, y: y0, width: total_w, height: SLOT_SIZE + SLOT_GAP });
        hotbar.color  = panel_color;
        hotbar.uv_min = white;
        hotbar.uv_max = white;
        let hotbar_idx = self.tree.add_child(UiNode::Panel(hotbar), 0);
        self.hotbar_container = hotbar_idx;

        for i in 0..HOTBAR_SLOTS {
            let x = i as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
            let mut slot = ButtonNode::new(Rect { x, y: SLOT_GAP / 2.0, width: SLOT_SIZE, height: SLOT_SIZE });
            slot.color  = Rgba::new(0.0, 0.0, 0.0, 0.6);
            slot.uv_min = white;
            slot.uv_max = white;
            self.tree.add_child(UiNode::Button(slot), hotbar_idx);
        }

        // Reapply visibility in case the tree was rebuilt mid-session
        if self.state != MenuState::None {
            let visible_idx = match self.state {
                MenuState::None          => unreachable!(),
                MenuState::Main          => self.main_container,
                MenuState::GameOptions   => self.game_container,
                MenuState::SystemOptions => self.system_container,
                MenuState::Keybinds      => self.keybind_container,
            };
            self.tree.nodes[visible_idx].base_mut().visible = true;
            self.tree.nodes[self.hotbar_container].base_mut().visible = false;
        }
    }

    pub fn toggle_menu(&mut self, window: &Window) {
        self.dirty = true;
        if self.state == MenuState::None {
            self.state = MenuState::Main;
            self.tree.nodes[self.main_container].base_mut().visible = true;
            self.tree.nodes[self.hotbar_container].base_mut().visible = false;
            window.set_cursor_grab(CursorGrabMode::None)
                .expect("Failed to free cursor");
            window.set_cursor_position(LogicalPosition::new(self.mouse_store.0, self.mouse_store.1))
                .expect("Failed to set cursor position");
            window.set_cursor_visible(true);
        } else {
            let current_idx = match self.state {
                MenuState::None          => unreachable!(),
                MenuState::Main          => self.main_container,
                MenuState::GameOptions   => self.game_container,
                MenuState::SystemOptions => self.system_container,
                MenuState::Keybinds      => self.keybind_container,
            };
            self.tree.nodes[current_idx].base_mut().visible = false;
            self.tree.nodes[self.hotbar_container].base_mut().visible = true;
            self.state = MenuState::None;
            window.set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .expect("Failed to grab cursor");
            window.set_cursor_visible(false);
        }
    }

    pub fn menu_opened(&self) -> bool {
        self.state != MenuState::None
    }

    pub fn handle_input(&mut self, input: &InputManager) -> Option<UiAction> {
        if self.state == MenuState::None { return None; }

        let cursor = input.cursor();
        let hit = self.tree.hit_test(
            cursor.0, cursor.1, 0,
            &Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        );

        if hit != self.hovered_node {
            if let Some(old) = self.hovered_node {
                if let UiNode::Button(b) = &mut self.tree.nodes[old] {
                    if let Some(hc) = b.hover_color.as_mut() {
                        std::mem::swap(&mut b.color, hc);
                        self.dirty_nodes.push(old);
                    }
                }
            }
            if let Some(new) = hit {
                if let UiNode::Button(b) = &mut self.tree.nodes[new] {
                    if let Some(hc) = b.hover_color.as_mut() {
                        std::mem::swap(&mut b.color, hc);
                        self.dirty_nodes.push(new);
                    }
                }
            }
            self.hovered_node = hit;
        }

        if let Some(idx) = hit {
            if let UiNode::Button(b) = &self.tree.nodes[idx] {
                if input.is_released(Action::PrimaryAction) {
                    if let Some(action) = &b.on_release {
                        match action {
                            UiAction::OpenKeybinds      => self.navigate(MenuState::Keybinds),
                            UiAction::OpenGameOptions   => self.navigate(MenuState::GameOptions),
                            UiAction::OpenSystemOptions => self.navigate(MenuState::SystemOptions),
                            UiAction::BackToMain        => self.navigate(MenuState::Main),
                            UiAction::CloseMenu | UiAction::ExitApp => return Some(action.clone()),
                            UiAction::Test(s) => println!("{s}"),
                        }
                    }
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
            if let UiNode::Button(b) = &mut self.tree.nodes[old] {
                if let Some(hc) = b.hover_color.as_mut() {
                    std::mem::swap(&mut b.color, hc);
                }
            }
        }
        self.dirty_nodes.clear();

        let idx_for = |state: MenuState| match state {
            MenuState::None          => unreachable!(),
            MenuState::Main          => self.main_container,
            MenuState::GameOptions   => self.game_container,
            MenuState::SystemOptions => self.system_container,
            MenuState::Keybinds      => self.keybind_container,
        };
        self.tree.nodes[idx_for(self.state)].base_mut().visible = false;
        self.tree.nodes[idx_for(new_state)].base_mut().visible = true;
        self.state = new_state;
        self.dirty = true;
    }
}
