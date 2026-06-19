use anyhow::Result;
use crate::{
    types::{Rgba, Texture},
    Ui,
};

use super::{Anchor, ButtonNode, PanelNode, UiNode};

/// Which axis a [`SliderNode`]'s value increases along: its track's width
/// (`Horizontal`) or height (`Vertical`). Drives the geometry math in
/// [`SliderNode::thumb_offset`], [`SliderNode::value_from_drag`], and the
/// thumb positioning in [`crate::Ui::layout_slider`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Drag gesture state: the cursor position / value captured when a drag
/// began, so deltas can be computed without accumulating drift. Whether a
/// drag is active at all is tracked by `Ui::dragging_node`, not here.
#[derive(Default, Clone, Copy)]
pub struct Draggable {
    pub start_cursor: (f32, f32),
    pub start_value:  f32,
}

impl Draggable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, cursor: (f32, f32), value: f32) {
        self.start_cursor = cursor;
        self.start_value  = value;
    }
}

/// A draggable slider with a track panel and a thumb button.
///
/// Implements [`crate::HasAxis`]; use [`crate::Ui::set_axis`] to switch
/// orientation at runtime (caller updates `panel.base` bounds first).
pub struct SliderNode {
    pub panel: PanelNode,
    pub(crate) axis: Axis,
    /// When `true`, the slider's value increases toward the track's start:
    /// right-to-left for [`Axis::Horizontal`], top-to-bottom for
    /// [`Axis::Vertical`]. Default `false` (left-to-right / top-to-bottom
    /// increase). Set this for fader-style controls where max should be at
    /// the top or right.
    pub reversed: bool,
    min_value: u32,
    max_value: u32,
    pub value: u32,
    pub step_size: u32,
    pub drag: Draggable,
    thumb_idx: Option<usize>,
    /// Inset, in UI pixels, kept clear at each end of the track — the thumb's
    /// travel range is `[track_padding, main_extent - track_padding]` rather
    /// than the full track. `0.0` by default; set via
    /// [`SliderNode::set_track_padding`].
    track_padding: f32,
    /// Fired by [`crate::Ui::handle_input`] when a drag or track click
    /// changes [`SliderNode::value`]. Not fired by programmatic
    /// [`SliderNode::set_value`] calls — hosts that change the value from
    /// their own code already know the new value and can update their own UI
    /// directly.
    pub on_value_changed: Option<Box<dyn FnMut(&mut Ui)>>,
}

impl SliderNode {
    /// Positions the thumb button at the offset implied by the slider's current
    /// value and marks it dirty. Called after any value change.
    pub fn layout(ui: &mut Ui, slider_idx: usize) -> Result<()> {
        let (thumb_idx, axis) = {
            let s = ui.get_node::<Self>(slider_idx)?;
            (s.get_thumb(), s.axis())
        };

        if let Some(thumb_idx) = thumb_idx {
            let thumb_extent = {
                let thumb = ui.get_node::<ButtonNode>(thumb_idx)?;
                match axis {
                    Axis::Horizontal => thumb.base.bounds.width,
                    Axis::Vertical   => thumb.base.bounds.height,
                }
            };
            let offset = ui.get_node::<Self>(slider_idx)?.thumb_offset(thumb_extent);
            let thumb  = ui.get_node_mut::<ButtonNode>(thumb_idx)?;
            match axis {
                Axis::Horizontal => thumb.base.set_position(Anchor::Left, offset, 0.0),
                Axis::Vertical   => thumb.base.set_position(Anchor::Top,  0.0,    offset),
            }
            ui.mark_dirty(thumb_idx);
        }

        Ok(())
    }

    /// Recomputes the slider's value from the cursor delta since the drag
    /// began, re-lays-out the thumb, and fires `on_value_changed` if the
    /// value changed. Called from [`crate::Ui::handle_input`] during a drag.
    pub fn apply_drag(ui: &mut Ui, slider_idx: usize, cursor: (f32, f32)) -> Result<()> {
        let (thumb_idx, axis) = {
            let s = ui.get_node::<Self>(slider_idx)?;
            (s.get_thumb(), s.axis())
        };
        let thumb_extent = match thumb_idx {
            Some(idx) => {
                let thumb = ui.get_node::<ButtonNode>(idx)?;
                match axis {
                    Axis::Horizontal => thumb.base.bounds.width,
                    Axis::Vertical   => thumb.base.bounds.height,
                }
            }
            None => 0.0,
        };

        let changed = ui.get_node_mut::<Self>(slider_idx)?.drag_to(cursor, thumb_extent);
        Self::layout(ui, slider_idx)?;

        if changed {
            ui.fire_slider_value_changed(slider_idx)?;
        }

        Ok(())
    }

    /// Inserts this slider and its default thumb [`ButtonNode`] into the tree
    /// under `parent`, configures the thumb's size and colors, and wires the
    /// thumb index back. This is the full construction logic for
    /// [`crate::Ui::create_slider`].
    pub fn build(ui: &mut Ui, parent: usize, axis: Axis) -> Result<(usize, &mut Self)> {
        let slider_idx = ui.add_node(UiNode::Slider(Self::new(axis)), parent)?;

        let (thumb_idx, thumb) = ui.create_button(slider_idx)?;
        thumb.base.tab_stop = false;
        match axis {
            Axis::Horizontal => thumb.base.set_size(16.0, 32.0),
            Axis::Vertical   => thumb.base.set_size(32.0, 16.0),
        }
        thumb.set_color(Rgba::new(0.8, 0.8, 0.8, 0.9));
        thumb.set_hover_color(Some(Rgba::new(0.3, 0.6, 1.0, 0.9)));

        let s = ui.get_node_mut::<Self>(slider_idx)?;
        s.set_thumb(Some(thumb_idx));
        Ok((slider_idx, s))
    }

    pub fn new(axis: Axis) -> Self {
        let mut this = Self {
            panel: PanelNode::new(),
            axis,
            reversed: false,
            min_value: 0,
            max_value: 0,
            value: 0,
            step_size: 1,
            drag: Draggable::new(),
            thumb_idx: None,
            track_padding: 0.0,
            on_value_changed: None,
        };

        match axis {
            Axis::Horizontal => this.panel.base.set_size(200.0, 32.0),
            Axis::Vertical   => this.panel.base.set_size(32.0, 200.0),
        }
        this.panel.set_color(Rgba { x: 0.0, y: 0.0, z: 0.0, w: 0.5 });

        this
    }

    pub fn axis(&self) -> Axis {
        self.axis
    }

    pub fn min_value(&self) -> u32 { self.min_value }
    pub fn max_value(&self) -> u32 { self.max_value }

    pub fn get_thumb(&self) -> Option<usize> {
        self.thumb_idx
    }

    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) { self.panel.base.set_position(anchor, x, y); }
    pub fn set_size(&mut self, width: f32, height: f32) { self.panel.base.set_size(width, height); }

    pub fn set_color(&mut self, color: Rgba) { self.panel.set_color(color); }

    pub fn set_texture(&mut self, texture: Texture) { self.panel.set_texture(texture); }

    pub fn set_min_max(&mut self, min: u32, max: u32) {
        self.min_value = min;
        self.max_value = max;
        self.value     = self.value.clamp(min, max);
    }

    /// Clamps to `[min_value, max_value]` and snaps down to the nearest step,
    /// except `min_value`/`max_value` themselves are kept exactly even if
    /// they're not on the step grid — otherwise clamping to `max_value` would
    /// snap back down to the previous step, making the true endpoints
    /// unreachable via [`crate::Ui::step_slider`].
    pub fn set_value(&mut self, value: u32) {
        let value = value.clamp(self.min_value, self.max_value);
        if value == self.min_value || value == self.max_value {
            self.value = value;
            return;
        }
        let steps = (value - self.min_value) / self.step_size;
        self.value = self.min_value + steps * self.step_size;
    }

    /// Adjusts the value by one [`SliderNode::step_size`] — up if `increase`,
    /// down otherwise — clamping as [`SliderNode::set_value`]. Returns
    /// whether the value actually changed. See [`crate::Ui::step_slider`].
    pub fn step(&mut self, increase: bool) -> bool {
        let old = self.value;
        let new = if increase {
            old.saturating_add(self.step_size)
        } else {
            old.saturating_sub(self.step_size)
        };
        self.set_value(new);
        self.value != old
    }

    /// Recomputes the value from the cursor position relative to where the
    /// drag started (see [`SliderNode::value_from_drag`]) and applies it.
    /// Returns whether the value actually changed. See
    /// [`crate::Ui::drag_slider`].
    pub fn drag_to(&mut self, cursor: (f32, f32), thumb_extent: f32) -> bool {
        let old = self.value;
        self.set_value(self.value_from_drag(cursor, thumb_extent));
        self.value != old
    }

    pub(crate) fn set_thumb(&mut self, idx: Option<usize>) {
        self.thumb_idx = idx;
    }

    pub(crate) fn set_track_padding(&mut self, padding: f32) {
        self.track_padding = padding;
    }

    /// Fraction (0.0-1.0) of the current value along `[min_value, max_value]`.
    fn value_fraction(&self) -> f32 {
        if self.max_value > self.min_value {
            (self.value - self.min_value) as f32 / (self.max_value - self.min_value) as f32
        } else {
            0.0
        }
    }

    /// The track's extent along [`SliderNode::axis`] (width if `Horizontal`,
    /// height if `Vertical`) available for the thumb's travel, after
    /// [`SliderNode::track_padding`] is kept clear at each end.
    fn main_extent(&self) -> f32 {
        let full = match self.axis {
            Axis::Horizontal => self.panel.base.bounds.width,
            Axis::Vertical   => self.panel.base.bounds.height,
        };
        (full - 2.0 * self.track_padding).max(0.0)
    }

    /// The component of `cursor` along [`SliderNode::axis`] (x if
    /// `Horizontal`, y if `Vertical`).
    fn cursor_main(&self, cursor: (f32, f32)) -> f32 {
        match self.axis {
            Axis::Horizontal => cursor.0,
            Axis::Vertical   => cursor.1,
        }
    }

    /// The thumb's offset from the track's start edge (left/top) for the
    /// current value, along [`SliderNode::axis`].
    pub fn thumb_offset(&self, thumb_extent: f32) -> f32 {
        let frac = if self.reversed { 1.0 - self.value_fraction() } else { self.value_fraction() };
        self.track_padding + frac * (self.main_extent() - thumb_extent)
    }

    /// The value implied by dragging the cursor away from where the drag started.
    pub fn value_from_drag(&self, cursor: (f32, f32), thumb_extent: f32) -> u32 {
        let usable_extent = (self.main_extent() - thumb_extent).max(1.0);
        let raw_delta     = self.cursor_main(cursor) - self.cursor_main(self.drag.start_cursor);
        let signed_delta  = if self.reversed { -raw_delta } else { raw_delta };
        let delta_value = signed_delta / usable_extent * (self.max_value - self.min_value) as f32;
        (self.drag.start_value + delta_value).round().clamp(self.min_value as f32, self.max_value as f32) as u32
    }

    /// The value implied by clicking directly on the track at `local_pos`
    /// (relative to the track's start edge, along [`SliderNode::axis`]),
    /// centering the thumb on the click.
    pub fn value_from_track_position(&self, local_pos: f32, thumb_extent: f32) -> u32 {
        let usable_extent = (self.main_extent() - thumb_extent).max(1.0);
        let fraction = ((local_pos - self.track_padding - thumb_extent / 2.0) / usable_extent).clamp(0.0, 1.0);
        let fraction = if self.reversed { 1.0 - fraction } else { fraction };
        (self.min_value as f32 + fraction * (self.max_value - self.min_value) as f32).round() as u32
    }
}

impl Default for SliderNode {
    fn default() -> Self {
        Self::new(Axis::Horizontal)
    }
}

