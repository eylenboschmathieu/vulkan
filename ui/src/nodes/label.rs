use crate::types::Rgba;

use super::NodeBase;

/// Text label. Not interactive, not labelable itself.
pub struct LabelNode {
    pub base: NodeBase,
    pub text: String,
    pub(crate) color: Rgba,
    /// Longest `text` has ever been, in chars. Rendering always reserves this
    /// many quads (padding unused slots with degenerate ones), so the label's
    /// vertex allocation stays constant across [`crate::Ui::flush_dirty`]
    /// updates and only needs to grow — never shrink — via
    /// [`crate::Ui::flush_all`].
    max_len: usize,
}

impl LabelNode {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let max_len = text.chars().count();
        Self {
            base: NodeBase::new(),
            text,
            color: Rgba::new(0.0, 0.0, 0.0, 1.0),
            max_len,
        }
    }

    pub fn max_len(&self) -> usize {
        self.max_len
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }

    /// Replaces the text, growing `max_len` if it's now the longest this
    /// label has ever held. Returns `true` when `max_len` grows — the caller
    /// must rebuild the whole tree (`Ui::dirty = true`) so
    /// [`crate::Ui::flush_all`] reserves the larger allocation; otherwise an
    /// in-place [`crate::Ui::flush_dirty`] update is enough.
    pub fn set_text(&mut self, text: impl Into<String>) -> bool {
        self.text = text.into();
        let len = self.text.chars().count();
        if len > self.max_len {
            self.max_len = len;
            true
        } else {
            false
        }
    }
}
