#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use blitz::{Blitz, Container, Pos2, Rgba, VERTEX_2D_RGBA, VertexBufferId};
use cgmath::Vector4;
use winit::{dpi::{LogicalPosition, PhysicalSize}, window::{CursorGrabMode, Window}};

use crate::input::{Action, InputManager};

const HOTBAR_SLOTS:    usize = 10;
const SLOT_SIZE:       f32   = 48.0;
const SLOT_GAP:        f32   = 4.0;
const SLOT_MARGIN_BOTTOM: f32 = 20.0;

const XH_SIZE:      f32 = 16.0; // half-length of each arm
const XH_THICKNESS: f32 = 2.0;  // half-thickness of each arm

const TOTAL_QUADS: usize = HOTBAR_SLOTS + 2; // hotbar + crosshair (horizontal + vertical)

trait BaseWidget {
    fn get_parent(&self) -> Option<usize>;
    fn get_children(&self) -> Vec<usize>;
    fn get_bounds(&self) -> Edges;
    fn add_child(&mut self, node: UiNode, parent_idx: usize);
}

#[derive(PartialEq, Debug)]
enum MenuState {
    None,
    Main,
    GameOptions,
    SystemOptions,
    Keybinds,
}

#[derive(PartialEq, Debug)]
pub enum Widget {
    Container,  // Doesn't render, just there as a collection
    Panel,      // Same as container, but actually renders
    Button,
    Label,
}

#[derive(Clone, Debug)]
pub enum UiAction {
    Test(String),
    CloseMenu,
    ExitApp,
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
            left: parent.left + self.x,
            right: parent.left + self.x + self.width,
            top: parent.top + self.y,
            bottom: parent.top + self.y + self.height,
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

#[derive(Debug)]
pub struct UiNode {
    pub bounds: Rect,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub widget: Widget,
    pub visible: bool,
    pub color: Vector4<f32>,
    on_click: Option<UiAction>,
    on_release: Option<UiAction>,
    on_hover: Option<UiAction>,
}

impl UiNode {
    pub fn new(widget: Widget, rect: Rect) -> Self {
        Self {
            bounds: rect,
            parent: None,
            children: Vec::new(),
            widget,
            visible: true,
            color: Vector4 { x: 0.0, y: 0.0, z: 0.0, w: 0.0 },

            on_click: None,
            on_release: None,
            on_hover: None,
        }
    }
}

#[derive(Debug)]
pub struct UiTree {
    pub nodes: Vec<UiNode>, // All nodes in one flat Vec
    pub root: usize,        // Index of root node
}

impl UiTree {
    pub fn default(area: PhysicalSize<u32>) -> Self {
        Self {
            root: 0,
            nodes: vec! [
                UiNode::new(
                    Widget::Container,
                    Rect { x: 0.0, y: 0.0, width: area.width as f32, height: area.height as f32 },
                )
            ],
        }
    }

    /// Assumes that a UiNode is never removed
    pub fn add_child(&mut self, node: UiNode, parent_idx: usize) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        self.nodes[parent_idx].children.push(idx);
        self.nodes[idx].parent = Some(parent_idx);
        idx
    }

    pub fn hit_test(&self, mouse_x: f32, mouse_y: f32, node_idx: usize, parent_edges: &Edges) -> Option<usize> {
        let node = &self.nodes[node_idx];
        if !node.visible {
            return None;
        }

        let edges = node.bounds.edges(parent_edges);
        if !edges.contains(mouse_x, mouse_y) {
            return None;
        }

        for &child_idx in &node.children {
            if let Some(hit) = self.hit_test(mouse_x, mouse_y, child_idx, &edges) {
                return Some(hit)
            }
        }

        Some(node_idx)
    }
}

#[derive(Debug)]
pub struct Ui {
    pub dirty: bool,
    quad_count: usize,
    vertex_id: VertexBufferId,
    hotbar_size: (u32, u32),
    mouse_store: (f32, f32),

    tree: UiTree,
    state: MenuState,
}

impl Ui {
    pub fn new(window: &Window, blitz: &Blitz) -> Self {
        let area = window.inner_size();
        let mouse = ((area.width / 2) as f32, (area.height / 2) as f32);
        let mut this = Self {
            dirty: true,
            quad_count: 0,
            vertex_id: blitz.ui_vertex_id(),
            hotbar_size: (0, 0),
            mouse_store: mouse, // Store the old mouse position whenever the menu reopens

            state: MenuState::None,
            tree: UiTree::default(area),
        };
        this.generate_tree(area.width as f32, area.height as f32);

        this
    }

pub unsafe fn flush(&mut self, container: &mut Container, screen: (f32, f32)) {
        self.dirty = false;
        let mut verts: Vec<VERTEX_2D_RGBA> = Vec::new();

        if self.state == MenuState::None {  // Render the crosshair when the menu is not open
            // Crosshair
            let cx = screen.0 / 2.0; // Screen width
            let cy = screen.1 / 2.0; // Screen height
            let xh_color = Rgba::new(1.0, 1.0, 1.0, 0.1);

            // Horizontal bar
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_SIZE, y: cy - XH_THICKNESS}, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_SIZE, y: cy - XH_THICKNESS}, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_SIZE, y: cy + XH_THICKNESS}, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_SIZE, y: cy + XH_THICKNESS}, xh_color));

            // Vertical bar
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_THICKNESS, y: cy - XH_SIZE}, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_THICKNESS, y: cy - XH_SIZE}, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx + XH_THICKNESS, y: cy + XH_SIZE}, xh_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx - XH_THICKNESS, y: cy + XH_SIZE}, xh_color));
        }

        let root_edges = self.tree.nodes[0].bounds.edges(&Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 });
        let mut stack: Vec<(usize, Edges)> = vec![(0, root_edges)];

        while !stack.is_empty() {
            let (node_idx, parent_edges) = stack.pop().unwrap();
            for &child_idx in &self.tree.nodes[node_idx].children {
                let child = &self.tree.nodes[child_idx];
                if child.visible {
                    let e = child.bounds.edges(&parent_edges);
                    verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.left,  y: e.top    }, child.color));
                    verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.right, y: e.top    }, child.color));
                    verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.right, y: e.bottom }, child.color));
                    verts.push(VERTEX_2D_RGBA::new(Pos2 { x: e.left,  y: e.bottom }, child.color));
                    stack.push((child_idx, e));
                }
            }
        }
        self.quad_count = verts.len() / 4;
        container.stage_vertex_update(self.vertex_id, &verts);
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        blitz.draw_ui_quads(0, self.quad_count);
    }

    fn generate_tree(&mut self, screen_width: f32, screen_height: f32) {
        // Main menu
        let menu_idx = self.tree.add_child(UiNode::new(
            Widget::Panel,
            Rect { x: 0.0, y: 0.0, width: screen_width / 2.0, height: screen_height as f32 },
        ), 0);
        self.tree.nodes[menu_idx].color = Rgba::new(0.8, 0.8, 0.8, 0.2);
        self.tree.nodes[menu_idx].visible = false;

        // Hotbar
        let total_w = HOTBAR_SLOTS as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
        let x0 = (screen_width - total_w) / 2.0;
        let y0 = screen_height - SLOT_SIZE - SLOT_MARGIN_BOTTOM - SLOT_GAP;
        let hotbar_idx = self.tree.add_child(UiNode::new(
            Widget::Panel,
            Rect { x: x0, y: y0, width: total_w, height: SLOT_SIZE + SLOT_GAP }
        ), 0);
        self.tree.nodes[hotbar_idx].color = Rgba::new(0.8, 0.8, 0.8, 0.2);

        // Menu items
        let resume_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 200.0, width: 300.0, height: 25.0 },
        ), menu_idx);
        self.tree.nodes[resume_idx].on_release = Some(UiAction::CloseMenu);
        self.tree.nodes[resume_idx].color = Rgba::new(0.5, 0.5, 0.5, 0.4 );

        let game_options_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 250.0, width: 300.0, height: 25.0 },
        ), menu_idx);
        self.tree.nodes[game_options_idx].on_release = Some(UiAction::Test("GameOptions".to_owned()));
        self.tree.nodes[game_options_idx].color = Rgba::new(0.5, 0.5, 0.5, 0.4);

        let system_options_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 300.0, width: 300.0, height: 25.0 },
        ), menu_idx);
        self.tree.nodes[system_options_idx].on_release = Some(UiAction::Test("SystemOptions".to_owned()));
        self.tree.nodes[system_options_idx].color = Rgba::new(0.5, 0.5, 0.5, 0.4);

        let keybinds_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 350.0, width: 300.0, height: 25.0 },
        ), menu_idx);
        self.tree.nodes[keybinds_idx].on_release = Some(UiAction::Test("Keybindings".to_owned()));
        self.tree.nodes[keybinds_idx].color = Rgba::new(0.5, 0.5, 0.5, 0.4);

        let quit_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 400.0, width: 300.0, height: 25.0 },
        ), menu_idx);
        self.tree.nodes[quit_idx].on_release = Some(UiAction::ExitApp);
        self.tree.nodes[quit_idx].color = Rgba::new(0.5, 0.5, 0.5, 0.4);

        // Hotbar slots — coords are relative to the hotbar panel
        for i in 0..HOTBAR_SLOTS {
            let x = i as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
            let slot_idx = self.tree.add_child(UiNode::new(
                Widget::Button,
                Rect { x, y: SLOT_GAP / 2.0, width: SLOT_SIZE, height: SLOT_SIZE }
            ), hotbar_idx);
            self.tree.nodes[slot_idx].color = Rgba::new(0.0, 0.0, 0.0, 0.6);
        }
    }

    /// Return true if in menu
    pub fn toggle_menu(&mut self, window: &Window) {
        // Node id 0: UiParent
        // Node id 1: Main Menu
        // Node id 2: Hotbar

        self.dirty = true;
        if self.state == MenuState::None {
            self.state = MenuState::Main;
            self.tree.nodes[1].visible = true;
            self.tree.nodes[2].visible = false;
            window.set_cursor_grab(CursorGrabMode::None)
                .expect("Failed to free cursor");
            window.set_cursor_position(LogicalPosition::new(self.mouse_store.0, self.mouse_store.1))
                .expect("Failed to set cursor position");
            window.set_cursor_visible(true);
        } else {
            self.state = MenuState::None;
            self.tree.nodes[1].visible = false;
            self.tree.nodes[2].visible = true;
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
        if self.state == MenuState::None {
            return None;
        }

        let cursor = input.cursor();
        let hit = self.tree.hit_test(
            cursor.0,
            cursor.1,
            0,
            &Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        );

        if let Some(idx) = hit {
            if input.is_released(Action::PrimaryAction) {
                let action = &self.tree.nodes[idx].on_release;
                if let Some(action) = action {
                    match action {
                        UiAction::Test(s)  => println!("{s}"),
                        UiAction::OpenKeybinds      => println!("Keybinds"),
                        UiAction::OpenGameOptions   => println!("Game Options"),
                        UiAction::OpenSystemOptions => println!("System Options"),
                        UiAction::CloseMenu | UiAction::ExitApp => return Some(action.clone()),
                    }
                }
            }
        }

        None
    }
}
