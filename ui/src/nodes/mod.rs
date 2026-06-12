mod button;
mod checkbox;
mod container;
mod label;
mod panel;
mod slider;

use crate::{Edges, Rect, Ui};

pub use button::ButtonNode;
pub use checkbox::CheckboxNode;
pub use container::ContainerNode;
pub use label::LabelNode;
pub use panel::PanelNode;
pub use slider::SliderNode;

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
    pub bounds:      Rect,
    pub src_anchor:  Anchor,        // Attachment point on this node
    pub target:      Option<usize>, // Node to anchor to; None = parent
    pub dst_anchor:  Anchor,        // Attachment point on the target node
    pub parent:        Option<usize>,
    pub children:      Vec<usize>,
    pub visible:       bool,
    pub vertex_offset: usize,
    pub visibility:    VisibilityCb,
}

impl NodeBase {
    pub fn new() -> Self {
        Self {
            bounds:      Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            src_anchor:  Anchor::TopLeft,
            target:      None,
            dst_anchor:  Anchor::TopLeft,
            parent:      None,
            children:    Vec::new(),
            visible:       true,
            vertex_offset: 0,
            visibility:    VisibilityCb::default(),
        }
    }

    /// Positions the node symmetrically relative to its parent: both the
    /// attachment point on this node and the reference point on the parent
    /// use the same anchor, so e.g. `Center + (0, 0)` truly centres the node.
    pub fn set_position(&mut self, anchor: Anchor, x: f32, y: f32) {
        self.src_anchor         = anchor;
        self.dst_anchor = anchor;
        self.target        = None;
        self.bounds.x      = x;
        self.bounds.y      = y;
    }

    /// Positions the node relative to an arbitrary sibling or ancestor.
    /// `src_anchor` is the attachment point on this node; `dst_anchor` is the
    /// reference point on the target node.
    pub fn set_position_anchored_to(&mut self, src_anchor: Anchor, target: usize, dst_anchor: Anchor, x: f32, y: f32) {
        self.src_anchor         = src_anchor;
        self.target        = Some(target);
        self.dst_anchor = dst_anchor;
        self.bounds.x      = x;
        self.bounds.y      = y;
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
        let ref_edges = match self.target {
            None      => parent_edges.clone(),
            Some(idx) => node_absolute_edges(idx, nodes),
        };
        let (px, py) = self.src_anchor.fractions();
        let (tx, ty) = self.dst_anchor.fractions();
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
}

impl UiNode {
    pub fn base(&self) -> &NodeBase {
        match self {
            UiNode::Container(n) => &n.base,
            UiNode::Panel(n)     => &n.base,
            UiNode::Button(n)    => &n.base,
            UiNode::Checkbox(n)  => &n.base,
            UiNode::Label(n)     => &n.base,
            UiNode::Slider(n)    => &n.panel.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut NodeBase {
        match self {
            UiNode::Container(n) => &mut n.base,
            UiNode::Panel(n)     => &mut n.base,
            UiNode::Button(n)    => &mut n.base,
            UiNode::Checkbox(n)  => &mut n.base,
            UiNode::Label(n)     => &mut n.base,
            UiNode::Slider(n)    => &mut n.panel.base,
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
