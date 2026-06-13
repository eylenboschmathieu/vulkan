#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{cell::Cell, rc::Rc};

use anyhow::Result;
use log::error;

use ui::{Anchor, CheckboxNode, ContainerNode, CursorRequest, PanelNode, Rect, Rgba, SliderNode, Ui};

const HOTBAR_SLOTS:       usize = 10;
const SLOT_SIZE:          f32   = 48.0;
const SLOT_GAP:           f32   = 4.0;
const SLOT_MARGIN_BOTTOM: f32   = 20.0;

const XH_SIZE:      f32 = 16.0;
const XH_THICKNESS: f32 = 2.0;

const DEBUG_PADDING: f32 = 10.0;

/// Identifies a top-level screen. Used as the `S` type parameter of `ui`'s
/// navigator: [`Screens::build`] registers each variant's container node via
/// [`Ui::register_screen`], buttons map to a target variant via
/// [`Ui::set_navigation`], and [`Ui::navigate_to`]/[`Ui::navigate_to_screen`]
/// show/hide the corresponding containers and update `ui`'s current screen.
/// `ui` itself never sees these names — it stores `Screen` values only as
/// `Copy + Eq + Hash` keys.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Screen {
    World,
    Title,
    Main,
    GameOptions,
    SystemOptions,
    Keybinds,
}

/// Identifies a rendering/z-order band. Used as the `L` type parameter of
/// `ui`'s layer order: [`Screens::build`] registers each root-level screen
/// container via [`Ui::register_layer`], in registration order — `Content`
/// (registered first) becomes band 0, `Debug` (registered last) becomes band
/// 1, so the debug overlay always renders and hit-tests on top of every other
/// screen regardless of `z_index`. Orthogonal to [`Screen`]: `Screen` is
/// which screen is currently visible, `Layer` is purely about stacking order.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Layer {
    Content,
    Debug,
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

/// Navigates to the screen registered for `trigger_idx` (see
/// [`Ui::set_navigation`]), logging instead of propagating on failure since
/// it's called from node callbacks that can't report errors to `App`.
fn navigate(ui: &mut Ui, trigger_idx: usize) {
    if let Err(e) = ui.navigate_to::<Screen>(trigger_idx) {
        error!("Screen navigation error: {e}");
    }
}

/// Owns the debug overlay built on top of [`ui::Ui`], alongside the
/// menu/title/world screens. Screen navigation itself is handled by `ui`'s
/// navigator (see [`Ui::navigate_to`]/[`Ui::navigate_to_screen`]) keyed by
/// [`Screen`]; `Screens` only tracks what it still needs after `build`: the
/// debug overlay's node indices and the System Options live-edit buffer.
pub struct Screens {
    debug_idx: usize,
    debug_cam_idx: usize,
    debug_mode_idx: usize,
    debug_quad_idx: usize,
    debug_fps_idx: usize,

    /// System Options' live-edit buffer for vsync/fps settings.
    pub pending: Rc<Cell<PendingSettings>>,
    /// Set by the Accept button alongside writing `pending`.
    pub settings_dirty: Rc<Cell<bool>>,
}

impl Screens {
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
    /// screens as children of the UI's root container, registering each with
    /// `ui`'s navigator under its [`Screen`] variant. Navigation buttons
    /// register themselves via [`Ui::set_navigation`] and call
    /// [`Ui::navigate_to`] on their own index in `on_release`; quit buttons
    /// call [`Ui::request_exit`], and the System Options screen reads/writes
    /// `pending`/`settings_dirty`.
    pub fn build(ui: &mut Ui, screen_size: (f32, f32)) -> Result<Self> {
        ui.init_navigation(Screen::Title);

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
        ui.register_screen(Screen::Main, main_idx)?;
        ui.register_layer(main_idx, Layer::Content)?;

        let (_, label) = ui.create_label(main_idx)?;
        label.set_text("Main Menu");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (resume_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, resume_idx)));
        ui.set_navigation(resume_idx, Screen::World)?;
        let (_, label) = ui.create_label(resume_idx)?;
        label.set_text("Resume");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (game_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 296.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, game_btn_idx)));
        ui.set_navigation(game_btn_idx, Screen::GameOptions)?;
        let (_, label) = ui.create_label(game_btn_idx)?;
        label.set_text("Game Options");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (sys_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 392.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, sys_btn_idx)));
        ui.set_navigation(sys_btn_idx, Screen::SystemOptions)?;
        let (_, label) = ui.create_label(sys_btn_idx)?;
        label.set_text("System Options");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (keybind_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, keybind_btn_idx)));
        ui.set_navigation(keybind_btn_idx, Screen::Keybinds)?;
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

        // ── Z-order test windows ─────────────────────────────────────────────
        // Two overlapping panels on the right side, registered as orderable so
        // clicking either raises it above the other.
        let (win_a_idx, panel) = ui.create_panel(main_idx)?;
        panel.base.bounds = Rect { x: panel_w + 40.0, y: 80.0, width: 240.0, height: 160.0 };
        panel.set_color(Rgba::new(0.8, 0.2, 0.2, 0.8));
        let (_, label) = ui.create_label(win_a_idx)?;
        label.set_text("Window A");
        label.set_color(Rgba::new(1.0, 1.0, 1.0, 1.0));
        label.base.set_position(Anchor::TopLeft, 10.0, 10.0);
        let (a_btn_idx, btn) = ui.create_button(win_a_idx)?;
        btn.base.set_position(Anchor::TopLeft, 10.0, 50.0);
        btn.base.set_size(100.0, 32.0);
        btn.set_color(Rgba::new(1.0, 1.0, 1.0, 0.4));
        btn.set_hover_color(Some(Rgba::new(1.0, 1.0, 1.0, 0.7)));
        let (_, label) = ui.create_label(a_btn_idx)?;
        label.set_text("Button A");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        let (win_b_idx, panel) = ui.create_panel(main_idx)?;
        panel.base.bounds = Rect { x: panel_w + 140.0, y: 160.0, width: 240.0, height: 160.0 };
        panel.set_color(Rgba::new(0.2, 0.2, 0.8, 0.8));
        let (_, label) = ui.create_label(win_b_idx)?;
        label.set_text("Window B");
        label.set_color(Rgba::new(1.0, 1.0, 1.0, 1.0));
        label.base.set_position(Anchor::TopLeft, 10.0, 10.0);
        let (b_btn_idx, btn) = ui.create_button(win_b_idx)?;
        btn.base.set_position(Anchor::TopLeft, 10.0, 50.0);
        btn.base.set_size(100.0, 32.0);
        btn.set_color(Rgba::new(1.0, 1.0, 1.0, 0.4));
        btn.set_hover_color(Some(Rgba::new(1.0, 1.0, 1.0, 0.7)));
        let (_, label) = ui.create_label(b_btn_idx)?;
        label.set_text("Button B");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        // Registration order sets the initial z-order: B starts on top of A.
        ui.register_orderable(win_a_idx)?;
        ui.register_orderable(win_b_idx)?;

        // ── Game Options ─────────────────────────────────────────────────────
        let (game_idx, panel) = ui.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.set_color(panel_color);
        panel.base.visible = false;
        ui.register_screen(Screen::GameOptions, game_idx)?;
        ui.register_layer(game_idx, Layer::Content)?;

        let (_, label) = ui.create_label(game_idx)?;
        label.set_text("Game Options");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (back_idx, btn) = ui.create_button(game_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, back_idx)));
        ui.set_navigation(back_idx, Screen::Main)?;
        let (_, label) = ui.create_label(back_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── System Options ───────────────────────────────────────────────────
        let (sys_idx, panel) = ui.create_panel(0)?;
        panel.base.bounds  = menu_rect;
        panel.set_color(panel_color);
        panel.base.visible = false;
        ui.register_screen(Screen::SystemOptions, sys_idx)?;
        ui.register_layer(sys_idx, Layer::Content)?;

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
            move |ui: &mut Ui| {
                let vsync   = ui.get_node::<CheckboxNode>(vsync_checkbox_idx).map(|c| c.selected).unwrap_or(true);
                let fps_cap = ui.get_node::<SliderNode>(fps_slider_idx).map(|s| s.value).unwrap_or(60);
                pending.set(PendingSettings { vsync, fps_cap });
                settings_dirty.set(true);
                navigate(ui, accept_idx);
            }
        }));
        ui.set_navigation(accept_idx, Screen::Main)?;
        let (_, label) = ui.create_label(accept_idx)?;
        label.set_text("Accept");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (back_idx, btn) = ui.create_button(sys_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 488.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, back_idx)));
        ui.set_navigation(back_idx, Screen::Main)?;
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
        ui.register_screen(Screen::Keybinds, keybind_idx)?;
        ui.register_layer(keybind_idx, Layer::Content)?;

        let (_, label) = ui.create_label(keybind_idx)?;
        label.set_text("Keybinds");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (back_idx, btn) = ui.create_button(keybind_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, back_idx)));
        ui.set_navigation(back_idx, Screen::Main)?;
        let (_, label) = ui.create_label(back_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── World UI ─────────────────────────────────────────────────────────
        let (world_idx, world) = ui.create_container(0)?;
        world.base.set_size(screen_size.0, screen_size.1);
        world.base.visible = false;
        ui.register_screen(Screen::World, world_idx)?;
        ui.register_layer(world_idx, Layer::Content)?;

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

        // World is the only screen with first-person mouse-look, so lock the
        // cursor whenever it becomes active and free it (restoring to
        // screen-center, captured once here) whenever it's left.
        let mouse_center = (screen_size.0 / 2.0, screen_size.1 / 2.0);
        let world_node = ui.get_node_mut::<ContainerNode>(world_idx)?;
        world_node.base.visibility.on_show = Some(Box::new(|ui: &mut Ui| ui.request_cursor(CursorRequest::Lock)));
        world_node.base.visibility.on_hide = Some(Box::new(move |ui: &mut Ui| {
            ui.request_cursor(CursorRequest::Free { x: mouse_center.0, y: mouse_center.1 });
        }));

        // ── Debug overlay ────────────────────────────────────────────────────
        let (debug_idx, debug) = ui.create_container(0)?;
        debug.base.set_size(screen_size.0, screen_size.1);
        debug.base.visible = false;
        ui.register_layer(debug_idx, Layer::Debug)?;

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
        ui.register_screen(Screen::Title, title_idx)?;
        ui.register_layer(title_idx, Layer::Content)?;

        let (_, label) = ui.create_label(title_idx)?;
        label.set_text("Playground");
        label.base.set_position(Anchor::Top, 0.0, 80.0);

        let (start_idx, start_btn) = ui.create_button(title_idx)?;
        start_btn.base.set_position(Anchor::Center, 0.0, 0.0);
        start_btn.base.set_size(200.0, 48.0);
        start_btn.set_color(Rgba::new(1.0, 1.0, 1.0, 1.0));
        start_btn.set_hover_color(Some(Rgba::new(0.2, 0.5, 1.0, 1.0)));
        start_btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, start_idx)));
        ui.set_navigation(start_idx, Screen::World)?;
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
            debug_idx,
            debug_cam_idx,
            debug_mode_idx,
            debug_quad_idx,
            debug_fps_idx,
            pending,
            settings_dirty,
        })
    }
}
