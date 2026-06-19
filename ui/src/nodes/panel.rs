use crate::types::{Rgba, Texture};

use super::{Container, NodeBase, Renderable, Scroll};

/// Visible background panel. Labelable.
pub struct PanelNode {
    pub base: NodeBase,
    pub(crate) renderable: Renderable,
    pub container: Container,
    /// `Some` if this panel acts as a scroll viewport for its children; see
    /// [`Scroll`]. `None` by default.
    pub scroll: Option<Scroll>,
}

impl PanelNode {
    pub fn new() -> Self {
        Self {
            base: NodeBase::new(),
            renderable: Renderable::default(),
            container: Container::new(),
            scroll: None,
        }
    }

    pub fn set_color(&mut self, color: Rgba) {
        self.renderable.set_color(color);
        self.base.mark_dirty();
    }

    pub fn set_texture(&mut self, texture: Texture) {
        self.renderable.set_texture(texture);
        self.base.mark_dirty();
    }

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
