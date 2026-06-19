use anyhow::Result;
use crate::{types::{Rgba, Texture}, Ui};

use super::{Anchor, Axis, Container, NodeBase, Renderable, UiNode};

/// A display-only filled track. The track quad is the node itself; the fill
/// is a single child [`PanelNode`] whose size along `axis` is kept at
/// `value × track_size`. Neither the track nor the fill are interactive.
/// Built by [`crate::Ui::create_progress_bar`].
///
/// Implements [`crate::HasAxis`]; use [`crate::Ui::set_axis`] to switch
/// orientation at runtime.
pub struct ProgressBarNode {
    pub base: NodeBase,
    pub(crate) renderable: Renderable,
    pub container: Container,
    pub axis: Axis,
    /// Index of the fill child panel; set by [`build`](Self::build).
    pub fill_idx: usize,
    value: f32,
}

impl ProgressBarNode {
    pub fn new(axis: Axis) -> Self {
        let mut base = NodeBase::new();
        base.interactive = false;
        base.tab_stop    = false;
        let mut container = Container::new();
        container.clip_children = true;
        Self {
            base,
            renderable: Renderable::default(),
            container,
            axis,
            fill_idx: 0,
            value: 0.0,
        }
    }

    pub fn build(ui: &mut Ui, parent: usize, axis: Axis, width: f32, height: f32) -> Result<(usize, &mut Self)> {
        let pb_idx = ui.add_node(UiNode::ProgressBar(Self::new(axis)), parent)?;
        ui.get_node_mut::<Self>(pb_idx)?.base.set_size(width, height);

        let anchor = Self::fill_anchor(axis, false);
        let (fill_idx, fill) = ui.create_panel(pb_idx)?;
        fill.base.interactive = false;
        fill.base.tab_stop    = false;
        match axis {
            Axis::Horizontal => {
                fill.base.set_position(anchor, 0.0, 0.0);
                fill.base.set_size(0.0, height);
            }
            Axis::Vertical => {
                fill.base.set_position(anchor, 0.0, 0.0);
                fill.base.set_size(width, 0.0);
            }
        }

        ui.get_node_mut::<Self>(pb_idx)?.fill_idx = fill_idx;
        Ok((pb_idx, ui.get_node_mut::<Self>(pb_idx)?))
    }

    /// The anchor the fill panel should use for the given `axis` and `reversed`
    /// combination. Horizontal normal: top-left (grows right). Horizontal
    /// reversed: top-right (grows left). Vertical normal: bottom-left (grows
    /// up). Vertical reversed: top-left (grows down).
    pub(crate) fn fill_anchor(axis: Axis, reversed: bool) -> Anchor {
        match (axis, reversed) {
            (Axis::Horizontal, false) => Anchor::TopLeft,
            (Axis::Horizontal, true)  => Anchor::TopRight,
            (Axis::Vertical,   false) => Anchor::BottomLeft,
            (Axis::Vertical,   true)  => Anchor::TopLeft,
        }
    }

    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) { self.base.set_position(anchor, x, y); }
    pub fn set_size(&mut self, width: f32, height: f32) { self.base.set_size(width, height); }

    pub fn value(&self) -> f32 { self.value }

    pub fn set_track_color(&mut self, color: Rgba) { self.renderable.set_color(color); self.base.mark_dirty(); }
    pub fn set_track_texture(&mut self, texture: Texture) { self.renderable.set_texture(texture); self.base.mark_dirty(); }

    /// Updates the stored value; callers use [`Ui::set_progress`] instead,
    /// which also resizes the fill panel and marks dirty.
    pub(crate) fn set_value(&mut self, value: f32) { self.value = value; }
}
