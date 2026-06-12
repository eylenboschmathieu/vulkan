use super::NodeBase;

/// Invisible grouping node — children only, no quad rendered.
pub struct ContainerNode {
    pub base: NodeBase,
}

impl ContainerNode {
    pub fn new() -> Self {
        Self { base: NodeBase::new() }
    }
}

impl Default for ContainerNode {
    fn default() -> Self {
        Self::new()
    }
}
