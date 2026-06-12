use crate::types::{Rgba, Texture};

use super::PanelNode;

/// Drag gesture state: tracks whether a drag is active and the cursor
/// position / value captured when it began, so deltas can be computed
/// without accumulating drift.
#[derive(Default, Clone, Copy)]
pub struct Draggable {
    pub is_dragging:  bool,
    pub start_cursor: (f32, f32),
    pub start_value:  f32,
}

impl Draggable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, cursor: (f32, f32), value: f32) {
        self.is_dragging  = true;
        self.start_cursor = cursor;
        self.start_value  = value;
    }

    pub fn stop(&mut self) {
        self.is_dragging = false;
    }
}

/// Slider
pub struct SliderNode {
    pub panel: PanelNode,
    min_value: u32,
    max_value: u32,
    pub value: u32,
    pub step_size: u32,
    pub drag: Draggable,
    thumb_idx: Option<usize>,
    label_idx: Option<usize>,
}

impl SliderNode {
    pub fn new() -> Self {
        let mut this = Self {
            panel: PanelNode::new(),
            min_value: 0,
            max_value: 0,
            value: 0,
            step_size: 1,
            drag: Draggable::new(),
            thumb_idx: None,
            label_idx: None,
        };

        this.panel.base.set_size(200.0, 32.0);
        this.panel.set_color(Rgba { x: 0.0, y: 0.0, z: 0.0, w: 0.5 });

        this
    }

    pub fn get_label(&self) -> Option<usize> {
        self.label_idx
    }

    pub fn get_thumb(&self) -> Option<usize> {
        self.thumb_idx
    }

    pub fn set_color(&mut self, color: Rgba) { self.panel.set_color(color); }
    
    pub fn set_texture(&mut self, texture: Texture) { self.panel.set_texture(texture); }

    pub fn set_min_max(&mut self, min: u32, max: u32) {
        self.min_value = min;
        self.max_value = max;
        self.value     = self.value.clamp(min, max);
    }

    /// Clamps to `[min_value, max_value]` and snaps down to the nearest step.
    pub fn set_value(&mut self, value: u32) {
        let value = value.clamp(self.min_value, self.max_value);
        let steps = (value - self.min_value) / self.step_size;
        self.value = self.min_value + steps * self.step_size;
    }

    pub(crate) fn set_label(&mut self, idx: Option<usize>) {
        self.label_idx = idx;
    }

    pub(crate) fn set_thumb(&mut self, idx: Option<usize>) {
        self.thumb_idx = idx;
    }

    /// Fraction (0.0-1.0) of the current value along `[min_value, max_value]`.
    fn value_fraction(&self) -> f32 {
        if self.max_value > self.min_value {
            (self.value - self.min_value) as f32 / (self.max_value - self.min_value) as f32
        } else {
            0.0
        }
    }

    /// The thumb's x-offset from the track's left edge for the current value.
    pub fn thumb_offset(&self, thumb_width: f32) -> f32 {
        self.value_fraction() * (self.panel.base.bounds.width - thumb_width)
    }

    /// Value formatted and padded with spaces to the width of `max_value` so
    /// the label maintains a stable visual width across all possible values.
    /// When `right_aligned` (the label's anchor sits on its right edge), the
    /// padding goes on the left so the digits stay flush against that edge;
    /// otherwise the padding goes on the right.
    pub fn display_text(&self, right_aligned: bool) -> String {
        let width = self.max_value.to_string().len();
        if right_aligned {
            format!("{:>width$}", self.value)
        } else {
            format!("{:<width$}", self.value)
        }
    }

    /// The value implied by dragging the cursor away from where the drag started.
    pub fn value_from_drag(&self, cursor: (f32, f32), thumb_width: f32) -> u32 {
        let usable_width = (self.panel.base.bounds.width - thumb_width).max(1.0);
        let delta_value  = (cursor.0 - self.drag.start_cursor.0) / usable_width
            * (self.max_value - self.min_value) as f32;
        (self.drag.start_value + delta_value).round().clamp(self.min_value as f32, self.max_value as f32) as u32
    }

    /// The value implied by clicking directly on the track at `local_x`
    /// (relative to the track's left edge), centering the thumb on the click.
    pub fn value_from_track_position(&self, local_x: f32, thumb_width: f32) -> u32 {
        let usable_width = (self.panel.base.bounds.width - thumb_width).max(1.0);
        let fraction = ((local_x - thumb_width / 2.0) / usable_width).clamp(0.0, 1.0);
        (self.min_value as f32 + fraction * (self.max_value - self.min_value) as f32).round() as u32
    }
}

impl Default for SliderNode {
    fn default() -> Self {
        Self::new()
    }
}
