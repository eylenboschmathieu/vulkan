use anyhow::Result;

use crate::{Axis, PanelNode, ProgressBarNode, Ui};
use crate::nodes::UiNodeVariant;

/// Implemented by node types that have a horizontal/vertical orientation.
/// [`Ui::flip`] calls [`flip_axis_in_place`](Self::flip_axis_in_place) on the
/// node, then [`sync_after_flip`](Self::sync_after_flip) to propagate the
/// change through the UI (recalculate children, re-layout thumb, mark dirty).
pub trait FlipAxis: UiNodeVariant {
    /// Toggle the node's axis and swap width ↔ height in place.
    fn flip_axis_in_place(&mut self);

    /// Called after the axis and bounds have been swapped; handles any
    /// secondary work that requires `&mut Ui` (fill recalculation, thumb
    /// re-layout, dirty marking).
    fn sync_after_flip(ui: &mut Ui, idx: usize) -> Result<()>;
}

impl FlipAxis for ProgressBarNode {
    fn flip_axis_in_place(&mut self) {
        self.axis = match self.axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical   => Axis::Horizontal,
        };
        std::mem::swap(&mut self.base.bounds.width, &mut self.base.bounds.height);
    }

    fn sync_after_flip(ui: &mut Ui, idx: usize) -> Result<()> {
        let pb = ui.get_node_mut::<ProgressBarNode>(idx)?;
        let value = pb.value();
        let (fill_idx, axis, w, h, visible) =
            (pb.fill_idx, pb.axis, pb.base.bounds.width, pb.base.bounds.height, pb.base.visible);
        let fill = ui.get_node_mut::<PanelNode>(fill_idx)?;
        match axis {
            Axis::Horizontal => {
                fill.base.bounds.x      = 0.0;
                fill.base.bounds.y      = 0.0;
                fill.base.bounds.width  = w * value;
                fill.base.bounds.height = h;
            }
            Axis::Vertical => {
                let fill_h = h * value;
                fill.base.bounds.x      = 0.0;
                fill.base.bounds.y      = h - fill_h;
                fill.base.bounds.width  = w;
                fill.base.bounds.height = fill_h;
            }
        }
        if visible {
            ui.mark_dirty(idx);
        }
        Ok(())
    }
}
