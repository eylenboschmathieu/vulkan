/// Shared state for container-like nodes that group children and may clip or
/// clamp them: [`super::GroupNode`], [`super::PanelNode`],
/// [`super::ScrollPanelNode`], [`super::WindowNode`].
pub struct Container {
    pub children: Vec<usize>,
    /// Next [`super::NodeBase::z_index`] to assign to a child raised to the
    /// front; starts at `1` since `0` means "not orderable".
    pub z_sentinel: u32,
    /// See [`crate::UiNode::clip_children`]. `false` by default.
    pub clip_children: bool,
    /// See [`crate::UiNode::clamp_children`]. `false` by default.
    pub clamp_children: bool,
}

impl Container {
    pub fn new() -> Self {
        Self { children: Vec::new(), z_sentinel: 1, clip_children: false, clamp_children: false }
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}
