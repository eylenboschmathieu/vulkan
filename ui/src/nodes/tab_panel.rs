use anyhow::Result;
use crate::{types::Rgba, Ui};

use super::{Axis, GroupNode, TabListNode, UiNode};

/// Whether a [`TabPanelNode`]'s body area should be a plain panel or a
/// vertically-scrollable scroll panel.
pub enum TabBody {
    Panel,
    ScrollPanel { scrollbar_width: f32 },
}

/// Composite tab widget: a [`TabListNode`] strip at the top and a content body
/// below. Use [`crate::Ui::add_tab`] to append tabs; the returned button index
/// and content panel index are the caller's to style and populate.
/// Constructed via [`crate::Ui::create_tab_panel`].
pub struct TabPanelNode {
    pub group: GroupNode,
    pub tab_list_idx: usize,
    /// The outer body node (a `Panel` or `ScrollPanel`).
    pub body_idx: usize,
    /// Where tab content panels are added — equals `body_idx` for a plain
    /// panel, or the scroll panel's `content_idx` for a scroll panel body.
    pub(crate) body_content_idx: usize,
    /// `(button_idx, content_panel_idx)` per tab, in insertion order.
    pub(crate) tabs: Vec<(usize, usize)>,
    pub(crate) active_tab: usize,
    /// When set, the active tab button gets this color; inactive tabs get `default_tab_color`.
    pub selected_tab_color: Option<Rgba>,
    pub default_tab_color:  Option<Rgba>,
    /// Hover color restored on inactive tab buttons when another tab is selected.
    /// When `None`, inactive tabs keep whatever hover color was set on them by the caller.
    pub tab_hover_color: Option<Rgba>,
}

impl TabPanelNode {
    fn new() -> Self {
        Self {
            group:              GroupNode::new(),
            tab_list_idx:       0,
            body_idx:           0,
            body_content_idx:   0,
            tabs:               Vec::new(),
            active_tab:         0,
            selected_tab_color: None,
            default_tab_color:  None,
            tab_hover_color:    None,
        }
    }

    pub fn build(
        ui:               &mut Ui,
        parent:           usize,
        width:            f32,
        height:           f32,
        tab_height:       f32,
        scrollbar_height: f32,
        body:             TabBody,
    ) -> Result<(usize, usize)> {
        let frame_idx = ui.tree.add_child(UiNode::TabPanel(Self::new()), parent)?;
        {
            let frame = ui.get_node_mut::<Self>(frame_idx)?;
            frame.group.base.set_size(width, height);
            frame.group.base.interactive = false;
        }

        // Body is added first so the tab list (and its hover scrollbar) renders
        // on top when they overlap.
        let body_y      = tab_height;
        let body_height = height - tab_height;

        let (body_idx, body_content_idx) = match body {
            TabBody::Panel => {
                let (p_idx, p) = ui.create_panel(frame_idx)?;
                p.base.set_size(width, body_height);
                p.base.bounds.y = body_y;
                p.container.clip_children = true;
                (p_idx, p_idx)
            }
            TabBody::ScrollPanel { scrollbar_width } => {
                let (sp_idx, sp) = ui.create_scroll_panel(
                    frame_idx, Axis::Vertical,
                    (width - scrollbar_width, body_height),
                    scrollbar_width,
                    (width - scrollbar_width, body_height),
                )?;
                let content_idx = sp.content_idx;
                sp.base.bounds.y = body_y;
                (sp_idx, content_idx)
            }
        };

        let (tab_list_idx, _) = TabListNode::build(ui, frame_idx, width, tab_height, scrollbar_height)?;

        {
            let frame = ui.get_node_mut::<Self>(frame_idx)?;
            frame.tab_list_idx     = tab_list_idx;
            frame.body_idx         = body_idx;
            frame.body_content_idx = body_content_idx;
        }

        ui.tab_panels.push(frame_idx);

        Ok((frame_idx, body_idx))
    }
}
