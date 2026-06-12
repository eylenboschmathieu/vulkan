use crate::types::{TextureId, Vertex};

/// Result of querying the UI for changes since the last frame.
pub enum UiUpdate {
    /// Nothing changed; the host's existing vertex buffer is still valid.
    None,
    /// The vertex buffer must be fully replaced with this data, bound to the
    /// given texture.
    Full(TextureId, Vec<Vertex>),
    /// In-place patches into the existing buffer: `(vertex_offset, vertices)`
    /// pairs, each overwriting the vertices starting at `vertex_offset`. The
    /// texture binding from the last `Full` still applies.
    Partial(Vec<(usize, Vec<Vertex>)>),
}

/// A request for the host to apply to its window's cursor state.
#[derive(Clone, Copy)]
pub enum CursorRequest {
    /// Lock and hide the cursor (entering world/gameplay input).
    Lock,
    /// Free and show the cursor at the given logical position (entering a menu).
    Free { x: f32, y: f32 },
}

/// Something the UI can't act on itself and needs the host to handle, raised
/// by node callbacks (which only have `&mut Ui`) and drained via
/// [`crate::Ui::take_events`].
#[derive(Clone, Copy)]
pub enum UiEvent {
    /// The host should exit the application.
    Exit,
    /// The host should apply this cursor state to its window.
    SetCursor(CursorRequest),
    /// A click wasn't consumed by any UI element — the host should handle it
    /// itself (e.g. world interaction / selection).
    Unhandled,
}
