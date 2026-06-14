mod button;
mod checkbox;
mod container;
mod label;
mod panel;
mod slider;
mod window;

use crate::{Edges, Rect, Ui};

pub use button::ButtonNode;
pub use checkbox::CheckboxNode;
pub use container::ContainerNode;
pub use label::LabelNode;
pub use panel::PanelNode;
pub use slider::SliderNode;
pub use window::{WindowNode, TITLEBAR_HEIGHT, WINDOW_BORDER};

// ── Layout primitives ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,    Top,    TopRight,
    Left,       Center, Right,
    BottomLeft, Bottom, BottomRight,
}

impl Anchor {
    /// Returns the (x, y) fractional position of this anchor within a rect.
    /// e.g. TopLeft = (0, 0), Center = (0.5, 0.5), BottomRight = (1, 1).
    fn fractions(self) -> (f32, f32) {
        match self {
            Anchor::TopLeft     => (0.0, 0.0),
            Anchor::Top         => (0.5, 0.0),
            Anchor::TopRight    => (1.0, 0.0),
            Anchor::Left        => (0.0, 0.5),
            Anchor::Center      => (0.5, 0.5),
            Anchor::Right       => (1.0, 0.5),
            Anchor::BottomLeft  => (0.0, 1.0),
            Anchor::Bottom      => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }

    /// True for anchors attached to the rect's right edge (`Right`,
    /// `TopRight`, `BottomRight`) — used to decide which side of a label's
    /// text should be padding so its content stays flush against the
    /// anchored edge.
    pub fn is_right(self) -> bool {
        matches!(self, Anchor::TopRight | Anchor::Right | Anchor::BottomRight)
    }
}

/// Defines how a node is positioned relative to a target node (or its parent,
/// if `target` is `None`).
pub struct Anchoring {
    /// Attachment point on this node.
    pub src: Anchor,
    /// Node to anchor to; `None` means the parent.
    pub target:     Option<usize>,
    /// Attachment point on the target node.
    pub dst: Anchor,
}

impl Anchoring {
    fn new() -> Self {
        Self { src: Anchor::TopLeft, target: None, dst: Anchor::TopLeft }
    }
}

/// Callbacks fired as a node becomes or stops being the visible menu screen.
#[derive(Default)]
pub struct VisibilityCb {
    /// Fired right after this node becomes the visible menu screen.
    pub on_show: Option<Box<dyn FnMut(&mut Ui)>>,
    /// Fired right before this node stops being the visible menu screen.
    pub on_hide: Option<Box<dyn FnMut(&mut Ui)>>,
}

/// Holds the four interaction callbacks shared by any interactive node type.
/// Fired by [`Ui::handle_input`] in response to user input, after any
/// built-in behavior for the node (e.g. a checkbox's selected toggle) has
/// already been applied.
#[derive(Default)]
pub struct InteractionCb {
    pub on_pressed: Option<Box<dyn FnMut(&mut Ui)>>,
    pub on_release: Option<Box<dyn FnMut(&mut Ui)>>,
    pub on_enter:   Option<Box<dyn FnMut(&mut Ui)>>,
    pub on_leave:   Option<Box<dyn FnMut(&mut Ui)>>,
}

pub struct NodeBase {
    pub bounds:        Rect,
    pub anchoring:     Anchoring,
    pub parent:        Option<usize>,
    pub visible:       bool,
    pub vertex_offset: usize,
    pub visibility:    VisibilityCb,
    /// This node's position among its parent's children, low to high
    /// (painter's algorithm: higher renders later/on top, and is hit-tested
    /// first). `0` means "not orderable" — such nodes sort below any sibling
    /// with `z_index >= 1` via a stable sort, so ties fall back to insertion
    /// order (today's behavior). Bumped by [`Ui`](crate::Ui)'s raise-on-press
    /// via the parent's `z_sentinel`.
    pub z_index: u32,
    /// For children of the root node only: which "layer" (in the host's own
    /// ordering, see [`Ui::register_layer`](crate::Ui::register_layer)) this
    /// node belongs to. Sorted before `z_index`, so it partitions root's
    /// children into independently-ordered bands (e.g. normal content vs. a
    /// debug overlay that should always render on top). Unused below the
    /// root.
    pub band: u32,
    /// When `true`, this node's children (and their whole subtrees) are
    /// clipped to this node's resolved bounds, intersected with any clip rect
    /// inherited from further up the tree. `false` by default. Set via
    /// [`Ui::set_clip_children`](crate::Ui::set_clip_children).
    pub clip_children: bool,
    /// When `true`, dragging this node (currently only meaningful for a
    /// draggable [`WindowNode`]) clamps its position so its resolved edges
    /// stay within its parent's resolved edges. `false` by default. Set via
    /// [`Ui::set_clamp_to_parent`](crate::Ui::set_clamp_to_parent).
    pub clamp_to_parent: bool,
}

impl NodeBase {
    pub fn new() -> Self {
        Self {
            bounds:        Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            anchoring:     Anchoring::new(),
            parent:        None,
            visible:       true,
            vertex_offset: 0,
            visibility:    VisibilityCb::default(),
            z_index:       0,
            band:          0,
            clip_children: false,
            clamp_to_parent: false,
        }
    }

    /// Positions the node symmetrically relative to its parent: both the
    /// attachment point on this node and the reference point on the parent
    /// use the same anchor, so e.g. `Center + (0, 0)` truly centres the node.
    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) {
        self.anchoring.src = anchor;
        self.anchoring.dst = anchor;
        self.anchoring.target     = None;
        self.bounds.x             = x;
        self.bounds.y             = y;
    }

    /// Positions the node relative to an arbitrary sibling or ancestor.
    /// `src_anchor` is the attachment point on this node; `dst_anchor` is the
    /// reference point on the target node.
    pub fn set_position_anchored_to(&mut self, src_anchor: Anchor, target: usize, dst_anchor: Anchor, x: f32, y: f32) {
        self.anchoring.src = src_anchor;
        self.anchoring.target     = Some(target);
        self.anchoring.dst = dst_anchor;
        self.bounds.x             = x;
        self.bounds.y             = y;
    }

    pub fn set_width(&mut self, width: f32) {
        self.bounds.width = width;
    }

    pub fn set_height(&mut self, height: f32) {
        self.bounds.height = height;
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        self.bounds.width  = width;
        self.bounds.height = height;
    }

    /// Computes the node's screen-space [`Edges`].
    /// When `target` is `None` the node is positioned relative to `parent_edges`.
    /// When `target` is `Some(idx)` the node is positioned relative to that
    /// node's absolute edges, computed by walking its parent chain.
    pub fn resolve(&self, parent_edges: &Edges, nodes: &[UiNode]) -> Edges {
        let ref_edges = match self.anchoring.target {
            None      => *parent_edges,
            Some(idx) => node_absolute_edges(idx, nodes),
        };
        let (px, py) = self.anchoring.src.fractions();
        let (tx, ty) = self.anchoring.dst.fractions();
        let ref_w    = ref_edges.right  - ref_edges.left;
        let ref_h    = ref_edges.bottom - ref_edges.top;
        let ref_x    = ref_edges.left + tx * ref_w  + self.bounds.x;
        let ref_y    = ref_edges.top  + ty * ref_h  + self.bounds.y;
        let left     = ref_x - px * self.bounds.width;
        let top      = ref_y - py * self.bounds.height;
        Edges { left, right: left + self.bounds.width, top, bottom: top + self.bounds.height }
    }
}

/// Computes the absolute screen-space edges of `idx` by walking its parent chain.
fn node_absolute_edges(idx: usize, nodes: &[UiNode]) -> Edges {
    let node = &nodes[idx];
    let parent_edges = match node.base().parent {
        None    => Edges { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 },
        Some(p) => node_absolute_edges(p, nodes),
    };
    node.base().resolve(&parent_edges, nodes)
}

// ── UiNode enum ──────────────────────────────────────────────────────────────

pub enum UiNode {
    Container(ContainerNode),
    Panel(PanelNode),
    Button(ButtonNode),
    Checkbox(CheckboxNode),
    Label(LabelNode),
    Slider(SliderNode),
    Window(WindowNode),
}

impl UiNode {
    pub fn base(&self) -> &NodeBase {
        match self {
            UiNode::Container(n) => &n.base,
            UiNode::Panel(n)         => &n.base,
            UiNode::Button(n)       => &n.base,
            UiNode::Checkbox(n)   => &n.base,
            UiNode::Label(n)         => &n.base,
            UiNode::Slider(n)       => &n.panel.base,
            UiNode::Window(n)       => &n.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut NodeBase {
        match self {
            UiNode::Container(n) => &mut n.base,
            UiNode::Panel(n)         => &mut n.base,
            UiNode::Button(n)       => &mut n.base,
            UiNode::Checkbox(n)   => &mut n.base,
            UiNode::Label(n)         => &mut n.base,
            UiNode::Slider(n)       => &mut n.panel.base,
            UiNode::Window(n)       => &mut n.base,
        }
    }

    /// Child indices, for container-like node types (`Container`, `Panel`,
    /// `Button`, `Slider`, `Window`). `None` for leaf node types, which
    /// cannot have children.
    pub fn children(&self) -> Option<&[usize]> {
        match self {
            UiNode::Container(n) => Some(&n.children),
            UiNode::Panel(n)           => Some(&n.children),
            UiNode::Button(n)         => Some(&n.children),
            UiNode::Slider(n)         => Some(&n.panel.children),
            UiNode::Window(n)         => Some(&n.children),
            UiNode::Checkbox(_) | UiNode::Label(_) => None,
        }
    }

    /// Mutable child indices; see [`UiNode::children`].
    pub fn children_mut(&mut self) -> Option<&mut Vec<usize>> {
        match self {
            UiNode::Container(n) => Some(&mut n.children),
            UiNode::Panel(n)         => Some(&mut n.children),
            UiNode::Button(n)       => Some(&mut n.children),
            UiNode::Slider(n)       => Some(&mut n.panel.children),
            UiNode::Window(n)       => Some(&mut n.children),
            UiNode::Checkbox(_) | UiNode::Label(_)   => None,
        }
    }

    /// The per-parent monotonic counter used to assign `z_index` to
    /// orderable children (see [`NodeBase::z_index`]); `None` for leaf node
    /// types, which cannot have children.
    pub fn z_sentinel_mut(&mut self) -> Option<&mut u32> {
        match self {
            UiNode::Container(n) => Some(&mut n.z_sentinel),
            UiNode::Panel(n)         => Some(&mut n.z_sentinel),
            UiNode::Button(n)       => Some(&mut n.z_sentinel),
            UiNode::Slider(n)       => Some(&mut n.panel.z_sentinel),
            UiNode::Window(n)       => Some(&mut n.z_sentinel),
            UiNode::Checkbox(_) | UiNode::Label(_)   => None,
        }
    }
}

/// Maps a concrete node type to its [`UiNode`] variant, so [`super::UiTree::get_node`]
/// / [`super::UiTree::get_node_mut`] can extract it generically and report a useful
/// error instead of panicking when the index is invalid or holds another type.
pub trait UiNodeVariant: Sized {
    const NAME: &'static str;
    fn from_node(node: &UiNode) -> Option<&Self>;
    fn from_node_mut(node: &mut UiNode) -> Option<&mut Self>;
}

macro_rules! ui_node_variant {
    ($ty:ty, $variant:ident) => {
        impl UiNodeVariant for $ty {
            const NAME: &'static str = stringify!($variant);

            fn from_node(node: &UiNode) -> Option<&Self> {
                match node { UiNode::$variant(n) => Some(n), _ => None }
            }

            fn from_node_mut(node: &mut UiNode) -> Option<&mut Self> {
                match node { UiNode::$variant(n) => Some(n), _ => None }
            }
        }
    };
}

ui_node_variant!(ContainerNode, Container);
ui_node_variant!(PanelNode,     Panel);
ui_node_variant!(ButtonNode,    Button);
ui_node_variant!(CheckboxNode,  Checkbox);
ui_node_variant!(LabelNode,     Label);
ui_node_variant!(SliderNode,    Slider);
ui_node_variant!(WindowNode,    Window);
