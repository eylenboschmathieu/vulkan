use crate::Ui;

use super::{Anchor, Container, NodeBase};

/// Invisible grouping node — children only, no quad rendered.
pub struct GroupNode {
    pub base: NodeBase,
    pub container: Container,
}

impl GroupNode {
    pub fn new() -> Self {
        Self { base: NodeBase::new(), container: Container::new() }
    }

    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) { self.base.set_position(anchor, x, y); }
    pub fn set_size(&mut self, width: f32, height: f32) { self.base.set_size(width, height); }
    pub fn set_visible(&mut self, visible: bool) { self.base.visible = visible; }
    pub fn set_on_show(&mut self, cb: impl FnMut(&mut Ui) + 'static) { self.base.visibility.on_show = Some(Box::new(cb)); }
    pub fn set_on_hide(&mut self, cb: impl FnMut(&mut Ui) + 'static) { self.base.visibility.on_hide = Some(Box::new(cb)); }
}

impl Default for GroupNode {
    fn default() -> Self {
        Self::new()
    }
}
