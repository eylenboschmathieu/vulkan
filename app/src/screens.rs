#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{cell::Cell, rc::Rc};

use anyhow::Result;
use log::error;

use ui::{Anchor, Axis, ButtonNode, CheckboxNode, CursorRequest, GroupNode, LabelNode, PanelNode, ProgressBarNode, Rect, Rgba, SliderNode, Ui, TITLEBAR_HEIGHT, WINDOW_BORDER};

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
    UiTest,
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
    debug_mem_idx: usize,
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
    pub fn update_debug(&self, ui: &mut Ui, camera_text: impl Into<String>, present_mode_text: impl Into<String>, mem_text: impl Into<String>, ui_quad_count: usize, fps: f32) -> Result<()> {
        ui.set_label_text(self.debug_cam_idx, camera_text)?;
        ui.set_label_text(self.debug_mode_idx, present_mode_text)?;
        ui.set_label_text(self.debug_mem_idx, mem_text)?;
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
        let scrollbar_color    = Rgba::new(0.35, 0.45, 0.55, 0.6);
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

        let (ui_test_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 584.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, ui_test_btn_idx)));
        ui.set_navigation(ui_test_btn_idx, Screen::UiTest)?;
        let (_, label) = ui.create_label(ui_test_btn_idx)?;
        label.set_text("UI Tests");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        let (quit_btn_idx, btn) = ui.create_button(main_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 680.0, width: 400.0, height: 48.0 };
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

        let (fps_slider_idx, slider) = ui.create_slider(slider_row_idx, Axis::Horizontal)?;
        slider.panel.base.set_position(Anchor::Right, -8.0, 0.0);
        slider.set_min_max(60, 999);
        slider.step_size = 8;
        slider.set_value(initial.fps_cap);

        let fps_label_text  = format!("{:>3}", initial.fps_cap);
        let fps_label_width = ui.label_width(&fps_label_text);
        let (fps_label_idx, label) = ui.create_label(slider_row_idx)?;
        label.set_text(fps_label_text);
        label.base.set_width(fps_label_width);
        label.base.set_position_anchored_to(Anchor::Right, fps_slider_idx, Anchor::Left, -8.0, 0.0);

        ui.get_node_mut::<SliderNode>(fps_slider_idx)?.on_value_changed = Some(Box::new(move |ui: &mut Ui| {
            let value = ui.get_node::<SliderNode>(fps_slider_idx).map(|s| s.value).unwrap_or(60);
            let _ = ui.set_label_text(fps_label_idx, format!("{value:>3}"));
        }));

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
                let fps_value = ui.get_node::<SliderNode>(fps_slider_idx).map(|s| s.value).unwrap_or(s.fps_cap);
                let _ = ui.set_label_text(fps_label_idx, format!("{fps_value:>3}"));
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

        // Scrollable list of keybind rows (placeholder labels for now, until
        // actual action-to-button bindings land here), with a vertical
        // scrollbar wired to the same scroll offset. Each row sits in a
        // (row_height + row_offset) "slot", with row_offset / 2 of leading
        // margin before the first row (and trailing margin after the last) -
        // so scrolling by one slot per step always re-aligns the next row's
        // top with that same leading margin from the panel's top.
        let row_height      = ui.font_atlas.cap_height;
        let row_offset      = 16.0;
        let row_pitch       = row_height + row_offset;
        let row_count       = 24;
        let visible_rows    = 14;
        let viewport        = (400.0, visible_rows as f32 * row_pitch);
        let content_h       = row_count as f32 * row_pitch;
        let scrollbar_width = 24.0;

        let (_, frame) = ui.create_scroll_panel(keybind_idx, Axis::Vertical, viewport, scrollbar_width, (viewport.0, content_h))?;
        frame.base.set_position(Anchor::TopLeft, 64.0, 264.0);
        let keybind_list_idx = frame.content_idx;
        let scrollbar_idx    = frame.scrollbar_idx;

        ui.get_node_mut::<PanelNode>(keybind_list_idx)?.set_color(row_color);
        let scrollbar = ui.get_node_mut::<SliderNode>(scrollbar_idx)?;
        scrollbar.set_color(scrollbar_color);
        scrollbar.step_size = row_pitch.round() as u32;

        for i in 0..row_count {
            let (_, label) = ui.create_label(keybind_list_idx)?;
            label.set_text(format!("Action {}", i + 1));
            label.base.set_position(Anchor::TopLeft, 8.0, row_offset / 2.0 + i as f32 * row_pitch);
        }

        // ── UI Tests ─────────────────────────────────────────────────────────
        let (ui_test_idx, panel) = ui.create_panel(0)?;
        panel.base.set_size(screen_size.0, screen_size.1);
        panel.set_color(panel_color);
        panel.base.visible = false;
        ui.register_screen(Screen::UiTest, ui_test_idx)?;
        ui.register_layer(ui_test_idx, Layer::Content)?;

        let (_, label) = ui.create_label(ui_test_idx)?;
        label.set_text("UI Tests");
        label.base.set_position(Anchor::TopLeft, 100.0, 100.0);

        let (back_idx, btn) = ui.create_button(ui_test_idx)?;
        btn.base.bounds = Rect { x: 64.0, y: 200.0, width: 400.0, height: 48.0 };
        btn.set_color(button_color);
        btn.set_hover_color(Some(button_hover_color));
        btn.interaction.on_release = Some(Box::new(move |ui: &mut Ui| navigate(ui, back_idx)));
        ui.set_navigation(back_idx, Screen::Main)?;
        let (_, label) = ui.create_label(back_idx)?;
        label.set_text("Back");
        label.base.set_position(Anchor::Left, 10.0, 0.0);

        // ── Z-order / clip / clamp window tests ──────────────────────────────
        // Two overlapping windows, registered as orderable so clicking either
        // raises it above the other.
        let (win_a_idx, window) = ui.create_window(ui_test_idx, 240.0, 160.0, ui::WindowBody::Panel)?;
        window.base.set_position(Anchor::TopLeft, panel_w + 40.0, 80.0);
        window.set_draggable(true);
        let title_a_idx = window.title;
        let body_a_idx  = window.body;
        ui.get_node_mut::<LabelNode>(title_a_idx)?.set_text("Window A");
        ui.get_node_mut::<PanelNode>(body_a_idx)?.set_color(Rgba::new(0.8, 0.2, 0.2, 1.0));
        let (a_btn_idx, btn) = ui.create_button(body_a_idx)?;
        btn.base.set_position(Anchor::TopLeft, 10.0, 10.0);
        btn.base.set_size(100.0, 32.0);
        btn.set_color(Rgba::new(1.0, 1.0, 1.0, 0.4));
        btn.set_hover_color(Some(Rgba::new(1.0, 1.0, 1.0, 0.7)));
        let (_, label) = ui.create_label(a_btn_idx)?;
        label.set_text("Button A");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        // Clip test: nested window overflows Window A's body — the overflowing
        // portion should be clipped, including while dragging.
        let (_, nested) = ui.create_window(body_a_idx, 150.0, 100.0, ui::WindowBody::Panel)?;
        nested.base.set_position(Anchor::TopLeft, 150.0, 80.0);
        nested.set_draggable(true);
        let nested_title_idx = nested.title;
        ui.get_node_mut::<LabelNode>(nested_title_idx)?.set_text("Nested");

        let (win_b_idx, window) = ui.create_window(ui_test_idx, 240.0, 160.0, ui::WindowBody::Panel)?;
        window.base.set_position(Anchor::TopLeft, panel_w + 140.0, 160.0);
        window.set_draggable(true);
        let title_b_idx = window.title;
        let body_b_idx  = window.body;
        ui.get_node_mut::<LabelNode>(title_b_idx)?.set_text("Window B");
        ui.get_node_mut::<PanelNode>(body_b_idx)?.set_color(Rgba::new(0.2, 0.2, 0.8, 1.0));
        let (b_btn_idx, btn) = ui.create_button(body_b_idx)?;
        btn.base.set_position(Anchor::TopLeft, 10.0, 10.0);
        btn.base.set_size(100.0, 32.0);
        btn.set_color(Rgba::new(1.0, 1.0, 1.0, 0.4));
        btn.set_hover_color(Some(Rgba::new(1.0, 1.0, 1.0, 0.7)));
        let (_, label) = ui.create_label(b_btn_idx)?;
        label.set_text("Button B");
        label.base.set_position(Anchor::Left, 8.0, 0.0);

        // Clamp test: nested window inside Window B's body with clamp_children,
        // so dragging keeps it fully within the body bounds.
        let (_clamped_idx, clamped) = ui.create_window(body_b_idx, 110.0, 90.0, ui::WindowBody::Panel)?;
        clamped.base.set_position(Anchor::TopLeft, 20.0, 30.0);
        clamped.set_draggable(true);
        let clamped_title_idx = clamped.title;
        ui.get_node_mut::<LabelNode>(clamped_title_idx)?.set_text("Clamped");
        ui.set_clamp_children(body_b_idx, true)?;

        // Registration order sets initial z-order: B starts on top of A.
        ui.register_orderable(win_a_idx)?;
        ui.register_orderable(win_b_idx)?;

        // ── Tab panel test ────────────────────────────────────────────────────
        // Six tabs whose total width exceeds 360 px, so the overflow scrollbar
        // appears on hover.
        let tab_w         = 360.0;
        let tab_h         = 32.0;
        let sb_h          = 2.5;
        let tab_body_h    = 180.0;
        let tab_label_pad = 12.0;

        let tab_list_bg = Rgba::new(0.20, 0.20, 0.23, 1.0);
        let tab_btn     = Rgba::new(0.28, 0.28, 0.32, 1.0);
        let tab_hover   = Rgba::new(0.35, 0.55, 0.85, 1.0);
        let tab_body    = Rgba::new(0.08, 0.08, 0.10, 1.0);

        let tab_win_w = tab_w + 2.0 * WINDOW_BORDER;
        let tab_win_h = tab_h + tab_body_h + TITLEBAR_HEIGHT + 3.0 * WINDOW_BORDER;
        let (_, win) = ui.create_window(ui_test_idx, tab_win_w, tab_win_h, ui::WindowBody::TabPanel {
            tab_height:       tab_h,
            scrollbar_height: sb_h,
            tab_body:         ui::TabBody::Panel,
        })?;
        win.base.set_position(Anchor::TopLeft, panel_w + 40.0, 360.0);
        win.set_draggable(true);
        let win_title = win.title;
        let tp_idx    = win.body;
        ui.get_node_mut::<LabelNode>(win_title)?.set_text("Settings");

        let tab_body_idx = ui.get_node::<ui::TabPanelNode>(tp_idx)?.body_idx;
        ui.get_node_mut::<ui::PanelNode>(tab_body_idx)?.set_color(tab_body);
        {
            let tl_idx = ui.get_node::<ui::TabPanelNode>(tp_idx)?.tab_list_idx;
            ui.get_node_mut::<ui::TabListNode>(tl_idx)?.set_color(tab_list_bg);
        }
        {
            let tp = ui.get_node_mut::<ui::TabPanelNode>(tp_idx)?;
            tp.selected_tab_color = Some(tab_body);
            tp.default_tab_color  = Some(tab_btn);
            tp.tab_hover_color    = Some(tab_hover);
        }

        let tab_labels = ["General", "Display", "Audio", "Controls", "Network", "Advanced"];
        for (i, label_text) in tab_labels.iter().enumerate() {
            let btn_w = ui.label_width(label_text) + tab_label_pad;
            let (btn_idx, content_idx) = ui.add_tab(tp_idx, btn_w)?;
            {
                let btn = ui.get_node_mut::<ui::ButtonNode>(btn_idx)?;
                btn.set_color(tab_btn);
                btn.set_hover_color(Some(tab_hover));
            }
            {
                let (_, lbl) = ui.create_label(btn_idx)?;
                lbl.set_text(*label_text);
                lbl.base.set_position(Anchor::Left, 6.0, 0.0);
            }
            {
                let (_, content_lbl) = ui.create_label(content_idx)?;
                content_lbl.set_text(format!("{label_text} settings"));
                content_lbl.base.set_position(Anchor::TopLeft, 12.0, 12.0);
            }
            if i == 0 {
                let (test_btn, _) = ui.create_button(content_idx)?;
                {
                    let btn = ui.get_node_mut::<ui::ButtonNode>(test_btn)?;
                    btn.base.set_position(Anchor::TopLeft, 12.0, 40.0);
                    btn.base.set_size(120.0, 28.0);
                    btn.set_color(tab_btn);
                    btn.set_hover_color(Some(tab_hover));
                }
                let (_, bl) = ui.create_label(test_btn)?;
                bl.set_text("Apply");
                bl.base.set_position(Anchor::Left, 8.0, 0.0);
            }
        }
        ui.select_tab(tp_idx, 0)?;

        // ── Progress bar test ─────────────────────────────────────────────────
        // A draggable window containing a horizontal/vertical progress bar
        // controlled by a slider; a checkbox toggles the axis.
        let pb_win_w = 260.0;
        let pb_win_h = 246.0;
        let (_, pb_win) = ui.create_window(ui_test_idx, pb_win_w, pb_win_h, ui::WindowBody::Panel)?;
        pb_win.base.set_position(Anchor::TopLeft, panel_w + 300.0, 80.0);
        pb_win.set_draggable(true);
        let pb_win_title = pb_win.title;
        let pb_body_idx  = pb_win.body;
        ui.get_node_mut::<LabelNode>(pb_win_title)?.set_text("Progress Bar");
        ui.get_node_mut::<PanelNode>(pb_body_idx)?.set_color(Rgba::new(0.1, 0.1, 0.12, 1.0));

        let pb_track_color = Rgba::new(0.18, 0.18, 0.20, 1.0);
        let pb_fill_color  = Rgba::new(0.20, 0.68, 0.32, 1.0);

        let (pb_idx, pb) = ui.create_progress_bar(pb_body_idx, Axis::Horizontal, 236.0, 14.0)?;
        pb.base.set_position(Anchor::TopLeft, 8.0, 54.0);
        pb.set_track_color(pb_track_color);
        let pb_fill_idx = pb.fill_idx;
        ui.get_node_mut::<PanelNode>(pb_fill_idx)?.set_color(pb_fill_color);
        ui.set_progress(pb_idx, 0.5)?;

        // "Vertical" label + checkbox.
        let vert_label_w = ui.label_width("Vertical:");
        let (_, vt_lbl) = ui.create_label(pb_body_idx)?;
        vt_lbl.set_text("Vertical:");
        vt_lbl.base.set_position(Anchor::TopLeft, 8.0, 12.0);

        let (pb_checkbox_idx, checkbox) = ui.create_checkbox(pb_body_idx)?;
        checkbox.base.set_position(Anchor::TopLeft, 8.0 + vert_label_w + 8.0, 6.0);
        checkbox.base.set_size(20.0, 20.0);
        checkbox.set_hover_color(Some(button_hover_color));

        // "Reversed" label + checkbox.
        let rev_label_w = ui.label_width("Reversed:");
        let (_, rev_lbl) = ui.create_label(pb_body_idx)?;
        rev_lbl.set_text("Reversed:");
        rev_lbl.base.set_position(Anchor::TopLeft, 8.0, 34.0);

        let (pb_rev_checkbox_idx, checkbox) = ui.create_checkbox(pb_body_idx)?;
        checkbox.base.set_position(Anchor::TopLeft, 8.0 + rev_label_w + 8.0, 28.0);
        checkbox.base.set_size(20.0, 20.0);
        checkbox.set_hover_color(Some(button_hover_color));

        // Percentage label + slider.
        let pct_w = ui.label_width("100%");
        let (pb_pct_idx, pct_lbl) = ui.create_label(pb_body_idx)?;
        pct_lbl.set_text("50%");
        pct_lbl.base.set_width(pct_w);
        pct_lbl.base.set_position(Anchor::TopLeft, 200.0, 188.0);

        let (pb_slider_idx, slider) = ui.create_slider(pb_body_idx, Axis::Horizontal)?;
        slider.panel.base.set_position(Anchor::TopLeft, 8.0, 186.0);
        slider.panel.base.set_size(186.0, 16.0);
        slider.set_min_max(0, 100);
        slider.step_size = 1;
        slider.set_value(50);

        ui.get_node_mut::<SliderNode>(pb_slider_idx)?.on_value_changed = Some(Box::new(move |ui: &mut Ui| {
            let v = ui.get_node::<SliderNode>(pb_slider_idx).map(|s| s.value).unwrap_or(50);
            let frac = v as f32 / 100.0;
            let _ = ui.set_progress(pb_idx, frac);
            let _ = ui.set_label_text(pb_pct_idx, format!("{v}%"));
        }));
        ui.layout_slider(pb_slider_idx)?;

        ui.get_node_mut::<CheckboxNode>(pb_rev_checkbox_idx)?.interaction.on_release = Some(Box::new(move |ui: &mut Ui| {
            let reversed = ui.get_node::<CheckboxNode>(pb_rev_checkbox_idx).map(|c| c.selected).unwrap_or(false);
            let _ = ui.set_fill_reversed(pb_idx, reversed);
        }));

        ui.get_node_mut::<CheckboxNode>(pb_checkbox_idx)?.interaction.on_release = Some(Box::new(move |ui: &mut Ui| {
            let vertical = ui.get_node::<CheckboxNode>(pb_checkbox_idx).map(|c| c.selected).unwrap_or(false);
            let axis = if vertical { Axis::Vertical } else { Axis::Horizontal };
            // Axis change resets fill direction; uncheck the Reversed box to match.
            let _ = ui.set_checkbox_selected(pb_rev_checkbox_idx, false);
            let thumb_idx = ui.get_node::<SliderNode>(pb_slider_idx).ok().and_then(|s| s.get_thumb());
            if vertical {
                let _ = ui.get_node_mut::<ProgressBarNode>(pb_idx).map(|pb| pb.base.set_size(14.0, 120.0));
                let _ = ui.set_axis::<ProgressBarNode>(pb_idx, axis);
                let _ = ui.get_node_mut::<SliderNode>(pb_slider_idx).map(|s| {
                    s.panel.base.set_position(Anchor::TopLeft, 30.0, 54.0);
                    s.panel.base.set_size(16.0, 120.0);
                    s.reversed = true;
                });
                if let Some(idx) = thumb_idx {
                    let _ = ui.get_node_mut::<ButtonNode>(idx).map(|b| b.base.set_size(32.0, 16.0));
                }
                let _ = ui.set_axis::<SliderNode>(pb_slider_idx, axis);
            } else {
                let _ = ui.get_node_mut::<ProgressBarNode>(pb_idx).map(|pb| pb.base.set_size(236.0, 14.0));
                let _ = ui.set_axis::<ProgressBarNode>(pb_idx, axis);
                let _ = ui.get_node_mut::<SliderNode>(pb_slider_idx).map(|s| {
                    s.panel.base.set_position(Anchor::TopLeft, 8.0, 186.0);
                    s.panel.base.set_size(186.0, 16.0);
                    s.reversed = false;
                });
                if let Some(idx) = thumb_idx {
                    let _ = ui.get_node_mut::<ButtonNode>(idx).map(|b| b.base.set_size(16.0, 32.0));
                }
                let _ = ui.set_axis::<SliderNode>(pb_slider_idx, axis);
            }
        }));

        // ── World UI ─────────────────────────────────────────────────────────
        let (world_idx, world) = ui.create_group(0)?;
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
        let world_node = ui.get_node_mut::<GroupNode>(world_idx)?;
        world_node.base.visibility.on_show = Some(Box::new(|ui: &mut Ui| ui.request_cursor(CursorRequest::Lock)));
        world_node.base.visibility.on_hide = Some(Box::new(move |ui: &mut Ui| {
            ui.request_cursor(CursorRequest::Free { x: mouse_center.0, y: mouse_center.1 });
        }));

        // ── Debug overlay ────────────────────────────────────────────────────
        let (debug_idx, debug) = ui.create_group(0)?;
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

        let (debug_mem_idx, label) = ui.create_label(debug_idx)?;
        label.set_text("Mem: 0000.0 MiB");
        label.set_color(debug_color);
        label.base.set_position(Anchor::TopLeft, DEBUG_PADDING, 54.0);

        let (debug_quad_idx, label) = ui.create_label(debug_idx)?;
        label.set_text("UI quad count: 0000");
        label.set_color(debug_color);
        label.base.set_position(Anchor::TopLeft, DEBUG_PADDING, 76.0);

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
            debug_mem_idx,
            debug_quad_idx,
            debug_fps_idx,
            pending,
            settings_dirty,
        })
    }
}
