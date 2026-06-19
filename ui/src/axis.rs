use anyhow::Result;

use crate::{Axis, PanelNode, ProgressBarNode, SliderNode, Ui};
use crate::nodes::UiNodeVariant;

/// Implemented by node types that have a horizontal/vertical orientation.
/// [`Ui::set_axis`] calls [`set_axis_in_place`](Self::set_axis_in_place) on
/// the node, then [`sync_after`](Self::sync_after) to propagate the change
/// through the UI (fill recalculation, thumb re-layout, dirty marking).
///
/// The caller is responsible for updating the node's bounds (size and position)
/// before calling [`Ui::set_axis`]. The method only updates the axis and
/// recalculates dependent child geometry from the current bounds and anchors —
/// it does not swap width and height automatically.
pub trait HasAxis: UiNodeVariant {
    /// Update the node's stored axis. Does not modify bounds.
    fn set_axis_in_place(&mut self, axis: Axis);

    /// Called after the axis has been updated; handles any secondary work that
    /// requires `&mut Ui`. Position is re-derived from the current anchors and
    /// bounds rather than hard-coded, so the layout is correct for any anchor
    /// the caller has configured.
    fn sync_after(ui: &mut Ui, idx: usize) -> Result<()>;
}

impl HasAxis for ProgressBarNode {
    fn set_axis_in_place(&mut self, axis: Axis) {
        self.axis = axis;
    }

    fn sync_after(ui: &mut Ui, idx: usize) -> Result<()> {
        let pb = ui.get_node_mut::<ProgressBarNode>(idx)?;
        let value = pb.value();
        let (fill_idx, axis, w, h, visible) =
            (pb.fill_idx, pb.axis, pb.base.bounds.width, pb.base.bounds.height, pb.base.visible);
        let anchor = ProgressBarNode::fill_anchor(axis, false);

        let fill = ui.get_node_mut::<PanelNode>(fill_idx)?;
        match axis {
            Axis::Horizontal => {
                fill.base.set_position(anchor, 0.0, 0.0);
                fill.base.set_size(w * value, h);
            }
            Axis::Vertical => {
                fill.base.set_position(anchor, 0.0, 0.0);
                fill.base.set_size(w, h * value);
            }
        }
        if visible {
            ui.mark_dirty(idx);
        }
        Ok(())
    }
}

impl HasAxis for SliderNode {
    fn set_axis_in_place(&mut self, axis: Axis) {
        self.axis = axis;
    }

    fn sync_after(ui: &mut Ui, idx: usize) -> Result<()> {
        ui.layout_slider(idx)
    }
}
