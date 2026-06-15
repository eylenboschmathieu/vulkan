use super::{Container, NodeBase};

/// Invisible grouping node — children only, no quad rendered.
pub struct GroupNode {
    pub base: NodeBase,
    pub container: Container,
}

impl GroupNode {
    pub fn new() -> Self {
        Self { base: NodeBase::new(), container: Container::new() }
    }
}

impl Default for GroupNode {
    fn default() -> Self {
        Self::new()
    }
}
