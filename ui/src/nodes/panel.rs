use crate::types::{Rgba, Texture};

use super::{Container, NodeBase};

/// Scroll state for a [`PanelNode`] acting as a scroll panel: an offset
/// applied to its children's resolved positions (shifting content within
/// the panel's own bounds, which remain the clip/viewport rect via
/// `clip_children`), and the total size of that content for clamping the
/// offset.
pub struct Scroll {
    pub offset: (f32, f32),
    pub content_size: (f32, f32),
    /// Index of a [`super::SliderNode`] acting as this panel's scrollbar, if
    /// any. When set, scroll-wheel input handled by
    /// [`crate::Ui::handle_input`] also updates this slider's value and
    /// thumb position to match the new offset (along the slider's own
    /// [`super::Axis`]). The reverse direction - dragging the scrollbar
    /// updating this panel's offset - is the host's responsibility via the
    /// slider's `on_value_changed` callback.
    pub scrollbar: Option<usize>,
}

impl Scroll {
    pub fn new(content_size: (f32, f32)) -> Self {
        Self { offset: (0.0, 0.0), content_size, scrollbar: None }
    }

    /// Maximum offset per axis before content's trailing edge would pass
    /// the viewport's trailing edge.
    pub fn max_offset(&self, viewport: (f32, f32)) -> (f32, f32) {
        ((self.content_size.0 - viewport.0).max(0.0), (self.content_size.1 - viewport.1).max(0.0))
    }

    /// Sets `offset`, clamped to `[0, max_offset(viewport)]` per axis.
    pub fn set_offset(&mut self, offset: (f32, f32), viewport: (f32, f32)) {
        let max = self.max_offset(viewport);
        self.offset = (offset.0.clamp(0.0, max.0), offset.1.clamp(0.0, max.1));
    }
}

/// Visible background panel. Labelable.
pub struct PanelNode {
    pub base: NodeBase,
    pub(crate) color: Rgba,
    pub(crate) texture: Texture,
    pub container: Container,
    /// `Some` if this panel acts as a scroll viewport for its children; see
    /// [`Scroll`]. `None` by default.
    pub scroll: Option<Scroll>,
}

impl PanelNode {
    pub fn new() -> Self {
        Self {
            base: NodeBase::new(),
            color: Rgba::new(0.0, 0.0, 0.0, 0.0),
            texture: Texture::default(),
            container: Container::new(),
            scroll: None,
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
    pub fn set_texture(&mut self, texture: Texture) { self.texture = texture; }

    /// Enables scrolling with the given total content size. This panel's
    /// own `base.bounds` size becomes the scroll viewport. Typically paired
    /// with [`crate::Ui::set_clip_children`] so shifted content is actually
    /// clipped to the viewport. Doesn't itself change any resolved position
    /// (offset starts at `(0, 0)`), so no follow-up `Ui` call is needed.
    pub fn enable_scroll(&mut self, content_size: (f32, f32)) {
        self.scroll = Some(Scroll::new(content_size));
    }

    /// Sets the scroll offset, clamped to `[0, max_offset]` against this
    /// panel's own bounds as the viewport. No-op if scrolling isn't enabled.
    /// Changes every descendant's resolved position, so callers must follow
    /// up with [`crate::Ui::mark_dirty`] — not accessible outside the crate
    /// for that reason; hosts call [`crate::Ui::set_scroll_offset`] instead,
    /// which does this for them.
    pub(crate) fn set_scroll_offset(&mut self, offset: (f32, f32)) {
        let viewport = (self.base.bounds.width, self.base.bounds.height);
        if let Some(scroll) = &mut self.scroll {
            scroll.set_offset(offset, viewport);
        }
    }

    /// Adjusts the scroll offset by `(dx, dy)`, clamped. No-op if scrolling
    /// isn't enabled. See [`PanelNode::set_scroll_offset`] re: visibility and
    /// [`crate::Ui::scroll_by`].
    pub(crate) fn scroll_by(&mut self, delta: (f32, f32)) {
        if let Some(scroll) = &self.scroll {
            let new = (scroll.offset.0 + delta.0, scroll.offset.1 + delta.1);
            self.set_scroll_offset(new);
        }
    }

    /// Updates the content size for a scroll-enabled panel, re-clamping the
    /// current offset against this panel's bounds as the viewport. No-op if
    /// scrolling isn't enabled. Used by
    /// [`crate::Ui::resize_scroll_panel`] after the panel's own bounds have
    /// already been updated to the new viewport.
    pub(crate) fn set_content_size(&mut self, content_size: (f32, f32)) {
        let viewport = (self.base.bounds.width, self.base.bounds.height);
        if let Some(scroll) = &mut self.scroll {
            scroll.content_size = content_size;
            scroll.set_offset(scroll.offset, viewport);
        }
    }
}

impl Default for PanelNode {
    fn default() -> Self {
        Self::new()
    }
}
