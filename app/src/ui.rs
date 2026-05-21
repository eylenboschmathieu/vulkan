#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use blitz::{Blitz, Container, Pos2, Rgba, VERTEX_2D_RGBA, VertexBufferId};
use winit::{dpi::{LogicalPosition, PhysicalSize}, window::{CursorGrabMode, Window}};

use crate::input::{Action, InputManager};

const HOTBAR_SLOTS:    usize = 10;
const SLOT_SIZE:       f32   = 48.0;
const SLOT_GAP:        f32   = 4.0;
const SLOT_MARGIN_BOTTOM: f32 = 20.0;

const XH_SIZE:      f32 = 16.0; // half-length of each arm
const XH_THICKNESS: f32 = 2.0;  // half-thickness of each arm

const TOTAL_QUADS: usize = HOTBAR_SLOTS + 2; // hotbar + crosshair (horizontal + vertical)

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

#[derive(Debug)]
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
    on_click: Option<UiAction>,
    on_release: Option<UiAction>,
    on_hover: Option<UiAction>,
}

impl UiNode {
    pub fn new(widget: Widget, rect: Rect, parent: usize) -> Self {
        Self {
            bounds: rect,
            parent: Some(parent),
            children: vec![],
            widget,
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
                UiNode {  // Root node
                    bounds: Rect { x: 0.0, y: 0.0, width: area.width as f32, height: area.height as f32 },
                    parent: None,
                    children: Vec::new(),
                    widget: Widget::Container,
                    on_click: None,
                    on_release: None,
                    on_hover: None,
                }
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
            vertex_id: blitz.ui_vertex_id(),
            hotbar_size: (0, 0),
            mouse_store: mouse, // Store the old mouse position whenever the menu reopens

            state: MenuState::None,
            tree: UiTree::default(area),
        };
        this.generate_tree(area.width as f32, area.height as f32);

        this
    }

    pub fn is_dirty(&self, size: (u32, u32)) -> bool {
        self.hotbar_size != size
    }

    pub unsafe fn flush(&mut self, container: &mut Container, size: (u32, u32)) {
        if self.hotbar_size != size {
            self.hotbar_size = size;
            let verts = Self::generate_ui(size.0 as f32, size.1 as f32);
            container.stage_vertex_update(self.vertex_id, &verts);
        }
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) {
        match self.state {
            MenuState::None => {
                blitz.draw_ui_quads(0, HOTBAR_SLOTS + 2)
            },
            MenuState::Main => blitz.draw_ui_quads(HOTBAR_SLOTS + 2, 6),
            MenuState::GameOptions |
            MenuState::SystemOptions |
            MenuState::Keybinds => blitz.draw_ui_quads(HOTBAR_SLOTS + 2, 6),
        }
    }

    fn generate_tree(&mut self, screen_width: f32, screen_height: f32) {
        let menu_idx = self.tree.add_child(UiNode::new(
            Widget::Panel,
            Rect { x: 0.0, y: 0.0, width: screen_width / 2.0, height: screen_height as f32 },
            0
        ), 0);
        let resume_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 200.0, width: 300.0, height: 25.0 },
            menu_idx
        ), menu_idx);
        self.tree.nodes[resume_idx].on_release = Some(UiAction::CloseMenu);

        let game_options_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 250.0, width: 300.0, height: 25.0 },
            menu_idx
        ), menu_idx);
        self.tree.nodes[game_options_idx].on_release = Some(UiAction::Test("GameOptions".to_owned()));

        let system_options_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 300.0, width: 300.0, height: 25.0 },
            menu_idx
        ), menu_idx);
        self.tree.nodes[system_options_idx].on_release = Some(UiAction::Test("SystemOptions".to_owned()));

        let keybinds_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 350.0, width: 300.0, height: 25.0 },
            menu_idx
        ), menu_idx);
        self.tree.nodes[keybinds_idx].on_release = Some(UiAction::Test("Keybindings".to_owned()));

        let quit_idx = self.tree.add_child(UiNode::new(
            Widget::Button,
            Rect { x: 100.0, y: 400.0, width: 300.0, height: 25.0 },
            menu_idx
        ), menu_idx);
        self.tree.nodes[quit_idx].on_release = Some(UiAction::ExitApp);

    }

    fn generate_ui(screen_width: f32, screen_height: f32) -> Vec<VERTEX_2D_RGBA> {
        let mut verts = Vec::with_capacity(TOTAL_QUADS * 4);

        // Hotbar
        let total_w = HOTBAR_SLOTS as f32 * SLOT_SIZE + (HOTBAR_SLOTS - 1) as f32 * SLOT_GAP;
        let x0 = (screen_width - total_w) / 2.0;
        let y0 = screen_height - SLOT_SIZE - SLOT_MARGIN_BOTTOM;
        let slot_color = Rgba::new(0.25, 0.25, 0.25, 1.0);
        for i in 0..HOTBAR_SLOTS {
            let x = x0 + i as f32 * (SLOT_SIZE + SLOT_GAP);
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: x,             y: y0             }, slot_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: x + SLOT_SIZE, y: y0             }, slot_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: x + SLOT_SIZE, y: y0 + SLOT_SIZE }, slot_color));
            verts.push(VERTEX_2D_RGBA::new(Pos2 { x: x,             y: y0 + SLOT_SIZE }, slot_color));
        }

        // Crosshair
        let cx = screen_width  / 2.0;
        let cy = screen_height / 2.0;
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

        // Main menu
        let menu_color = Rgba::new(0.8, 0.8, 0.8, 0.2);
        let button_color = Rgba::new(0.5, 0.5, 0.5, 0.4);
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 0.0, y: 0.0                  }, menu_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx,  y: 0.0                  }, menu_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: cx,  y: screen_height }, menu_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 0.0, y: screen_height }, menu_color));

        // Resume button
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 200.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 200.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 225.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 225.0 }, button_color));

        // Keybinds
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 250.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 250.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 275.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 275.0 }, button_color));

        // Game options
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 300.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 300.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 325.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 325.0 }, button_color));

        // System options
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 350.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 350.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 375.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 375.0 }, button_color));

        // Quit
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 400.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 400.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 400.0, y: 425.0 }, button_color));
        verts.push(VERTEX_2D_RGBA::new(Pos2 { x: 100.0, y: 425.0 }, button_color));

        verts
    }

    /// Return true if in menu
    pub fn toggle_menu(&mut self, window: &Window) {
        if self.state == MenuState::None {
            self.state = MenuState::Main;
            window.set_cursor_grab(CursorGrabMode::None)
                .expect("Failed to free cursor");
            window.set_cursor_position(LogicalPosition::new(self.mouse_store.0, self.mouse_store.1))
                .expect("Failed to set cursor position");
            window.set_cursor_visible(true);
        } else {
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
        if self.state == MenuState::None {
            return None;
        }

        let cursor = input.cursor();
        let root = &self.tree.nodes[0];
        let hit = self.tree.hit_test(
            cursor.0,
            cursor.1,
            1,
            &root.bounds.edges(&Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 })
        );

        if let Some(idx) = hit {
            if input.is_released(Action::PrimaryAction) {
                let action = self.tree.nodes[idx].on_release.clone();
                if let Some(action) = action {
                    match &action {
                        UiAction::Test(s)  => println!("{s}"),
                        UiAction::OpenKeybinds      => println!("Keybinds"),
                        UiAction::OpenGameOptions   => println!("Game Options"),
                        UiAction::OpenSystemOptions => println!("System Options"),
                        UiAction::CloseMenu | UiAction::ExitApp => return Some(action),
                    }
                }
            }
        }

        None
    }
}