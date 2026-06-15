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

    let (container_idx, container) = ui.create_group(0).unwrap();
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

    let (slider_idx, slider) = ui.create_slider(0, Axis::Horizontal).unwrap();
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

    let (slider_idx, slider) = ui.create_slider(0, Axis::Horizontal).unwrap();
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
fn step_slider_reaches_max_value_even_off_grid() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (slider_idx, slider) = ui.create_slider(0, Axis::Horizontal).unwrap();
    // max_value (95) isn't a multiple of step_size (10): naive clamp-then-snap
    // would land on 90 and get stuck there forever.
    slider.set_min_max(0, 95);
    slider.step_size = 10;
    slider.set_value(80);

    for _ in 0..2 {
        ui.step_slider(slider_idx, true).unwrap();
    }
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 95);

    // Further increments stay pinned at max_value.
    ui.step_slider(slider_idx, true).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 95);

    // Decrementing from the off-grid max snaps back onto the step grid.
    ui.step_slider(slider_idx, false).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 80);
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

    let (container_idx, container) = ui.create_group(0).unwrap();
    container.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    container.base.set_size(200.0, 100.0);

    let (checkbox_idx, checkbox) = ui.create_checkbox(container_idx).unwrap();
    checkbox.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    checkbox.base.set_size(32.0, 32.0);

    let hidden = Rc::new(Cell::new(false));
    let hidden_cb = hidden.clone();
    ui.get_node_mut::<GroupNode>(container_idx).unwrap().base.visibility.on_hide = Some(Box::new(move |_ui| {
        hidden_cb.set(true);
    }));

    // Hover over the checkbox.
    let hover = UiInput::new((16.0, 16.0));
    ui.handle_input(&hover).unwrap();
    assert_eq!(ui.hovered_node, Some(checkbox_idx));

    // Hiding the container restores the checkbox's hover state and fires on_hide.
    ui.set_visible(container_idx, false).unwrap();

    assert_eq!(ui.hovered_node, None);
    assert!(!ui.get_node::<GroupNode>(container_idx).unwrap().base.visible);
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
fn add_child_on_leaf_node_errors() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (label_idx, _) = ui.create_label(0).unwrap();
    assert!(ui.create_panel(label_idx).is_err());
}

/// Three overlapping panels, all registered as orderable in creation order
/// (A, B, C), so C starts on top. Pressing on A's exposed corner (not
/// covered by B or C) raises it above both, both for hit-testing and
/// rendering.
#[test]
fn raise_to_front_reorders_render_and_hit_test() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (container_idx, container) = ui.create_group(0).unwrap();
    container.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    container.base.set_size(100.0, 100.0);

    let (a_idx, a) = ui.create_panel(container_idx).unwrap();
    a.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    a.base.set_size(60.0, 60.0);

    let (b_idx, b) = ui.create_panel(container_idx).unwrap();
    b.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    b.base.set_size(40.0, 40.0);

    let (c_idx, c) = ui.create_panel(container_idx).unwrap();
    c.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    c.base.set_size(40.0, 40.0);

    ui.register_orderable(a_idx).unwrap();
    ui.register_orderable(b_idx).unwrap();
    ui.register_orderable(c_idx).unwrap();

    // Initial order matches registration order: A, B, C (C on top).
    assert_eq!(ui.tree.ordered_children(container_idx), vec![a_idx, b_idx, c_idx]);

    let root_edges = Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };

    // (20, 20) is covered by all three -> topmost (C) wins.
    assert_eq!(ui.tree.hit_test(20.0, 20.0, 0, &root_edges, None), Some(c_idx));

    // Press on A's exposed corner (only A covers (50, 50)) -> raises A.
    let press = UiInput::new((50.0, 50.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();

    // A is now on top of B and C.
    assert_eq!(ui.tree.ordered_children(container_idx), vec![b_idx, c_idx, a_idx]);
    assert_eq!(ui.tree.hit_test(20.0, 20.0, 0, &root_edges, None), Some(a_idx));

    // Render order follows the new z-order too: A is drawn last (on top).
    let _ = ui.flush_all();
    let a_offset = ui.get_node::<PanelNode>(a_idx).unwrap().base.vertex_offset;
    let b_offset = ui.get_node::<PanelNode>(b_idx).unwrap().base.vertex_offset;
    let c_offset = ui.get_node::<PanelNode>(c_idx).unwrap().base.vertex_offset;
    assert!(a_offset > b_offset && a_offset > c_offset);
}

/// Two overlapping panels, each with a nested panel child, both registered
/// as orderable. Raising a panel to the front must move its entire subtree
/// (own quad + child), not just its own quad, so it stays grouped as a
/// contiguous block ahead of the other panel's subtree in the vertex buffer.
#[test]
fn raise_moves_subtree_as_a_block_in_vertex_buffer() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (container_idx, container) = ui.create_group(0).unwrap();
    container.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    container.base.set_size(100.0, 100.0);

    let (a_idx, a) = ui.create_panel(container_idx).unwrap();
    a.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    a.base.set_size(60.0, 60.0);

    let (a_child_idx, a_child) = ui.create_panel(a_idx).unwrap();
    a_child.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    a_child.base.set_size(5.0, 5.0);

    let (b_idx, b) = ui.create_panel(container_idx).unwrap();
    b.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    b.base.set_size(40.0, 40.0);

    let (b_child_idx, b_child) = ui.create_panel(b_idx).unwrap();
    b_child.base.set_position(Anchor::TopLeft, 5.0, 5.0);
    b_child.base.set_size(5.0, 5.0);

    ui.register_orderable(a_idx).unwrap();
    ui.register_orderable(b_idx).unwrap();

    // B registered last -> on top initially. A's subtree (own quad + child)
    // forms one contiguous block, entirely before B's subtree.
    let _ = ui.flush_all();
    let a_off       = ui.get_node::<PanelNode>(a_idx).unwrap().base.vertex_offset;
    let a_child_off = ui.get_node::<PanelNode>(a_child_idx).unwrap().base.vertex_offset;
    let b_off       = ui.get_node::<PanelNode>(b_idx).unwrap().base.vertex_offset;
    let b_child_off = ui.get_node::<PanelNode>(b_child_idx).unwrap().base.vertex_offset;
    assert!(a_off < a_child_off && a_child_off < b_off && b_off < b_child_off);

    // Press on A's exposed corner (only A covers (52, 52)) -> raises A.
    let press = UiInput::new((52.0, 52.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    assert_eq!(ui.tree.ordered_children(container_idx), vec![b_idx, a_idx]);

    // A's subtree now forms a contiguous block, entirely after B's subtree.
    let _ = ui.flush_all();
    let a_off       = ui.get_node::<PanelNode>(a_idx).unwrap().base.vertex_offset;
    let a_child_off = ui.get_node::<PanelNode>(a_child_idx).unwrap().base.vertex_offset;
    let b_off       = ui.get_node::<PanelNode>(b_idx).unwrap().base.vertex_offset;
    let b_child_off = ui.get_node::<PanelNode>(b_child_idx).unwrap().base.vertex_offset;
    assert!(b_off < b_child_off && b_child_off < a_off && a_off < a_child_off);
}

/// Two overlapping "windows" (containers), both registered as orderable.
/// Clicking a button inside one of them (in the area the other doesn't
/// cover) raises that window itself, even though the button was never
/// registered as orderable.
#[test]
fn raise_propagates_to_orderable_ancestor() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (win1_idx, win1) = ui.create_group(0).unwrap();
    win1.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    win1.base.set_size(60.0, 60.0);

    let (win2_idx, win2) = ui.create_group(0).unwrap();
    win2.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    win2.base.set_size(40.0, 40.0);

    ui.register_orderable(win1_idx).unwrap();
    ui.register_orderable(win2_idx).unwrap();

    // win2 registered last -> on top initially.
    assert_eq!(ui.tree.ordered_children(0), vec![win1_idx, win2_idx]);

    // A button inside win1, in the area win2 doesn't cover.
    let (_, button) = ui.create_button(win1_idx).unwrap();
    button.base.set_position(Anchor::TopLeft, 50.0, 50.0);
    button.base.set_size(8.0, 8.0);

    // Clicking the button raises win1 (its container) to the front.
    let press = UiInput::new((54.0, 54.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();

    assert_eq!(ui.tree.ordered_children(0), vec![win2_idx, win1_idx]);
}

/// Bands assigned via `register_layer` (in registration order) take priority
/// over `z_index`: a layer registered later always sorts above one
/// registered earlier, regardless of how high the earlier layer's `z_index`
/// gets raised.
#[test]
fn register_layer_bands_take_priority_over_z_index() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    enum TestLayer { Content, Debug }

    let (content_idx, _) = ui.create_group(0).unwrap();
    let (debug_idx, _) = ui.create_group(0).unwrap();

    // Content registered first -> band 0; Debug registered second -> band 1.
    ui.register_layer(content_idx, TestLayer::Content).unwrap();
    ui.register_layer(debug_idx, TestLayer::Debug).unwrap();

    // Give content a non-zero z_index -- it still loses to debug's band.
    ui.register_orderable(content_idx).unwrap();

    assert_eq!(ui.tree.ordered_children(0), vec![content_idx, debug_idx]);
}

#[test]
fn register_layer_errors_for_non_root_child() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    enum TestLayer { Content }

    let (container_idx, _) = ui.create_group(0).unwrap();
    let (nested_idx, _) = ui.create_group(container_idx).unwrap();

    assert!(ui.register_layer(nested_idx, TestLayer::Content).is_err());
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

#[test]
fn create_window_wires_titlebar_title_close_and_body() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (window_idx, window) = ui.create_window(0, 200.0, 150.0).unwrap();
    let titlebar_idx = window.titlebar;
    let title_idx    = window.title;
    let close_idx    = window.close_button;
    let body_idx     = window.body;
    let children     = window.container.children.clone();

    assert_eq!(children, vec![titlebar_idx, body_idx]);

    // All parts are distinct nodes.
    let mut indices = vec![window_idx, titlebar_idx, title_idx, close_idx, body_idx];
    indices.sort();
    indices.dedup();
    assert_eq!(indices.len(), 5);

    // Titlebar is inset from the window's edges by WINDOW_BORDER, and
    // contains the title label and close button.
    let titlebar = ui.get_node::<PanelNode>(titlebar_idx).unwrap();
    assert!(titlebar.container.children.contains(&title_idx));
    assert!(titlebar.container.children.contains(&close_idx));
    assert_eq!(titlebar.base.bounds.width, 200.0 - 2.0 * WINDOW_BORDER);
    assert_eq!(titlebar.base.bounds.height, TITLEBAR_HEIGHT);

    // Body is inset from the window's edges by WINDOW_BORDER, with another
    // WINDOW_BORDER gap below the titlebar.
    let body = ui.get_node::<PanelNode>(body_idx).unwrap();
    assert_eq!(body.base.bounds.width, 200.0 - 2.0 * WINDOW_BORDER);
    assert_eq!(body.base.bounds.height, 150.0 - TITLEBAR_HEIGHT - 3.0 * WINDOW_BORDER);
}

#[test]
fn close_button_hides_window() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (window_idx, _) = ui.create_window(0, 200.0, 150.0).unwrap();

    // Inside the close button, in the titlebar's top-right corner.
    let click = UiInput::new((190.0, 12.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&click).unwrap();

    assert!(!ui.get_node::<WindowNode>(window_idx).unwrap().base.visible);
}

#[test]
fn window_drag_requires_draggable_and_moves_subtree() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (window_idx, window) = ui.create_window(0, 200.0, 150.0).unwrap();
    window.base.set_position(Anchor::TopLeft, 10.0, 10.0);
    let body_idx = window.body;

    // A press+drag on the titlebar (away from the close button) does nothing
    // while `draggable` is false.
    let press = UiInput::new((60.0, 12.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let drag = UiInput::new((100.0, 52.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();
    let release = UiInput::new((100.0, 52.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&release).unwrap();

    let bounds = ui.get_node::<WindowNode>(window_idx).unwrap().base.bounds;
    assert_eq!((bounds.x, bounds.y), (10.0, 10.0));

    // Enabling `draggable` makes the same gesture move the window and its
    // whole subtree by the cursor's delta.
    ui.get_node_mut::<WindowNode>(window_idx).unwrap().set_draggable(true);

    let press = UiInput::new((60.0, 12.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let drag = UiInput::new((100.0, 52.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();

    let bounds = ui.get_node::<WindowNode>(window_idx).unwrap().base.bounds;
    assert_eq!((bounds.x, bounds.y), (50.0, 50.0));

    // The body (a descendant) tracks the new window position.
    let body_edges = ui.node_edges(body_idx);
    assert_eq!((body_edges.left, body_edges.top), (52.0, 78.0));
}

#[test]
fn edges_intersect() {
    let a = Edges { left: 0.0, right: 10.0, top: 0.0, bottom: 10.0 };

    // Overlapping: result is the shared region.
    let b = Edges { left: 5.0, right: 15.0, top: 5.0, bottom: 15.0 };
    assert_eq!(a.intersect(&b), Edges { left: 5.0, right: 10.0, top: 5.0, bottom: 10.0 });

    // Nested: result is the inner rect.
    let c = Edges { left: 2.0, right: 8.0, top: 2.0, bottom: 8.0 };
    assert_eq!(a.intersect(&c), c);

    // Non-overlapping: result is degenerate (left > right, top > bottom).
    let d = Edges { left: 20.0, right: 30.0, top: 20.0, bottom: 30.0 };
    let result = a.intersect(&d);
    assert!(result.left > result.right);
    assert!(result.top > result.bottom);
}

/// A `clip_children` panel groups its children's quads into a batch tagged
/// with its own edges as `clip_rect`, separate from its own quad (rendered
/// under whatever clip it inherited) and from a sibling outside it.
#[test]
fn flush_all_groups_quads_into_clip_batches() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p1_idx, p1) = ui.create_panel(0).unwrap();
    p1.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p1.base.set_size(100.0, 100.0);
    ui.set_clip_children(p1_idx, true).unwrap();

    let (_a_idx, a) = ui.create_panel(p1_idx).unwrap();
    a.base.set_position(Anchor::TopLeft, 10.0, 10.0);
    a.base.set_size(20.0, 20.0);

    // Extends past P1's bounds; still grouped under P1's clip rect.
    let (_b_idx, b) = ui.create_panel(p1_idx).unwrap();
    b.base.set_position(Anchor::TopLeft, 90.0, 90.0);
    b.base.set_size(50.0, 50.0);

    // A sibling outside P1, unclipped.
    let (_s_idx, s) = ui.create_panel(0).unwrap();
    s.base.set_position(Anchor::TopLeft, 200.0, 200.0);
    s.base.set_size(30.0, 30.0);

    let _ = ui.flush_all();
    let p1_edges = ui.node_edges(p1_idx);

    assert_eq!(ui.batches(), &[
        DrawBatch { clip_rect: None, first_quad: 0, quad_count: 1 },              // P1's own quad
        DrawBatch { clip_rect: Some(p1_edges), first_quad: 1, quad_count: 2 },    // A and B
        DrawBatch { clip_rect: None, first_quad: 3, quad_count: 1 },              // S
    ]);
}

/// A node nested inside two `clip_children` ancestors is clipped to the
/// intersection of both ancestors' bounds.
#[test]
fn flush_all_nested_clip_children_intersects_ancestors() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p1_idx, p1) = ui.create_panel(0).unwrap();
    p1.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p1.base.set_size(200.0, 200.0);
    ui.set_clip_children(p1_idx, true).unwrap();

    // Extends past P1's right/bottom edges.
    let (p2_idx, p2) = ui.create_panel(p1_idx).unwrap();
    p2.base.set_position(Anchor::TopLeft, 100.0, 100.0);
    p2.base.set_size(200.0, 200.0);
    ui.set_clip_children(p2_idx, true).unwrap();

    let (_c_idx, c) = ui.create_panel(p2_idx).unwrap();
    c.base.set_position(Anchor::TopLeft, 10.0, 10.0);
    c.base.set_size(20.0, 20.0);

    let _ = ui.flush_all();

    let p1_edges = ui.node_edges(p1_idx);
    let p2_edges = ui.node_edges(p2_idx);
    let expected_clip = Some(p1_edges.intersect(&p2_edges));

    // C's quad is the last one emitted; its batch is clipped by both ancestors.
    assert_eq!(ui.batches().last().unwrap().clip_rect, expected_clip);
}

/// Dragging a draggable window moves its body (a `clip_children` node by
/// default), so `flush_dirty` must refresh the `clip_rect` of the batch
/// holding an overflowing child to match the body's new position.
#[test]
fn flush_dirty_refreshes_clip_rect_on_window_drag() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (_window_idx, window) = ui.create_window(0, 200.0, 150.0).unwrap();
    window.base.set_position(Anchor::TopLeft, 10.0, 10.0);
    window.set_draggable(true);
    let body_idx = window.body;

    // Overflows the body's bounds, so it's clipped.
    let (child_idx, child) = ui.create_panel(body_idx).unwrap();
    child.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    child.base.set_size(500.0, 500.0);

    let _ = ui.flush_all();

    // Drag the titlebar by (40, 40).
    let press = UiInput::new((60.0, 12.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let drag = UiInput::new((100.0, 52.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();

    let UiUpdate::Partial(_) = ui.flush_dirty() else { panic!("expected UiUpdate::Partial") };

    let body_edges = ui.node_edges(body_idx);
    let child_quad = ui.get_node::<PanelNode>(child_idx).unwrap().base.vertex_offset / 4;

    let batch = ui.batches().iter()
        .find(|b| child_quad >= b.first_quad && child_quad < b.first_quad + b.quad_count)
        .unwrap();
    assert_eq!(batch.clip_rect, Some(body_edges));
}

/// A `Group` nested inside a draggable window's body never gets a real
/// `vertex_offset` (it renders no quad of its own), so `mark_dirty` must not
/// queue it for `refresh_batch_clip` — otherwise dragging the window would
/// mistarget batch 0 with the group's resolved clip rect.
#[test]
fn flush_dirty_skips_containers_and_does_not_clobber_unrelated_batch() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    // An unclipped panel elsewhere in the tree -> part of batch 0.
    let (_, other) = ui.create_panel(0).unwrap();
    other.base.set_position(Anchor::TopLeft, 300.0, 300.0);
    other.base.set_size(50.0, 50.0);

    let (_window_idx, window) = ui.create_window(0, 200.0, 150.0).unwrap();
    window.base.set_position(Anchor::TopLeft, 10.0, 10.0);
    window.set_draggable(true);
    let body_idx = window.body;

    // A Container nested inside the draggable window's body.
    let (_, container) = ui.create_group(body_idx).unwrap();
    container.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    container.base.set_size(50.0, 50.0);

    let _ = ui.flush_all();
    let batch0_before = ui.batches()[0];

    // Drag the titlebar by (40, 40).
    let press = UiInput::new((60.0, 12.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let drag = UiInput::new((100.0, 52.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();
    let _ = ui.flush_dirty();

    assert_eq!(ui.batches()[0], batch0_before);
}

/// A child positioned outside a `clip_children` ancestor's bounds is not hit,
/// even though its own resolved edges would otherwise contain the cursor.
#[test]
fn hit_test_respects_clip_children() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p1_idx, p1) = ui.create_panel(0).unwrap();
    p1.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p1.base.set_size(100.0, 100.0);
    ui.set_clip_children(p1_idx, true).unwrap();

    // Extends past P1's bounds: (90, 90) to (140, 140).
    let (button_idx, button) = ui.create_button(p1_idx).unwrap();
    button.base.set_position(Anchor::TopLeft, 90.0, 90.0);
    button.base.set_size(50.0, 50.0);

    let _ = ui.flush_all();

    let root_edges = Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };

    // Within the button's own bounds, but outside P1's clip rect -> no hit.
    assert_eq!(ui.tree.hit_test(120.0, 120.0, 0, &root_edges, None), None);

    // Within both the button's bounds and P1's clip rect -> hit.
    assert_eq!(ui.tree.hit_test(95.0, 95.0, 0, &root_edges, None), Some(button_idx));
}

/// A window dragged inside a parent with `clamp_children` set can't be
/// dragged past that parent's edges: it stops flush against whichever edge
/// the cursor overshoots.
#[test]
fn window_drag_clamps_to_parent_when_set() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (_outer_idx, outer) = ui.create_window(0, 200.0, 150.0).unwrap();
    outer.base.set_position(Anchor::TopLeft, 10.0, 10.0);
    let body_idx = outer.body;

    let (inner_idx, inner) = ui.create_window(body_idx, 50.0, 40.0).unwrap();
    inner.base.set_position(Anchor::TopLeft, 10.0, 10.0);
    inner.set_draggable(true);

    ui.set_clamp_children(body_idx, true).unwrap();

    let _ = ui.flush_all();
    let body_edges = ui.node_edges(body_idx);

    // Press on the inner window's titlebar, then drag far up-left, past the
    // body's top-left corner.
    let press = UiInput::new((30.0, 50.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let drag = UiInput::new((-170.0, -150.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();

    let edges = ui.node_edges(inner_idx);
    assert_eq!((edges.left, edges.top), (body_edges.left, body_edges.top));

    // Drag far down-right, past the body's bottom-right corner.
    let drag = UiInput::new((830.0, 850.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();

    let edges = ui.node_edges(inner_idx);
    assert_eq!((edges.right, edges.bottom), (body_edges.right, body_edges.bottom));
}

/// A window dragged inside a scroll panel with `clamp_children` set clamps
/// to the panel's full `content_size`, not its (smaller) viewport — its
/// resolved edges are already in the content's scrolled coordinate space, so
/// the clamp rect must be too.
#[test]
fn window_drag_clamps_to_scroll_panel_content_size() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p_idx, p) = ui.create_panel(0).unwrap();
    p.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p.base.set_size(100.0, 100.0);
    p.enable_scroll((300.0, 300.0));
    ui.set_clip_children(p_idx, true).unwrap();
    ui.set_clamp_children(p_idx, true).unwrap();
    ui.set_scroll_offset(p_idx, (20.0, 20.0)).unwrap();

    let (inner_idx, inner) = ui.create_window(p_idx, 40.0, 30.0).unwrap();
    inner.base.set_position(Anchor::TopLeft, 30.0, 30.0);
    inner.set_draggable(true);

    let _ = ui.flush_all();

    // Content rect, in the content's scrolled coordinate space: the
    // viewport's top-left (0,0) translated by -offset (20,20), extended by
    // content_size (300,300) -> (-20,-20)..(280,280). The inner window
    // starts at resolved (10,10)..(50,40), within the viewport.
    let content_min = -20.0;
    let content_max = content_min + 300.0;

    // Drag far up-left, past the content area's top-left corner. (x=15,
    // rather than further right, to avoid the titlebar's close button.)
    let press = UiInput::new((15.0, 20.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let drag = UiInput::new((-1000.0, -1000.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();

    let edges = ui.node_edges(inner_idx);
    assert_eq!((edges.left, edges.top), (content_min, content_min));

    // Drag far down-right, past the content area's bottom-right corner.
    let drag = UiInput::new((1000.0, 1000.0)).with_mouse_button(MouseButton::Primary, true, false, false);
    ui.handle_input(&drag).unwrap();

    let edges = ui.node_edges(inner_idx);
    assert_eq!((edges.right, edges.bottom), (content_max, content_max));
}

/// The `clip_rect` of the draw batch containing `idx`'s quad, found the same
/// way [`flush_dirty_refreshes_clip_rect_on_window_drag`] does.
fn batch_clip_for(ui: &Ui, idx: usize) -> Option<Edges> {
    let quad = ui.get_node::<PanelNode>(idx).unwrap().base.vertex_offset / 4;
    ui.batches().iter()
        .find(|b| quad >= b.first_quad && quad < b.first_quad + b.quad_count)
        .unwrap()
        .clip_rect
}

/// Scrolling a panel shifts its children's resolved positions by
/// `-scroll_offset`, while the clip rect they inherit via `clip_children`
/// stays anchored to the panel's own unshifted bounds — content moves within
/// a fixed viewport. `hit_test` reflects the shifted positions too.
#[test]
fn scroll_offset_shifts_children_within_fixed_viewport() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p_idx, p) = ui.create_panel(0).unwrap();
    p.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p.base.set_size(100.0, 100.0);
    p.enable_scroll((100.0, 200.0));
    ui.set_clip_children(p_idx, true).unwrap();

    // Two stacked children spanning the full (taller-than-viewport) content.
    let (a_idx, a) = ui.create_panel(p_idx).unwrap();
    a.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    a.base.set_size(100.0, 100.0);

    let (b_idx, b) = ui.create_panel(p_idx).unwrap();
    b.base.set_position(Anchor::TopLeft, 0.0, 100.0);
    b.base.set_size(100.0, 100.0);

    let _ = ui.flush_all();
    let p_edges = ui.node_edges(p_idx);
    let root_edges = Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };

    // Before scrolling: A fills the viewport, both clipped to P's bounds.
    assert_eq!(ui.node_edges(a_idx), Edges { left: 0.0, right: 100.0, top: 0.0, bottom: 100.0 });
    assert_eq!(batch_clip_for(&ui, a_idx), Some(p_edges));
    assert_eq!(batch_clip_for(&ui, b_idx), Some(p_edges));
    assert_eq!(ui.tree.hit_test(50.0, 50.0, 0, &root_edges, None), Some(a_idx));

    // Scroll down past the max (clamped to content_size - viewport = (0, 100)).
    let p = ui.get_node_mut::<PanelNode>(p_idx).unwrap();
    p.scroll_by((0.0, 1000.0));
    assert_eq!(p.scroll.as_ref().unwrap().offset, (0.0, 100.0));
    ui.mark_dirty(p_idx);
    let _ = ui.flush_dirty();

    // A has scrolled fully out of the viewport; B now fills it.
    assert_eq!(ui.node_edges(a_idx), Edges { left: 0.0, right: 100.0, top: -100.0, bottom: 0.0 });
    assert_eq!(ui.node_edges(b_idx), Edges { left: 0.0, right: 100.0, top: 0.0, bottom: 100.0 });

    // The viewport itself (P's edges and the inherited clip rect) hasn't moved.
    assert_eq!(ui.node_edges(p_idx), p_edges);
    assert_eq!(batch_clip_for(&ui, a_idx), Some(p_edges));
    assert_eq!(batch_clip_for(&ui, b_idx), Some(p_edges));

    // hit_test at the same screen point now resolves to B.
    assert_eq!(ui.tree.hit_test(50.0, 50.0, 0, &root_edges, None), Some(b_idx));
}

/// `PanelNode::scroll_by`/`set_scroll_offset` clamp the offset to
/// `[0, content_size - bounds.size]` per axis.
#[test]
fn panel_scroll_offset_clamps_to_content_size() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p_idx, p) = ui.create_panel(0).unwrap();
    p.base.set_size(100.0, 100.0);
    p.enable_scroll((100.0, 250.0));

    let p = ui.get_node_mut::<PanelNode>(p_idx).unwrap();

    // Overshoot downward -> clamps to max_offset = content_size - bounds = (0, 150).
    p.scroll_by((0.0, 1000.0));
    assert_eq!(p.scroll.as_ref().unwrap().offset, (0.0, 150.0));

    // Overshoot back up -> clamps to 0.
    p.scroll_by((0.0, -1000.0));
    assert_eq!(p.scroll.as_ref().unwrap().offset, (0.0, 0.0));

    // Horizontal content_size == bounds.width -> max_offset.x is 0.
    p.scroll_by((1000.0, 0.0));
    assert_eq!(p.scroll.as_ref().unwrap().offset.0, 0.0);
}

/// `handle_input` routes a scroll-wheel delta to the nearest scroll-enabled
/// ancestor of the hovered node (walking up through non-scrollable
/// children), adding it to that panel's offset, clamped. Scrolling while
/// hovering outside the panel is a no-op.
#[test]
fn handle_input_routes_scroll_to_hovered_scroll_panel() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p_idx, p) = ui.create_panel(0).unwrap();
    p.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p.base.set_size(100.0, 100.0);
    p.enable_scroll((100.0, 200.0));
    ui.set_clip_children(p_idx, true).unwrap();

    // Non-scrollable child filling the viewport - scroll input hovering over
    // it should route up to its scrollable parent.
    let (_child_idx, child) = ui.create_panel(p_idx).unwrap();
    child.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    child.base.set_size(100.0, 100.0);

    let _ = ui.flush_all();

    // One wheel "line" with no scrollbar moves by `DEFAULT_LINE_STEP` (48px).
    let scroll = UiInput::new((50.0, 50.0)).with_scroll_delta((0.0, 1.0));
    ui.handle_input(&scroll).unwrap();
    assert_eq!(ui.get_node::<PanelNode>(p_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 48.0));

    // Overshoot clamps to max_offset = content_size - bounds = (0, 100).
    let scroll = UiInput::new((50.0, 50.0)).with_scroll_delta((0.0, 1000.0));
    ui.handle_input(&scroll).unwrap();
    assert_eq!(ui.get_node::<PanelNode>(p_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 100.0));

    // Scrolling while hovering outside the panel is a no-op.
    let p = ui.get_node_mut::<PanelNode>(p_idx).unwrap();
    p.set_scroll_offset((0.0, 0.0));
    ui.mark_dirty(p_idx);
    let scroll = UiInput::new((700.0, 500.0)).with_scroll_delta((0.0, 1.0));
    ui.handle_input(&scroll).unwrap();
    assert_eq!(ui.get_node::<PanelNode>(p_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 0.0));
}

/// Scroll-wheel input syncs a scroll panel's [`Scroll::scrollbar`] slider:
/// the slider's value tracks the offset along its own axis, and its thumb is
/// re-laid-out to match.
#[test]
fn scroll_wheel_syncs_scrollbar_slider() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p_idx, p) = ui.create_panel(0).unwrap();
    p.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p.base.set_size(100.0, 100.0);
    p.enable_scroll((100.0, 200.0)); // max_offset = (0, 100)
    ui.set_clip_children(p_idx, true).unwrap();

    let (slider_idx, slider) = ui.create_slider(0, Axis::Vertical).unwrap();
    slider.set_min_max(0, 100);

    let p = ui.get_node_mut::<PanelNode>(p_idx).unwrap();
    p.scroll.as_mut().unwrap().scrollbar = Some(slider_idx);

    let _ = ui.flush_all();

    let scroll = UiInput::new((50.0, 50.0)).with_scroll_delta((0.0, 40.0));
    ui.handle_input(&scroll).unwrap();

    assert_eq!(ui.get_node::<PanelNode>(p_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 40.0));
    assert_eq!(ui.get_node::<SliderNode>(slider_idx).unwrap().value, 40);

    // Thumb's resolved top should reflect value=40 of [0, 100]:
    // 0.4 * (slider_extent=200 - thumb_extent=16) = 73.6.
    let thumb_idx = ui.get_node::<SliderNode>(slider_idx).unwrap().get_thumb().unwrap();
    let slider_top = ui.node_edges(slider_idx).top;
    let thumb_top  = ui.node_edges(thumb_idx).top;
    assert_eq!(thumb_top - slider_top, 73.6);
}

/// A wheel "line" scrolls a panel by its [`Scroll::scrollbar`]'s
/// `step_size` along that slider's axis — the same amount as one click of
/// its step buttons (via [`Ui::step_slider`]) — rather than a fixed pixel
/// amount.
#[test]
fn scroll_wheel_uses_scrollbar_step_size() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let (p_idx, p) = ui.create_panel(0).unwrap();
    p.base.set_position(Anchor::TopLeft, 0.0, 0.0);
    p.base.set_size(100.0, 100.0);
    p.enable_scroll((200.0, 200.0)); // max_offset = (100, 100)
    ui.set_clip_children(p_idx, true).unwrap();

    let (slider_idx, slider) = ui.create_slider(0, Axis::Vertical).unwrap();
    slider.set_min_max(0, 100);
    slider.step_size = 8;

    let p = ui.get_node_mut::<PanelNode>(p_idx).unwrap();
    p.scroll.as_mut().unwrap().scrollbar = Some(slider_idx);

    let _ = ui.flush_all();

    // One line along the scrollbar's axis moves by its step_size (8px), not
    // the default 48px line step.
    let scroll = UiInput::new((50.0, 50.0)).with_scroll_delta((0.0, 1.0));
    ui.handle_input(&scroll).unwrap();
    assert_eq!(ui.get_node::<PanelNode>(p_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 8.0));

    // The cross axis (no scrollbar on it) still uses the default 48px line step.
    let scroll = UiInput::new((50.0, 50.0)).with_scroll_delta((1.0, 0.0));
    ui.handle_input(&scroll).unwrap();
    assert_eq!(ui.get_node::<PanelNode>(p_idx).unwrap().scroll.as_ref().unwrap().offset.0, 48.0);
}

/// `create_scroll_panel` lays out a content panel, scrollbar track, and
/// dec/inc step buttons around a `viewport`, and wires them together: the
/// content panel scrolls and clips, the scrollbar's range/thumb match
/// `content_size`/`viewport`, and dragging either the scrollbar or the step
/// buttons keeps the content's offset and the scrollbar's value in sync.
#[test]
fn create_scroll_panel_wires_content_scrollbar_and_step_buttons() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let viewport = (100.0, 100.0);
    let scrollbar_width = 20.0;
    let content_size = (100.0, 300.0);
    let (_, frame) = ui.create_scroll_panel(0, Axis::Vertical, viewport, scrollbar_width, content_size).unwrap();
    // Frame = viewport plus the scrollbar's cross-axis extent.
    assert_eq!((frame.base.bounds.width, frame.base.bounds.height), (120.0, 100.0));
    let content_idx = frame.content_idx;
    let scrollbar_idx = frame.scrollbar_idx;

    let content = ui.get_node::<PanelNode>(content_idx).unwrap();
    let scroll = content.scroll.as_ref().unwrap();
    assert_eq!(scroll.content_size, content_size);
    assert_eq!((content.base.bounds.width, content.base.bounds.height), viewport);
    assert!(ui.tree.nodes[content_idx].clip_children());

    // Thumb travel = bar_extent(100) - 2*track_padding(20+2) = 56; proportional
    // size (56*56/300≈10.45) is below the button_size floor (20-4=16), so the
    // thumb is floor-sized at 16.
    let thumb_idx = ui.get_node::<SliderNode>(scrollbar_idx).unwrap().get_thumb().unwrap();
    assert_eq!(ui.get_node::<ButtonNode>(thumb_idx).unwrap().base.bounds.height, 16.0);

    // Scrolling the content clamps to max_offset (300-100=200) and syncs the scrollbar.
    ui.set_scroll_offset(content_idx, (0.0, 1000.0)).unwrap();
    assert_eq!(ui.get_node::<PanelNode>(content_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 200.0));
    assert_eq!(ui.get_node::<SliderNode>(scrollbar_idx).unwrap().value, 200);

    // The decrement button sits at (102,2)-(118,18) (inset within the track's
    // top end by padding_half=2); clicking it steps the scrollbar down by its
    // step_size (1, the default) and pushes the new value back to the content
    // panel via the scrollbar's `on_value_changed`.
    let press = UiInput::new((110.0, 10.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let release = UiInput::new((110.0, 10.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&release).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(scrollbar_idx).unwrap().value, 199);
    assert_eq!(ui.get_node::<PanelNode>(content_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 199.0));

    // The increment button sits at (102,82)-(118,98); steps back up.
    let press = UiInput::new((110.0, 90.0)).with_mouse_button(MouseButton::Primary, true, true, false);
    ui.handle_input(&press).unwrap();
    let release = UiInput::new((110.0, 90.0)).with_mouse_button(MouseButton::Primary, false, false, true);
    ui.handle_input(&release).unwrap();
    assert_eq!(ui.get_node::<SliderNode>(scrollbar_idx).unwrap().value, 200);
    assert_eq!(ui.get_node::<PanelNode>(content_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 200.0));
}

/// `resize_scroll_panel` re-derives the viewport from the frame's
/// already-resized `base.bounds`, resizes/re-clamps the content panel's
/// scroll offset against any new `content_size`, and recomputes the
/// scrollbar's range and thumb size to match.
#[test]
fn resize_scroll_panel_reclamps_offset_and_resizes_thumb() {
    let mut ui = Ui::new((800.0, 600.0), test_atlas());

    let viewport = (100.0, 100.0);
    let scrollbar_width = 20.0;
    let content_size = (100.0, 300.0);
    let (frame_idx, frame) = ui.create_scroll_panel(0, Axis::Vertical, viewport, scrollbar_width, content_size).unwrap();
    let content_idx = frame.content_idx;
    let scrollbar_idx = frame.scrollbar_idx;

    // Scroll to the (current) max offset before resizing.
    ui.set_scroll_offset(content_idx, (0.0, 1000.0)).unwrap();
    assert_eq!(ui.get_node::<PanelNode>(content_idx).unwrap().scroll.as_ref().unwrap().offset, (0.0, 200.0));

    // Grow the frame's overall height to 244px -> viewport.1 = 244, and grow
    // the content to 400px tall.
    ui.get_node_mut::<ScrollPanelNode>(frame_idx).unwrap().base.set_size(120.0, 244.0);
    ui.resize_scroll_panel(frame_idx, (100.0, 400.0)).unwrap();

    let content = ui.get_node::<PanelNode>(content_idx).unwrap();
    assert_eq!((content.base.bounds.width, content.base.bounds.height), (100.0, 244.0));
    // New max_offset = 400 - 244 = 156; the old offset of 200 is re-clamped.
    assert_eq!(content.scroll.as_ref().unwrap().offset, (0.0, 156.0));
    assert_eq!(ui.get_node::<SliderNode>(scrollbar_idx).unwrap().value, 156);

    // Thumb travel = bar_extent(244) - 2*track_padding(20+2) = 200;
    // thumb = (200*200/400).max(16) = 100.
    let thumb_idx = ui.get_node::<SliderNode>(scrollbar_idx).unwrap().get_thumb().unwrap();
    assert_eq!(ui.get_node::<ButtonNode>(thumb_idx).unwrap().base.bounds.height, 100.0);
}
