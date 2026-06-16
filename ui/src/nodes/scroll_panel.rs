use anyhow::Result;
use crate::{types::Rgba, Ui};

use super::{Axis, Container, NodeBase, SliderNode, UiNode};

/// Default scroll-wheel step, in UI pixels per wheel "line", for a scroll
/// panel's axes that aren't covered by its [`Scroll::scrollbar`] (or for
/// panels with no scrollbar at all). See [`crate::Ui::line_scroll_step`].
const DEFAULT_LINE_STEP: f32 = 48.0;

/// Gap, in UI pixels, kept between a scroll panel's scrollbar thumb and the
/// track's edges: split in half for the two ends along the track's main axis
/// (via [`super::SliderNode::set_track_padding`]), and used in full to shrink
/// the thumb's cross-axis size. See [`crate::Ui::layout_scroll_panel`].
pub const SCROLLBAR_THUMB_PADDING: f32 = 4.0;

/// Scroll state for a content [`super::PanelNode`] inside a
/// [`ScrollPanelNode`]: an offset applied to its children's resolved positions
/// (shifting content within the panel's own bounds, which remain the
/// clip/viewport rect via `clip_children`), and the total size of that content
/// for clamping the offset.
pub struct Scroll {
    pub offset: (f32, f32),
    pub content_size: (f32, f32),
    /// Index of a [`super::SliderNode`] acting as this panel's scrollbar, if
    /// any. When set, scroll-wheel input handled by
    /// [`crate::Ui::handle_input`] also updates this slider's value and
    /// thumb position to match the new offset (along the slider's own
    /// [`Axis`]). The reverse direction — dragging the scrollbar updating
    /// this panel's offset — is the host's responsibility via the slider's
    /// `on_value_changed` callback.
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

    /// The pixel distance to scroll per wheel "line", per axis. If
    /// `scrollbar` is `Some` (this scroll's [`Scroll::scrollbar`], resolved
    /// by the caller), its `step_size` applies along its own [`Axis`] — so
    /// wheel-scrolling moves it by the same amount as one click of its step
    /// buttons. The other axis (and a `None` scrollbar) fall back to
    /// [`DEFAULT_LINE_STEP`]. See [`crate::Ui::line_scroll_step`].
    pub fn line_step(scrollbar: Option<&SliderNode>) -> (f32, f32) {
        match scrollbar {
            Some(s) => match s.axis() {
                Axis::Horizontal => (s.step_size as f32, DEFAULT_LINE_STEP),
                Axis::Vertical   => (DEFAULT_LINE_STEP, s.step_size as f32),
            },
            None => (DEFAULT_LINE_STEP, DEFAULT_LINE_STEP),
        }
    }
}

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
    /// Inserts this scroll panel and all its structural children (content
    /// panel, scrollbar slider + thumb, dec/inc buttons) into the tree, wires
    /// their indices, sets default colors and callbacks, and runs the initial
    /// layout. This is the full construction logic for
    /// [`crate::Ui::create_scroll_panel`].
    pub fn build(
        ui: &mut Ui,
        parent: usize,
        axis: Axis,
        viewport: (f32, f32),
        scrollbar_width: f32,
        content_size: (f32, f32),
    ) -> Result<(usize, &mut Self)> {
        let frame_idx = ui.add_node(UiNode::ScrollPanel(Self::new(axis, scrollbar_width, 0, 0, 0, 0)), parent)?;

        let (content_idx, content) = ui.create_panel(frame_idx)?;
        content.enable_scroll(content_size);
        ui.set_clip_children(content_idx, true)?;

        let (scrollbar_idx, _) = ui.create_slider(frame_idx, axis)?;

        let (dec_idx, dec) = ui.create_button(frame_idx)?;
        dec.base.tab_stop = false;
        dec.set_color(Rgba::new(0.8, 0.8, 0.8, 0.9));
        dec.set_hover_color(Some(Rgba::new(0.3, 0.6, 1.0, 0.9)));
        dec.interaction.on_release = Some(Box::new(move |ui: &mut Ui| {
            let _ = ui.step_slider(scrollbar_idx, false);
        }));

        let (inc_idx, inc) = ui.create_button(frame_idx)?;
        inc.base.tab_stop = false;
        inc.set_color(Rgba::new(0.8, 0.8, 0.8, 0.9));
        inc.set_hover_color(Some(Rgba::new(0.3, 0.6, 1.0, 0.9)));
        inc.interaction.on_release = Some(Box::new(move |ui: &mut Ui| {
            let _ = ui.step_slider(scrollbar_idx, true);
        }));

        if let Some(scroll) = &mut ui.get_node_mut::<crate::PanelNode>(content_idx)?.scroll {
            scroll.scrollbar = Some(scrollbar_idx);
        }
        ui.get_node_mut::<SliderNode>(scrollbar_idx)?.on_value_changed = Some(Box::new(move |ui: &mut Ui| {
            let value = ui.get_node::<SliderNode>(scrollbar_idx).map(|s| s.value).unwrap_or(0);
            let offset = match axis {
                Axis::Horizontal => (value as f32, 0.0),
                Axis::Vertical   => (0.0, value as f32),
            };
            let _ = ui.set_scroll_offset(content_idx, offset);
        }));

        let frame_size = match axis {
            Axis::Vertical   => (viewport.0 + scrollbar_width, viewport.1),
            Axis::Horizontal => (viewport.0, viewport.1 + scrollbar_width),
        };
        let frame = ui.get_node_mut::<Self>(frame_idx)?;
        frame.base.set_size(frame_size.0, frame_size.1);
        frame.content_idx = content_idx;
        frame.scrollbar_idx = scrollbar_idx;
        frame.dec_idx = dec_idx;
        frame.inc_idx = inc_idx;

        ui.layout_scroll_panel(frame_idx, content_size)?;
        Ok((frame_idx, ui.get_node_mut::<Self>(frame_idx)?))
    }

    pub(crate) fn new(axis: Axis, scrollbar_width: f32, content_idx: usize, scrollbar_idx: usize, dec_idx: usize, inc_idx: usize) -> Self {
        Self { base: NodeBase::new(), container: Container::new(), axis, scrollbar_width, content_idx, scrollbar_idx, dec_idx, inc_idx }
    }
}
