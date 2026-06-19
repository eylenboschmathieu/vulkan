/// Raw pointers to `Ui`'s dirty-flagging state, set once via
/// [`init`] after the `Ui` is placed in its final heap location.
/// Single-threaded; no synchronization.
///
/// # Safety invariant
/// `DIRTY` and `DIRTY_NODES` point into a live `Ui` for the program's
/// lifetime once [`init`] has been called. Nodes are never removed from the
/// tree, so indices are stable. Callers must not hold two `&mut` references
/// to the *same* node index simultaneously.
static mut DIRTY:       *mut bool       = std::ptr::null_mut();
static mut DIRTY_NODES: *mut Vec<usize> = std::ptr::null_mut();

/// Called once by [`crate::Ui::register_global`] after the `Ui` is in its
/// final location. `dirty` and `dirty_nodes` must remain valid for the rest
/// of the program's lifetime.
pub(crate) unsafe fn init(dirty: *mut bool, dirty_nodes: *mut Vec<usize>) {
    DIRTY       = dirty;
    DIRTY_NODES = dirty_nodes;
}

/// Queues `idx` for a partial re-render, unless a full rebuild is already
/// pending or the node is invisible (invisible nodes have no vertex slot).
/// A no-op before [`init`] is called.
pub(crate) fn mark_node_dirty(idx: usize, visible: bool) {
    unsafe {
        if DIRTY.is_null() { return; }
        if visible && !*DIRTY {
            (*DIRTY_NODES).push(idx);
        }
    }
}

/// Schedules a full tree rebuild. A no-op before [`init`] is called.
pub(crate) fn mark_full_dirty() {
    unsafe {
        if !DIRTY.is_null() {
            *DIRTY = true;
        }
    }
}
