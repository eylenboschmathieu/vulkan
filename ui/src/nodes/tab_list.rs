use anyhow::Result;
use crate::{types::Rgba, Ui};

use super::{Axis, ButtonNode, PanelNode, SliderNode, UiNode};

const TRACK_COLOR:    Rgba = Rgba { x: 0.15, y: 0.15, z: 0.15, w: 0.6 };
const THUMB_COLOR:    Rgba = Rgba { x: 0.55, y: 0.55, z: 0.55, w: 0.8 };
const THUMB_HOVER:    Rgba = Rgba { x: 0.3,  y: 0.6,  z: 1.0,  w: 0.9 };

/// Vertical gap between the top of the strip and each tab button, also used
/// as left/right padding. Bottom is flush so active-tab color bleeds into body.
pub(crate) const BUTTON_MARGIN: f32 = 3.0;
/// Horizontal gap between consecutive tab buttons.
pub(crate) const BUTTON_GAP: f32 = 2.0;

/// Horizontal strip of tab buttons with a thin hover-revealed scrollbar. Use
/// [`crate::Ui::add_tab`] to append buttons; the caller sets each button's
/// label or texture. Constructed via [`crate::Ui::create_tab_panel`] as part
/// of a [`super::TabPanelNode`].
pub struct TabListNode {
    /// Provides `base`, `container`, and `renderable` — identical to the
    /// [`SliderNode`] pattern.
    pub panel: PanelNode,
    /// Scroll-enabled clip panel that holds the tab buttons.
    pub content_idx: usize,
    /// Thin horizontal [`SliderNode`] at the bottom, initially hidden;
    /// shown by [`crate::Ui`] when the strip overflows and the cursor is over
    /// it.
    pub scrollbar_idx: usize,
    pub(crate) tab_height:       f32,
    pub(crate) scrollbar_height: f32,
    /// Right edge of the last tab button (left_margin + Σwidths + (n-1)*gaps).
    /// Used with `button_margin` to compute the total visual width for scrolling.
    pub(crate) content_width: f32,
    pub(crate) button_margin: f32,
    pub(crate) button_gap:    f32,
}

impl TabListNode {
    pub fn set_color(&mut self, color: crate::types::Rgba) {
        self.panel.set_color(color);
    }
}

impl TabListNode {
    pub(crate) fn new(tab_height: f32, scrollbar_height: f32) -> Self {
        Self {
            panel:         PanelNode::new(),
            content_idx:   0,
            scrollbar_idx: 0,
            tab_height,
            scrollbar_height,
            content_width: 0.0,
            button_margin: BUTTON_MARGIN,
            button_gap:    BUTTON_GAP,
        }
    }

    pub fn build(ui: &mut Ui, parent: usize, width: f32, tab_height: f32, scrollbar_height: f32) -> Result<(usize, &mut Self)> {
        let frame_idx = ui.tree.add_child(
            UiNode::TabList(Self::new(tab_height, scrollbar_height)),
            parent,
        )?;
        {
            let frame = ui.get_node_mut::<Self>(frame_idx)?;
            frame.panel.base.set_size(width, tab_height);
            frame.panel.base.interactive = false;
        }

        // Inner panel: clips buttons, scrolls horizontally.
        let (content_idx, content) = ui.create_panel(frame_idx)?;
        content.base.set_size(width, tab_height);
        content.enable_scroll((0.0, tab_height));
        ui.set_clip_children(content_idx, true)?;

        // Thin scrollbar, hidden until overflow + hover.
        let (scrollbar_idx, _) = ui.create_slider(frame_idx, Axis::Horizontal)?;
        {
            let s = ui.get_node_mut::<SliderNode>(scrollbar_idx)?;
            s.panel.base.set_size(width, scrollbar_height);
            s.panel.base.bounds.y = tab_height - scrollbar_height;
            s.panel.set_color(TRACK_COLOR);
            s.panel.base.visible  = false;
            s.panel.base.tab_stop = false;
        }
        if let Some(thumb_idx) = ui.get_node::<SliderNode>(scrollbar_idx)?.get_thumb() {
            let t = ui.get_node_mut::<ButtonNode>(thumb_idx)?;
            t.set_color(THUMB_COLOR);
            t.set_hover_color(Some(THUMB_HOVER));
            t.base.tab_stop = false;
        }
        // Scrollbar value → content scroll offset.
        ui.get_node_mut::<SliderNode>(scrollbar_idx)?.on_value_changed = Some(Box::new(move |ui: &mut Ui| {
            let value = ui.get_node::<SliderNode>(scrollbar_idx).map(|s| s.value).unwrap_or(0);
            let _ = ui.set_scroll_offset(content_idx, (value as f32, 0.0));
        }));
        // Content scroll offset → scrollbar position (via sync_scrollbar).
        if let Some(scroll) = &mut ui.get_node_mut::<PanelNode>(content_idx)?.scroll {
            scroll.scrollbar = Some(scrollbar_idx);
        }

        let frame = ui.get_node_mut::<Self>(frame_idx)?;
        frame.content_idx   = content_idx;
        frame.scrollbar_idx = scrollbar_idx;

        Ok((frame_idx, ui.get_node_mut::<Self>(frame_idx)?))
    }
}
