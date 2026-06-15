use super::{Axis, Container, NodeBase};

/// Composite scroll widget: a scroll-enabled content [`super::PanelNode`], a
/// [`super::SliderNode`] scrollbar, and decrement/increment
/// [`super::ButtonNode`]s, grouped so [`crate::Ui::resize_scroll_panel`] can
/// reposition/resize all four together.
pub struct ScrollPanelNode {
    pub base: NodeBase,
    pub container: Container,
    pub(crate) axis: Axis,
    /// Fixed at creation time; the scrollbar track + step buttons' extent
    /// along the cross axis. [`crate::Ui::resize_scroll_panel`] derives the
    /// new viewport as `base.bounds` (already updated via `base.set_size`)
    /// minus this, along `axis`.
    pub(crate) scrollbar_width: f32,
    pub content_idx: usize,
    pub scrollbar_idx: usize,
    pub dec_idx: usize,
    pub inc_idx: usize,
}

impl ScrollPanelNode {
    pub(crate) fn new(axis: Axis, scrollbar_width: f32, content_idx: usize, scrollbar_idx: usize, dec_idx: usize, inc_idx: usize) -> Self {
        Self { base: NodeBase::new(), container: Container::new(), axis, scrollbar_width, content_idx, scrollbar_idx, dec_idx, inc_idx }
    }
}
