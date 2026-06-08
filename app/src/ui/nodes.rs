use blitz::Rgba;

use super::{Edges, Rect, Ui, UiAction};

// ── Layout primitives ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Clip,
}

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
}

/// Callbacks fired as a node becomes or stops being the visible menu screen.
#[derive(Default)]
pub struct VisibilityCb {
    /// Fired right after this node becomes the visible menu screen.
    pub on_show: Option<Box<dyn FnMut(&mut Ui)>>,
    /// Fired right before this node stops being the visible menu screen.
    pub on_hide: Option<Box<dyn FnMut(&mut Ui)>>,
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

// ── Node types ───────────────────────────────────────────────────────────────

/// Invisible grouping node — children only, no quad rendered.
pub struct ContainerNode {
    pub base: NodeBase,
}

impl ContainerNode {
    pub fn new() -> Self {
        Self { base: NodeBase::new() }
    }
}

/// Visible background panel. Labelable.
pub struct PanelNode {
    pub base: NodeBase,
    pub color: Rgba,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

impl PanelNode {
    pub fn new() -> Self {
        Self {
            base: NodeBase::new(),
            color: Rgba::new(0.0, 0.0, 0.0, 0.0),
            uv_min: [0.0, 0.0],
            uv_max: [0.0, 0.0],
        }
    }

    pub fn set_color(&mut self, color: Rgba) { self.color = color; }
}

/// Holds the four interaction callbacks shared by any interactive node type.
#[derive(Default)]
pub struct InteractionCb {
    pub on_pressed: Option<UiAction>,
    pub on_release: Option<UiAction>,
    pub on_enter:   Option<UiAction>,
    pub on_leave:   Option<UiAction>,
}

/// Interactive button. Labelable.
pub struct ButtonNode {
    pub base:        NodeBase,
    pub color:       Rgba,
    pub hover_color: Option<Rgba>,
    pub uv_min:      [f32; 2],
    pub uv_max:      [f32; 2],
    pub interaction: InteractionCb,
}

impl ButtonNode {
    pub fn new() -> Self {
        Self {
            base:        NodeBase::new(),
            color:       Rgba::new(0.0, 0.0, 0.0, 0.0),
            hover_color: None,
            uv_min:      [0.0, 0.0],
            uv_max:      [0.0, 0.0],
            interaction: InteractionCb::default(),
        }
    }
}

/// Toggleable checkbox with distinct unselected, selected, and hovered colours.
pub struct CheckboxNode {
    pub base:           NodeBase,
    pub color:          Rgba,        // unselected colour
    pub selected_color: Rgba,        // selected colour
    pub hover_color:    Option<Rgba>,
    pub uv_min:         [f32; 2],
    pub uv_max:         [f32; 2],
    pub selected:       bool,
    pub hovered:        bool,
    pub interaction:    InteractionCb,
}

impl CheckboxNode {
    pub fn new() -> Self {
        Self {
            base:           NodeBase::new(),
            color:          Rgba::new(0.5, 0.5, 0.5, 0.4),
            selected_color: Rgba::new(0.2, 0.7, 0.3, 0.7),
            hover_color:    None,
            uv_min:         [0.0, 0.0],
            uv_max:         [0.0, 0.0],
            selected:       false,
            hovered:        false,
            interaction:    InteractionCb::default(),
        }
    }

    pub fn display_color(&self) -> Rgba {
        if self.hovered {
            self.hover_color.unwrap_or(if self.selected { self.selected_color } else { self.color })
        } else if self.selected {
            self.selected_color
        } else {
            self.color
        }
    }
}

/// Drag gesture state: tracks whether a drag is active and the cursor
/// position / value captured when it began, so deltas can be computed
/// without accumulating drift.
#[derive(Default, Clone, Copy)]
pub struct Draggable {
    pub is_dragging:  bool,
    pub start_cursor: (f32, f32),
    pub start_value:  f32,
}

impl Draggable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, cursor: (f32, f32), value: f32) {
        self.is_dragging  = true;
        self.start_cursor = cursor;
        self.start_value  = value;
    }

    pub fn stop(&mut self) {
        self.is_dragging = false;
    }
}

/// Slider
pub struct SliderNode {
    pub panel: PanelNode,
    min_value: u32,
    max_value: u32,
    pub value: u32,
    pub step_size: u32,
    pub drag: Draggable,
    thumb_idx: Option<usize>,
    label_idx: Option<usize>,
}

impl SliderNode {
    pub fn new() -> Self {
        let mut this = Self {
            panel: PanelNode::new(),
            min_value: 0,
            max_value: 0,
            value: 0,
            step_size: 1,
            drag: Draggable::new(),
            thumb_idx: None,
            label_idx: None,
        };

        this.panel.base.set_size(200.0, 32.0);
        this.panel.set_color(Rgba { x: 0.0, y: 0.0, z: 0.0, w: 0.5 });

        this
    }

    pub fn get_label(&self) -> Option<usize> {
        self.label_idx
    }

    pub fn get_thumb(&self) -> Option<usize> {
        self.thumb_idx
    }

    pub fn set_min_max(&mut self, min: u32, max: u32) {
        self.min_value = min;
        self.max_value = max;
        self.value     = self.value.clamp(min, max);
    }

    /// Clamps to `[min_value, max_value]` and snaps down to the nearest step.
    pub fn set_value(&mut self, value: u32) {
        let value = value.clamp(self.min_value, self.max_value);
        let steps = (value - self.min_value) / self.step_size;
        self.value = self.min_value + steps * self.step_size;
    }

    pub fn set_label(&mut self, idx: Option<usize>) {
        self.label_idx = idx;
    }

    pub fn set_thumb(&mut self, idx: Option<usize>) {
        self.thumb_idx = idx;
    }

    /// Fraction (0.0-1.0) of the current value along `[min_value, max_value]`.
    fn value_fraction(&self) -> f32 {
        if self.max_value > self.min_value {
            (self.value - self.min_value) as f32 / (self.max_value - self.min_value) as f32
        } else {
            0.0
        }
    }

    /// The thumb's x-offset from the track's left edge for the current value.
    pub fn thumb_offset(&self, thumb_width: f32) -> f32 {
        self.value_fraction() * (self.panel.base.bounds.width - thumb_width)
    }

    /// Value formatted and right-padded with spaces to the width of `max_value`
    /// so the label maintains a stable visual width across all possible values.
    pub fn display_text(&self) -> String {
        let width = self.max_value.to_string().len();
        format!("{:<width$}", self.value)
    }

    /// The value implied by dragging the cursor away from where the drag started.
    pub fn value_from_drag(&self, cursor: (f32, f32), thumb_width: f32) -> u32 {
        let usable_width = (self.panel.base.bounds.width - thumb_width).max(1.0);
        let delta_value  = (cursor.0 - self.drag.start_cursor.0) / usable_width
            * (self.max_value - self.min_value) as f32;
        (self.drag.start_value + delta_value).round().clamp(self.min_value as f32, self.max_value as f32) as u32
    }
}

/// Text label. Not interactive, not labelable itself.
pub struct LabelNode {
    pub base: NodeBase,
    pub text: String,
    pub color: Rgba,
    /// Longest `text` has ever been, in chars. Rendering always reserves this
    /// many quads (padding unused slots with degenerate ones), so the label's
    /// vertex allocation stays constant across [`Ui::flush_dirty`] updates and
    /// only needs to grow — never shrink — via [`Ui::flush_all`].
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

    /// Replaces the text, growing `max_len` if it's now the longest this
    /// label has ever held. Returns `true` when `max_len` grows — the caller
    /// must rebuild the whole tree (`Ui::dirty = true`) so [`Ui::flush_all`]
    /// reserves the larger allocation; otherwise an in-place
    /// [`Ui::flush_dirty`] update is enough.
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
