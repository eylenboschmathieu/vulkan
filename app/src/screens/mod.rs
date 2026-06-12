#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{cell::Cell, rc::Rc};

use anyhow::Result;

use ui::{Anchor, CheckboxNode, CursorRequest, PanelNode, Rect, Rgba, SliderNode, Ui};

const HOTBAR_SLOTS:       usize = 10;
const SLOT_SIZE:          f32   = 48.0;
const SLOT_GAP:           f32   = 4.0;
const SLOT_MARGIN_BOTTOM: f32   = 20.0;

const XH_SIZE:      f32 = 16.0;
const XH_THICKNESS: f32 = 2.0;

const DEBUG_PADDING: f32 = 10.0;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Screen {
    World,
    Title,
    Main,
    GameOptions,
    SystemOptions,
    Keybinds,
}

/// Settings staged in the UI and applied when the user hits Accept.
#[derive(Clone, Copy)]
pub struct PendingSettings {
    pub vsync: bool,
    pub fps_cap: u32,
}

impl Default for PendingSettings {
    fn default() -> Self {
        Self { vsync: true, fps_cap: 60 }
    }
}

/// Owns the menu/title/world screens built on top of [`ui::Ui`] and the
/// container indices used to show/hide them. Node callbacks only have
/// `&mut Ui`, so navigation and settings changes are communicated back to
/// `App` via the `Rc<Cell<_>>` handles below: `App` reads and clears them
/// after each [`ui::Ui::handle_input`] call.
pub struct Screens {
    main_idx: usize,
    game_idx: usize,
    sys_idx: usize,
    keybind_idx: usize,
    world_idx: usize,
    title_idx: usize,

    debug_idx: usize,
    debug_cam_idx: usize,
    debug_mode_idx: usize,
    debug_quad_idx: usize,
    debug_fps_idx: usize,

    current: Cell<Screen>,
    /// Logical cursor position to restore when leaving the world screen,
    /// captured once at startup (the world is always entered/exited at
    /// screen-center).
    mouse_center: (f32, f32),

    /// Set by navigation buttons' `on_release`; consumed by `App` via
    /// [`Self::go_to`] after `ui.handle_input`.
    pub nav_request: Rc<Cell<Option<Screen>>>,
    /// System Options' live-edit buffer for vsync/fps settings.
    pub pending: Rc<Cell<PendingSettings>>,
    /// Set by the Accept button alongside writing `pending`.
    pub settings_dirty: Rc<Cell<bool>>,
}

impl Screens {
    pub fn current(&self) -> Screen {
        self.current.get()
    }

    /// Maps a [`Screen`] to the index of the container node it displays.
    fn container_for(&self, screen: Screen) -> usize {
        match screen {
            Screen::World         => self.world_idx,
            Screen::Title         => self.title_idx,
            Screen::Main          => self.main_idx,
            Screen::GameOptions   => self.game_idx,
            Screen::SystemOptions => self.sys_idx,
            Screen::Keybinds      => self.keybind_idx,
        }
    }

    /// Switches the visible screen, hiding `current` and showing `target`
    /// via [`ui::Ui::set_visible`] (which fires `on_hide`/`on_show`). Also
    /// queues a cursor lock/free request when crossing the World <-> menu
    /// boundary.
    pub fn go_to(&self, ui: &mut Ui, target: Screen) -> Result<()> {
        let current = self.current.get();
        if current == target { return Ok(()); }

        ui.set_visible(self.container_for(current), false)?;
        ui.set_visible(self.container_for(target), true)?;

        if target == Screen::World {
            ui.request_cursor(CursorRequest::Lock);
        } else if current == Screen::World {
            ui.request_cursor(CursorRequest::Free { x: self.mouse_center.0, y: self.mouse_center.1 });
        }

        self.current.set(target);
        Ok(())
    }

    /// Shows or hides the debug overlay (camera position, present mode, UI
    /// quad count, FPS).
    pub fn set_debug_visible(&self, ui: &mut Ui, visible: bool) -> Result<()> {
        ui.set_visible(self.debug_idx, visible)
    }

    /// Refreshes the debug overlay's text. Only meaningful while the overlay
    /// is visible — callers should skip this while it's hidden, since label
    /// updates on a hidden subtree aren't reflected until the next full flush.
    pub fn update_debug(&self, ui: &mut Ui, camera_text: impl Into<String>, present_mode_text: impl Into<String>, ui_quad_count: usize, fps: f32) -> Result<()> {
        ui.set_label_text(self.debug_cam_idx, camera_text)?;
        ui.set_label_text(self.debug_mode_idx, present_mode_text)?;
        ui.set_label_text(self.debug_quad_idx, format!("UI quad count: {ui_quad_count}"))?;
        ui.set_label_text(self.debug_fps_idx, format!("{fps:.0} fps"))?;
        Ok(())
    }

    /// Builds the title, main menu, game/system options, keybinds, and world
    /// screens as children of the UI's root container. Navigation buttons
    /// write to `nav_request`, quit buttons call [`ui::Ui::request_exit`],
    /// and the System Options screen reads/writes `pending`/`settings_dirty`.
    pub fn build(ui: &mut Ui, screen_size: (f32, f32)) -> Result<Self> {
        let nav_request    = Rc::new(Cell::new(None));
        let pending        = Rc::new(Cell::new(PendingSettings::default()));
        let settings_dirty = Rc::new(Cell::new(false));

        let panel_color        = Rgba::new(0.8, 0.8, 0.8, 0.2);
        let button_color       = Rgba::new(0.5, 0.5, 0.5, 0.4);
        let button_hover_color = Rgba::new(0.65, 0.65, 0.65, 0.5);
        let row_color          = Rgba::new(0.0, 0.0, 0.0, 0.2);
        let panel_w            = screen_size.0 / 2.0;
        let menu_rect          = Rect { x: 0.0, y: 0.0, width: panel_w, height: screen_size.1 };

        // ── Main menu ────────────────────────────────────────────────────────
        let (main_idx, panel) = ui.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.set_color(panel_color);
        panel.base.visible = false;

        let (_, label) = ui.create_label(main_idx)?;
        label.set_text("Main Menu");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (resume_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::World))
        }));
        let (_, label) = ui.create_label(resume_idx)?;
        label.set_text("Resume");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (game_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::GameOptions))
        }));
        let (_, label) = ui.create_label(game_btn_idx)?;
        label.set_text("Game Options");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (sys_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::SystemOptions))
        }));
        let (_, label) = ui.create_label(sys_btn_idx)?;
        label.set_text("System Options");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (keybind_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::Keybinds))
        }));
        let (_, label) = ui.create_label(keybind_btn_idx)?;
        label.set_text("Keybinds");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (quit_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 584.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(|ui: &mut Ui| ui.request_exit()));
        let (_, label) = ui.create_label(quit_btn_idx)?;
        label.set_text("Quit");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── Game Options ─────────────────────────────────────────────────────
        let (game_idx, panel) = ui.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.set_color(panel_color);
        panel.base.visible = false;

        let (_, label) = ui.create_label(game_idx)?;
        label.set_text("Game Options");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (back_idx, btn) = ui.create_button(game_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::Main))
        }));
        let (_, label) = ui.create_label(back_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── System Options ───────────────────────────────────────────────────
        let (sys_idx, panel) = ui.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.set_color(panel_color);
        panel.base.visible = false;

        let (_, label) = ui.create_label(sys_idx)?;
        label.set_text("System Options");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        // V-Sync row
        let (row_idx, panel) = ui.create_panel(sys_idx)?;
        panel.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        panel.set_color(row_color);
        let (_, label) = ui.create_label(row_idx)?;
        label.set_text("V-Sync");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        let initial = pending.get();
        let (vsync_checkbox_idx, checkbox) = ui.create_checkbox(row_idx)?;
        checkbox.base.set_position(Anchor::Right, -8.0, 0.0);
        checkbox.base.set_size(32.0, 32.0);
        checkbox.selected = initial.vsync;
        checkbox.set_hover_color(Some(button_hover_color));

        // Slider row
        let (slider_row_idx, panel) = ui.create_panel(sys_idx)?;
        panel.base.bounds = Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 };
        panel.set_color(row_color);
        let (_, label) = ui.create_label(slider_row_idx)?;
        label.set_text("Framerate");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        let (fps_slider_idx, slider) = ui.create_slider(slider_row_idx)?;
        slider.panel.base.set_position(Anchor::Right, -8.0, 0.0);
        slider.set_min_max(60, 999);
        slider.step_size = 8;
        slider.set_value(initial.fps_cap);
        ui.layout_slider(fps_slider_idx)?;

        let (accept_idx, btn) = ui.create_button(sys_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let pending        = Rc::clone(&pending);
            let settings_dirty = Rc::clone(&settings_dirty);
            let nav_request    = Rc::clone(&nav_request);
            move |ui: &mut Ui| {
                let vsync   = ui.get_node::<CheckboxNode>(vsync_checkbox_idx).map(|c| c.selected).unwrap_or(true);
                let fps_cap = ui.get_node::<SliderNode>(fps_slider_idx).map(|s| s.value).unwrap_or(60);
                pending.set(PendingSettings { vsync, fps_cap });
                settings_dirty.set(true);
                nav_request.set(Some(Screen::Main));
            }
        }));
        let (_, label) = ui.create_label(accept_idx)?;
        label.set_text("Accept");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (back_idx, btn) = ui.create_button(sys_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::Main))
        }));
        let (_, label) = ui.create_label(back_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // Re-syncs the V-Sync checkbox and frame rate slider from `pending`
        // whenever this screen is shown, so they always reflect the values
        // staged when it was opened.
        let sys_panel = ui.get_node_mut::<PanelNode>(sys_idx)?;
        sys_panel.base.visibility.on_show = Some(Box::new({
            let pending = Rc::clone(&pending);
            move |ui: &mut Ui| {
                let s = pending.get();
                if let Ok(checkbox) = ui.get_node_mut::<CheckboxNode>(vsync_checkbox_idx) {
                    checkbox.selected = s.vsync;
                }
                if let Ok(slider) = ui.get_node_mut::<SliderNode>(fps_slider_idx) {
                    slider.set_value(s.fps_cap);
                }
                let _ = ui.layout_slider(fps_slider_idx);
            }
        }));

        // ── Keybinds ─────────────────────────────────────────────────────────
        let (keybind_idx, panel) = ui.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.set_color(panel_color);
        panel.base.visible = false;

        let (_, label) = ui.create_label(keybind_idx)?;
        label.set_text("Keybinds");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (back_idx, btn) = ui.create_button(keybind_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::Main))
        }));
        let (_, label) = ui.create_label(back_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── World UI ─────────────────────────────────────────────────────────
        let (world_idx, world) = ui.create_container(0)?;
        world.base.set_size(screen_size.0, screen_size.1);
        world.base.visible = false;

        let total_w = HOTBAR_SLOTS as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
        let (hotbar_idx, hotbar) = ui.create_panel(world_idx)?;
        hotbar.base.set_position(Anchor::Bottom, 0.0, -SLOT_MARGIN_BOTTOM);
        hotbar.base.set_size(total_w, SLOT_SIZE + SLOT_GAP);
        hotbar.set_color(panel_color);

        for i in 0..HOTBAR_SLOTS {
            let x = i as f32 * (SLOT_SIZE + SLOT_GAP) + SLOT_GAP;
            let (_, slot) = ui.create_button(hotbar_idx)?;
            slot.base.set_position(Anchor::TopLeft, x, SLOT_GAP / 2.0);
            slot.base.set_size(SLOT_SIZE, SLOT_SIZE);
            slot.set_color(Rgba::new(0.0, 0.0, 0.0, 0.6));
        }

        // Crosshair
        let xh_color = Rgba::new(1.0, 1.0, 1.0, 0.1);

        let (_, h_bar) = ui.create_panel(world_idx)?;
        h_bar.base.set_position(Anchor::Center, 0.0, 0.0);
        h_bar.base.set_size(XH_SIZE * 2.0, XH_THICKNESS * 2.0);
        h_bar.set_color(xh_color);

        let (_, v_bar) = ui.create_panel(world_idx)?;
        v_bar.base.set_position(Anchor::Center, 0.0, 0.0);
        v_bar.base.set_size(XH_THICKNESS * 2.0, XH_SIZE * 2.0);
        v_bar.set_color(xh_color);

        // ── Debug overlay ────────────────────────────────────────────────────
        let (debug_idx, debug) = ui.create_container(0)?;
        debug.base.set_size(screen_size.0, screen_size.1);
        debug.base.visible = false;

        let debug_color = Rgba::new(1.0, 1.0, 1.0, 1.0);

        let (debug_cam_idx, label) = ui.create_label(debug_idx)?;
        label.set_text("x:-9999.9 y:-9999.9 z:-9999.9");
        label.set_color(debug_color);
        label.base.set_position(Anchor::TopLeft, DEBUG_PADDING, DEBUG_PADDING);

        let (debug_mode_idx, label) = ui.create_label(debug_idx)?;
        label.set_text("Present mode: FIFO_LATEST_READY");
        label.set_color(debug_color);
        label.base.set_position(Anchor::TopLeft, DEBUG_PADDING, 32.0);

        let (debug_quad_idx, label) = ui.create_label(debug_idx)?;
        label.set_text("UI quad count: 0000");
        label.set_color(debug_color);
        label.base.set_position(Anchor::TopLeft, DEBUG_PADDING, 54.0);

        let (debug_fps_idx, label) = ui.create_label(debug_idx)?;
        label.set_text("999 fps");
        label.set_color(debug_color);
        label.base.set_position(Anchor::TopRight, -90.0, DEBUG_PADDING);

        // ── Title screen ─────────────────────────────────────────────────────
        let (title_idx, title) = ui.create_panel(0)?;
        title.base.set_size(screen_size.0, screen_size.1);
        title.set_color(Rgba::new(0.0, 0.0, 0.0, 1.0));

        let (_, label) = ui.create_label(title_idx)?;
        label.set_text("Playground");
        label.base.set_position(Anchor::Top, 0.0, 80.0);

        let (start_idx, start_btn) = ui.create_button(title_idx)?;
        start_btn.base.set_position(Anchor::Center, 0.0, 0.0);
        start_btn.base.set_size(200.0, 48.0);
        start_btn.set_color(Rgba::new(1.0, 1.0, 1.0, 1.0));
        start_btn.set_hover_color(Some(Rgba::new(0.2, 0.5, 1.0, 1.0)));
        start_btn.interaction.on_release = Some(Box::new({
            let nav_request = Rc::clone(&nav_request);
            move |_: &mut Ui| nav_request.set(Some(Screen::World))
        }));
        let (_, label) = ui.create_label(start_idx)?;
        label.set_text("Start");
        label.base.set_position(Anchor::Left, 64.0, 0.0);

        let (quit_idx, quit_btn) = ui.create_button(title_idx)?;
        quit_btn.base.set_position(Anchor::Center, 0.0, 64.0);
        quit_btn.base.set_size(200.0, 48.0);
        quit_btn.set_color(Rgba::new(1.0, 1.0, 1.0, 1.0));
        quit_btn.set_hover_color(Some(Rgba::new(0.2, 0.5, 1.0, 1.0)));
        quit_btn.interaction.on_release = Some(Box::new(|ui: &mut Ui| ui.request_exit()));
        let (_, label) = ui.create_label(quit_idx)?;
        label.set_text("Quit");
        label.base.set_position(Anchor::Left, 64.0, 0.0);

        Ok(Self {
            main_idx,
            game_idx,
            sys_idx,
            keybind_idx,
            world_idx,
            title_idx,
            debug_idx,
            debug_cam_idx,
            debug_mode_idx,
            debug_quad_idx,
            debug_fps_idx,
            current: Cell::new(Screen::Title),
            mouse_center: (screen_size.0 / 2.0, screen_size.1 / 2.0),
            nav_request,
            pending,
            settings_dirty,
        })
    }
}
