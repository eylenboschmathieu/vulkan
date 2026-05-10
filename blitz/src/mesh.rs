#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use crate::{IndexBufferId, VertexBufferId};

/// A pair of sub-allocation IDs into the global vertex and index buffers.
///
/// Returned by [`Container::alloc_mesh`] and passed to the `draw_*` methods.
/// `Default` initialises to `usize::MAX` so an unallocated mesh is obvious
/// rather than silently aliasing ID 0.
#[derive(Debug, Clone, Copy)]
pub struct Mesh {
    pub vertices: VertexBufferId,
    pub indices: IndexBufferId,
}

impl Default for Mesh {
    fn default() -> Self {
        Self { vertices: usize::MAX, indices: usize::MAX }
    }
}