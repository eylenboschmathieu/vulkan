mod button;
mod checkbox;
mod container;
mod group;
mod label;
mod panel;
mod renderable;
mod scroll_panel;
mod slider;
mod tab_list;
mod tab_panel;
mod window;

use crate::{Edges, Rect, Ui};
use crate::types::{Rgba, Texture};

pub use button::ButtonNode;
pub use checkbox::CheckboxNode;
pub use container::Container;
pub use group::GroupNode;
pub use label::LabelNode;
pub use panel::PanelNode;
pub use renderable::Renderable;
pub use scroll_panel::{Scroll, ScrollPanelNode, SCROLLBAR_THUMB_PADDING};
pub use slider::{Axis, SliderNode};
pub use tab_list::TabListNode;
pub use tab_panel::{TabBody, TabPanelNode};
pub use window::{WindowBody, WindowNode, TITLEBAR_HEIGHT, WINDOW_BORDER};

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
    /// Fired while this node is the [`Ui`](crate::Ui) capture target (see
    /// [`Ui::start_key_capture`](crate::Ui::start_key_capture)), with the
    /// host-supplied name of the key that was pressed
    /// ([`UiInput::captured_key`](crate::UiInput::captured_key)). The
    /// callback is responsible for calling
    /// [`Ui::end_key_capture`](crate::Ui::end_key_capture) once it's done
    /// with the key (e.g. after recording a new binding).
    pub on_key_capture: Option<Box<dyn FnMut(&mut Ui, &str)>>,
    pub hover_color:     Option<Rgba>,
    pub pressed_color:   Option<Rgba>,
    pub hover_texture:   Option<Texture>,
    pub pressed_texture: Option<Texture>,
}

pub struct NodeBase {
    pub bounds:        Rect,
    pub anchoring:     Anchoring,
    pub parent:        Option<usize>,
    pub visible:       bool,
    pub vertex_offset: usize,
    pub visibility:    VisibilityCb,
    /// When `false`, `hit_test` treats this node as transparent to pointer
    /// events (children remain hit-testable). Used for overlay nodes like the
    /// focus ring that must render on top but must never absorb input.
    pub interactive:   bool,
    /// When `false`, this node is excluded from Tab/Shift+Tab keyboard
    /// navigation. Set to `false` on structural buttons (window close,
    /// scroll-panel step, slider thumb) so only semantic controls participate.
    pub tab_stop:      bool,
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
            interactive:   true,
            tab_stop:      true,
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
        Some(p) => {
            let edges = node_absolute_edges(p, nodes);
            match nodes[p].scroll() {
                Some(s) => edges.translate(-s.offset.0, -s.offset.1),
                None    => edges,
            }
        }
    };
    node.base().resolve(&parent_edges, nodes)
}

// ── UiNode enum ──────────────────────────────────────────────────────────────

pub enum UiNode {
    Group(GroupNode),
    Panel(PanelNode),
    Button(ButtonNode),
    Checkbox(CheckboxNode),
    Label(LabelNode),
    ScrollPanel(ScrollPanelNode),
    Slider(SliderNode),
    TabList(TabListNode),
    TabPanel(TabPanelNode),
    Window(WindowNode),
}

impl UiNode {
    pub fn base(&self) -> &NodeBase {
        match self {
            UiNode::Group(n)       => &n.base,
            UiNode::Panel(n)       => &n.base,
            UiNode::Button(n)      => &n.base,
            UiNode::Checkbox(n)    => &n.base,
            UiNode::Label(n)       => &n.base,
            UiNode::ScrollPanel(n) => &n.base,
            UiNode::Slider(n)      => &n.panel.base,
            UiNode::TabList(n)     => &n.panel.base,
            UiNode::TabPanel(n)    => &n.group.base,
            UiNode::Window(n)      => &n.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut NodeBase {
        match self {
            UiNode::Group(n)       => &mut n.base,
            UiNode::Panel(n)       => &mut n.base,
            UiNode::Button(n)      => &mut n.base,
            UiNode::Checkbox(n)    => &mut n.base,
            UiNode::Label(n)       => &mut n.base,
            UiNode::ScrollPanel(n) => &mut n.base,
            UiNode::Slider(n)      => &mut n.panel.base,
            UiNode::TabList(n)     => &mut n.panel.base,
            UiNode::TabPanel(n)    => &mut n.group.base,
            UiNode::Window(n)      => &mut n.base,
        }
    }

    /// Child indices, for container-like node types (`Group`, `Panel`,
    /// `Button`, `ScrollPanel`, `Slider`, `Window`). `None` for leaf node
    /// types, which cannot have children.
    pub fn children(&self) -> Option<&[usize]> {
        match self {
            UiNode::Group(n)       => Some(&n.container.children),
            UiNode::Panel(n)       => Some(&n.container.children),
            UiNode::Button(n)      => Some(&n.children),
            UiNode::ScrollPanel(n) => Some(&n.container.children),
            UiNode::Slider(n)      => Some(&n.panel.container.children),
            UiNode::TabList(n)     => Some(&n.panel.container.children),
            UiNode::TabPanel(n)    => Some(&n.group.container.children),
            UiNode::Window(n)      => Some(&n.container.children),
            UiNode::Checkbox(_) | UiNode::Label(_) => None,
        }
    }

    /// Mutable child indices; see [`UiNode::children`].
    pub fn children_mut(&mut self) -> Option<&mut Vec<usize>> {
        match self {
            UiNode::Group(n)       => Some(&mut n.container.children),
            UiNode::Panel(n)       => Some(&mut n.container.children),
            UiNode::Button(n)      => Some(&mut n.children),
            UiNode::ScrollPanel(n) => Some(&mut n.container.children),
            UiNode::Slider(n)      => Some(&mut n.panel.container.children),
            UiNode::TabList(n)     => Some(&mut n.panel.container.children),
            UiNode::TabPanel(n)    => Some(&mut n.group.container.children),
            UiNode::Window(n)      => Some(&mut n.container.children),
            UiNode::Checkbox(_) | UiNode::Label(_) => None,
        }
    }

    /// The per-parent monotonic counter used to assign `z_index` to
    /// orderable children (see [`NodeBase::z_index`]); `None` for leaf node
    /// types, which cannot have children.
    pub fn z_sentinel_mut(&mut self) -> Option<&mut u32> {
        match self {
            UiNode::Group(n)       => Some(&mut n.container.z_sentinel),
            UiNode::Panel(n)       => Some(&mut n.container.z_sentinel),
            UiNode::Button(n)      => Some(&mut n.z_sentinel),
            UiNode::ScrollPanel(n) => Some(&mut n.container.z_sentinel),
            UiNode::Slider(n)      => Some(&mut n.panel.container.z_sentinel),
            UiNode::TabList(n)     => Some(&mut n.panel.container.z_sentinel),
            UiNode::TabPanel(n)    => Some(&mut n.group.container.z_sentinel),
            UiNode::Window(n)      => Some(&mut n.container.z_sentinel),
            UiNode::Checkbox(_) | UiNode::Label(_) => None,
        }
    }

    /// This node's [`Container`], for container-like node types (`Group`,
    /// `Panel`, `ScrollPanel`, `Window`); `None` for all others.
    fn container(&self) -> Option<&Container> {
        match self {
            UiNode::Group(n)       => Some(&n.container),
            UiNode::Panel(n)       => Some(&n.container),
            UiNode::ScrollPanel(n) => Some(&n.container),
            UiNode::TabList(n)     => Some(&n.panel.container),
            UiNode::TabPanel(n)    => Some(&n.group.container),
            UiNode::Window(n)      => Some(&n.container),
            _ => None,
        }
    }

    /// Mutable counterpart of [`UiNode::container`].
    fn container_mut(&mut self) -> Option<&mut Container> {
        match self {
            UiNode::Group(n)       => Some(&mut n.container),
            UiNode::Panel(n)       => Some(&mut n.container),
            UiNode::ScrollPanel(n) => Some(&mut n.container),
            UiNode::TabList(n)     => Some(&mut n.panel.container),
            UiNode::TabPanel(n)    => Some(&mut n.group.container),
            UiNode::Window(n)      => Some(&mut n.container),
            _ => None,
        }
    }

    /// Whether this node's children (and their whole subtrees) are clipped
    /// to this node's resolved bounds, intersected with any clip rect
    /// inherited from further up the tree. Only meaningful for
    /// container-like nodes (`Group`, `Panel`, `Window`); `false` for
    /// all others. Set via [`Ui::set_clip_children`](crate::Ui::set_clip_children).
    pub fn clip_children(&self) -> bool {
        self.container().is_some_and(|c| c.clip_children)
    }

    /// Mutable access to [`UiNode::clip_children`]; `None` for node types
    /// that don't carry the flag.
    pub fn clip_children_mut(&mut self) -> Option<&mut bool> {
        self.container_mut().map(|c| &mut c.clip_children)
    }

    /// Whether dragging one of this node's children clamps its position so
    /// its resolved edges stay within this node's resolved edges. Only
    /// meaningful for container-like nodes (`Group`, `Panel`, `Window`);
    /// `false` for all others. Set via
    /// [`Ui::set_clamp_children`](crate::Ui::set_clamp_children).
    pub fn clamp_children(&self) -> bool {
        self.container().is_some_and(|c| c.clamp_children)
    }

    /// Mutable access to [`UiNode::clamp_children`]; `None` for node types
    /// that don't carry the flag.
    pub fn clamp_children_mut(&mut self) -> Option<&mut bool> {
        self.container_mut().map(|c| &mut c.clamp_children)
    }

    /// This node's scroll state, for scrollable containers (currently only
    /// `Panel`, via [`PanelNode::scroll`]); `None` for all others, and for
    /// panels with scrolling disabled.
    pub fn scroll(&self) -> Option<&Scroll> {
        match self { UiNode::Panel(p) => p.scroll.as_ref(), _ => None }
    }

    /// Mutable counterpart of [`UiNode::scroll`].
    pub fn scroll_mut(&mut self) -> Option<&mut Scroll> {
        match self { UiNode::Panel(p) => p.scroll.as_mut(), _ => None }
    }

    /// Whether this node can receive keyboard focus via Tab/Shift+Tab
    /// traversal ([`Ui::focus_next`](crate::Ui::focus_next)/
    /// [`focus_prev`](crate::Ui::focus_prev)) and activation via Enter/Space.
    /// Currently `Button`, `Checkbox`, and `Slider`, subject to
    /// [`NodeBase::tab_stop`] being `true`. Structural buttons (window close,
    /// scroll-panel steps, slider thumb) have `tab_stop = false` set at build
    /// time.
    pub fn focusable(&self) -> bool {
        self.base().tab_stop && matches!(self, UiNode::Button(_) | UiNode::Checkbox(_) | UiNode::Slider(_))
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

ui_node_variant!(GroupNode,       Group);
ui_node_variant!(PanelNode,       Panel);
ui_node_variant!(ButtonNode,      Button);
ui_node_variant!(CheckboxNode,    Checkbox);
ui_node_variant!(LabelNode,       Label);
ui_node_variant!(ScrollPanelNode, ScrollPanel);
ui_node_variant!(SliderNode,      Slider);
ui_node_variant!(TabListNode,     TabList);
ui_node_variant!(TabPanelNode,    TabPanel);
ui_node_variant!(WindowNode,      Window);
