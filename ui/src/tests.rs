use super::input::MouseButton;
use super::*;
use std::cell::Cell;
use std::collections::HashMap;

/// A minimal atlas with no glyphs — enough to construct a `Ui` and
/// exercise layout/flush without needing real font data.
fn test_atlas() -> Rc<FontAtlas> {
    Rc::new(FontAtlas {
        texture_id: TextureId(42),
        glyphs: HashMap::new(),
        white_uv: [0.0, 0.0],
        line_height: 16.0,
        cap_height: 10.0,
    })
}

#[test]
fn flush_all_emits_quad_for_nested_panel() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (container_idx, container) = ui.create_container(0).unwrap();
    container.base.set_position(Anchor::TopLeft, 50.0, 60.0);
    container.base.set_size(200.0, 150.0);

    let (_, panel) = ui.create_panel(container_idx).unwrap();
    panel.base.set_position(Anchor::TopLeft, 5.0, 5.0);
    panel.base.set_size(20.0, 10.0);
    panel.set_color(Rgba::new(0.2, 0.4, 0.6, 1.0));

    let update = ui.flush_all();

    let (texture_id, verts) = match update {
        UiUpdate::Full(texture_id, verts) => (texture_id, verts),
        _ => panic!("expected UiUpdate::Full"),
    };

    assert_eq!(texture_id, TextureId(42));
    assert_eq!(ui.quad_count(), 1);
    assert_eq!(verts.len(), 4);

    // Panel is offset (5, 5) within the container, which itself is
    // offset (50, 60) from the screen origin -> absolute (55, 65).
    assert_eq!(verts[0].pos, Pos2::new(55.0, 65.0)); // top-left
    assert_eq!(verts[1].pos, Pos2::new(75.0, 65.0)); // top-right
    assert_eq!(verts[2].pos, Pos2::new(75.0, 75.0)); // bottom-right
    assert_eq!(verts[3].pos, Pos2::new(55.0, 75.0)); // bottom-left

    for v in &verts {
        assert_eq!(v.color, Rgba::new(0.2, 0.4, 0.6, 1.0));
        assert_eq!(v.uv, UV::new(0.0, 0.0));
    }

    // Nothing dirty right after a full flush.
    assert!(matches!(ui.flush_dirty(), UiUpdate::None));
}

#[test]
fn resize_updates_root_bounds_and_marks_dirty() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    // A panel pinned to the bottom-right corner, so its position tracks the
    // root container's size.
    let (_, panel) = ui.create_panel(0).unwrap();
    panel.base.set_position(Anchor::BottomRight, 0.0, 0.0);
    panel.base.set_size(50.0, 50.0);

    let UiUpdate::Full(_, verts) = ui.flush_all() else { panic!("expected UiUpdate::Full") };
    assert_eq!(verts[2].pos, Pos2::new(800.0, 600.0)); // bottom-right corner == screen size

    ui.resize((1024.0, 768.0));
    assert!(ui.dirty);

    let UiUpdate::Full(_, verts) = ui.flush_all() else { panic!("expected UiUpdate::Full") };
    assert_eq!(verts[2].pos, Pos2::new(1024.0, 768.0)); // re-anchored to the new screen size
}

#[test]
fn flush_dirty_returns_partial_patch_after_checkbox_click() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (checkbox_idx, checkbox) = ui.create_checkbox(0).unwrap();
    checkbox.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    checkbox.base.set_size(32.0, 32.0);

    // Establish vertex offsets via a full flush.
    let UiUpdate::Full(_, verts) = ui.flush_all() else { panic!("expected UiUpdate::Full") };
    assert_eq!(verts.len(), 4);
    let offset = ui.get_node::<CheckboxNode>(checkbox_idx).unwrap().base.vertex_offset;

    // Frame 1: cursor moves over the checkbox -> marks it dirty (hover).
    let hover = UiInput::new((16.0, 16.0));
    ui.handle_input(&hover).unwrap();

    let UiUpdate::Partial(patches) = ui.flush_dirty() else { panic!("expected UiUpdate::Partial") };
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].0, offset);
    assert_eq!(patches[0].1.len(), 4);

    // Drained by the flush above; nothing left to patch.
    assert!(matches!(ui.flush_dirty(), UiUpdate::None));

    // Frame 2: releasing the primary button toggles `selected`.
    let click = UiInput::new((16.0, 16.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&click).unwrap();
    assert!(ui.get_node::<CheckboxNode>(checkbox_idx).unwrap().selected);

    let UiUpdate::Partial(patches) = ui.flush_dirty() else { panic!("expected UiUpdate::Partial") };
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].0, offset);

    // Selecting switches the panel's color to `selected_color` (the
    // CheckboxNode::new() default).
    let expected_color = Rgba::new(0.2, 0.7, 0.3, 0.7);
    assert_eq!(patches[0].1.len(), 4);
    for v in &patches[0].1 {
        assert_eq!(v.color, expected_color);
    }

    // No more dirty nodes; flush_dirty is now a no-op.
    assert!(matches!(ui.flush_dirty(), UiUpdate::None));
}

/// Mirrors the host's "system menu" pattern: a checkbox's `on_show` pulls in
/// host-owned state (e.g. `blitz.vsync()`) when the screen becomes visible,
/// and its `on_release` writes the toggled state back out.
#[test]
fn checkbox_callbacks_mirror_host_state() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    // Host-owned state the checkbox mirrors.
    let vsync = Rc::new(Cell::new(true));

    let (checkbox_idx, checkbox) = ui.create_checkbox(0).unwrap();
    checkbox.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    checkbox.base.set_size(32.0, 32.0);

    // Initialization: on_show pulls the current host state into the checkbox.
    let vsync_show = vsync.clone();
    checkbox.base.visibility.on_show = Some(Box::new(move |ui| {
        let c = ui.get_node_mut::<CheckboxNode>(checkbox_idx).unwrap();
        c.selected = vsync_show.get();
    }));

    // Interaction: on_release writes the (toggled) state back to the host.
    let vsync_release = vsync.clone();
    checkbox.interaction.on_release = Some(Box::new(move |ui| {
        let c = ui.get_node::<CheckboxNode>(checkbox_idx).unwrap();
        vsync_release.set(c.selected);
    }));

    // Showing the screen fires on_show, pulling the host's current state in.
    ui.set_visible(checkbox_idx, true).unwrap();
    assert!(ui.get_node::<CheckboxNode>(checkbox_idx).unwrap().selected);

    // Clicking the checkbox toggles `selected` (built-in), then fires on_release.
    let click = UiInput::new((16.0, 16.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&click).unwrap();
    assert!(!ui.get_node::<CheckboxNode>(checkbox_idx).unwrap().selected);

    // Result: the host-owned state reflects the new value.
    assert!(!vsync.get());
}

#[test]
fn slider_drag_updates_value_with_clamping() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (slider_idx, slider) = ui.create_slider(0).unwrap();
    slider.set_min_max(0, 100);
    slider.set_value(50);
    ui.layout_slider(slider_idx).unwrap();

    let UiUpdate::Full(_, verts) = ui.flush_all() else { panic!("expected UiUpdate::Full") };

    let thumb_idx = ui.get_node::<SliderNode>(slider_idx).unwrap().get_thumb().unwrap();
    let thumb_offset = ui.get_node::<ButtonNode>(thumb_idx).unwrap().base.vertex_offset;
    // value=50 of [0, 100] -> thumb centred: left = 0.5 * (200 - 16) = 92.
    assert_eq!(verts[thumb_offset].pos, Pos2::new(92.0, 0.0));

    // Press on the thumb to start a drag.
    let press = UiInput::new((100.0, 16.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();

    // Drag far past the right edge -> clamps to max_value rather than overflowing.
    let drag = UiInput::new((2000.0, 16.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 100);

    // Releasing stops the drag.
    let release = UiInput::new((2000.0, 16.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&release).unwrap();

    let UiUpdate::Partial(patches) = ui.flush_dirty() else { panic!("expected UiUpdate::Partial") };
    let thumb_patch = &patches.iter().find(|(offset, _)| *offset == thumb_offset).expect("thumb patch").1;
    // value=100 -> thumb pushed fully right: left = 1.0 * (200 - 16) = 184.
    assert_eq!(thumb_patch[0].pos, Pos2::new(184.0, 0.0));

    // Drag back down past the left edge -> clamps to min_value.
    let press = UiInput::new((192.0, 16.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();

    let drag = UiInput::new((-2000.0, 16.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 0);

    let release = UiInput::new((-2000.0, 16.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&release).unwrap();

    let UiUpdate::Partial(patches) = ui.flush_dirty() else { panic!("expected UiUpdate::Partial") };
    let thumb_patch = &patches.iter().find(|(offset, _)| *offset == thumb_offset).expect("thumb patch").1;
    // value=0 -> thumb back at the left edge.
    assert_eq!(thumb_patch[0].pos, Pos2::new(0.0, 0.0));
}

#[test]
fn slider_track_click_jumps_value_then_drag_continues_from_there() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (slider_idx, slider) = ui.create_slider(0).unwrap();
    slider.set_min_max(0, 100);
    slider.set_value(50);
    ui.layout_slider(slider_idx).unwrap();

    let UiUpdate::Full(_, verts) = ui.flush_all() else { panic!("expected UiUpdate::Full") };
    let thumb_idx = ui.get_node::<SliderNode>(slider_idx).unwrap().get_thumb().unwrap();
    let thumb_offset = ui.get_node::<ButtonNode>(thumb_idx).unwrap().base.vertex_offset;
    // value=50 -> thumb centred: left = 0.5 * (200 - 16) = 92.
    assert_eq!(verts[thumb_offset].pos, Pos2::new(92.0, 0.0));

    // Click (press+release, no movement) on the track far to the right of the
    // thumb (which spans [92, 108]) -> jumps the value to that position.
    let press = UiInput::new((192.0, 16.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 100);

    let release = UiInput::new((192.0, 16.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&release).unwrap();

    let UiUpdate::Partial(patches) = ui.flush_dirty() else { panic!("expected UiUpdate::Partial") };
    let thumb_patch = &patches.iter().find(|(offset, _)| *offset == thumb_offset).expect("thumb patch").1;
    // value=100 -> thumb pushed fully right: left = 1.0 * (200 - 16) = 184.
    assert_eq!(thumb_patch[0].pos, Pos2::new(184.0, 0.0));

    // Click on the track far to the left -> jumps to value 0, then drag right
    // by 92px (half the usable width) -> value moves by 50 from there.
    let press = UiInput::new((8.0, 16.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 0);

    let drag = UiInput::new((100.0, 16.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 50);

    let release = UiInput::new((100.0, 16.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&release).unwrap();
}

#[test]
fn get_node_errors_on_bad_index_or_wrong_type() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (panel_idx, _) = ui.create_panel(0).unwrap();

    // Out of bounds.
    assert!(ui.get_node::<PanelNode>(99).is_err());
    assert!(ui.get_node_mut::<PanelNode>(99).is_err());

    // Wrong variant for a valid index.
    assert!(ui.get_node::<CheckboxNode>(panel_idx).is_err());
    assert!(ui.get_node_mut::<CheckboxNode>(panel_idx).is_err());

    // Correct type still succeeds.
    assert!(ui.get_node::<PanelNode>(panel_idx).is_ok());
}

#[test]
fn unhandled_click_on_empty_space_is_queued_as_event() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (_, panel) = ui.create_panel(0).unwrap();
    panel.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    panel.base.set_size(50.0, 50.0);

    // Release the primary button somewhere the panel doesn't cover.
    let click = UiInput::new((700.0, 500.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&click).unwrap();

    let events = ui.take_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], UiEvent::Unhandled));

    // take_events drains the queue.
    assert!(ui.take_events().is_empty());

    // A click that lands on a node isn't reported as unhandled.
    let click_on_panel = UiInput::new((10.0, 10.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&click_on_panel).unwrap();
    assert!(ui.take_events().is_empty());
}

#[test]
fn set_visible_false_fires_on_hide_and_restores_hover() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (container_idx, container) = ui.create_container(0).unwrap();
    container.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    container.base.set_size(200.0, 100.0);

    let (checkbox_idx, checkbox) = ui.create_checkbox(container_idx).unwrap();
    checkbox.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    checkbox.base.set_size(32.0, 32.0);

    let hidden = Rc::new(Cell::new(false));
    let hidden_cb = hidden.clone();
    ui.get_node_mut::<ContainerNode>(container_idx).unwrap().base.visibility.on_hide = Some(Box::new(move |_ui| {
        hidden_cb.set(true);
    }));

    // Hover over the checkbox.
    let hover = UiInput::new((16.0, 16.0));
    ui.handle_input(&hover).unwrap();
    assert_eq!(ui.hovered_node, Some(checkbox_idx));

    // Hiding the container restores the checkbox's hover state and fires on_hide.
    ui.set_visible(container_idx, false).unwrap();

    assert_eq!(ui.hovered_node, None);
    assert!(!ui.get_node::<ContainerNode>(container_idx).unwrap().base.visible);
    assert!(ui.dirty);
    assert!(hidden.get());
}

#[test]
fn anchored_to_target_tracks_target_position() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    // A panel positioned away from the origin...
    let (anchor_idx, anchor) = ui.create_panel(0).unwrap();
    anchor.base.set_position(Anchor::TopLeft, 100.0, 50.0);
    anchor.base.set_size(40.0, 20.0);

    // ...and a second panel anchored to the first's right edge.
    let (satellite_idx, satellite) = ui.create_panel(0).unwrap();
    satellite.base.set_position_anchored_to(Anchor::Left, anchor_idx, Anchor::Right, 8.0, 0.0);
    satellite.base.set_size(10.0, 10.0);

    let UiUpdate::Full(_, verts) = ui.flush_all() else { panic!("expected UiUpdate::Full") };

    // anchor's right edge is at x=140, vertically centred at y=60; the
    // satellite's Left anchor sits 8px past that, vertically centred.
    let offset = ui.get_node::<PanelNode>(satellite_idx).unwrap().base.vertex_offset;
    assert_eq!(verts[offset].pos, Pos2::new(148.0, 55.0)); // top-left: (140+8, 60-5)

    // Moving the anchor repositions the satellite on the next flush.
    ui.get_node_mut::<PanelNode>(anchor_idx).unwrap().base.set_position(Anchor::TopLeft, 200.0, 150.0);
    let UiUpdate::Full(_, verts) = ui.flush_all() else { panic!("expected UiUpdate::Full") };
    assert_eq!(verts[offset].pos, Pos2::new(248.0, 155.0)); // top-left: (240+8, 160-5)
}

#[test]
fn take_events_drains_queued_events_in_order() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (_, button) = ui.create_button(0).unwrap();
    button.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    button.base.set_size(32.0, 32.0);
    button.interaction.on_release = Some(Box::new(|ui| {
        ui.request_cursor(CursorRequest::Lock);
        ui.request_exit();
    }));

    let click = UiInput::new((16.0, 16.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&click).unwrap();

    let events = ui.take_events();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], UiEvent::SetCursor(CursorRequest::Lock)));
    assert!(matches!(events[1], UiEvent::Exit));

    // Drained by the call above; nothing left to take.
    assert!(ui.take_events().is_empty());
}
