use super::NodeBase;

/// Invisible grouping node — children only, no quad rendered.
pub struct ContainerNode {
    pub base: NodeBase,
    pub children: Vec<usize>,
    /// Next [`NodeBase::z_index`] to assign to a child raised to the front;
    /// starts at `1` since `0` means "not orderable".
    pub z_sentinel: u32,
}

impl ContainerNode {
    pub fn new() -> Self {
        Self { base: NodeBase::new(), children: Vec::new(), z_sentinel: 1 }
    }
}

impl Default for ContainerNode {
    fn default() -> Self {
        Self::new()
    }
}
