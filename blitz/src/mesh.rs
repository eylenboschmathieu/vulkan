#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use crate::{
    resources::buffers::{
        index_buffer::IndexAllocId,
        vertex_buffer::VertexAllocId,
    },
};

/// A pair of sub-allocation IDs into the global vertex and index buffers.
///
/// Returned by [`Container::alloc_mesh`] and passed to the `draw_*` methods.
/// `Default` initialises both IDs to their sentinel values so an unallocated
/// mesh is obvious rather than silently aliasing slot 0.
#[derive(Debug, Clone, Copy)]
pub struct Mesh {
    pub vertices: VertexAllocId,
    pub indices:  IndexAllocId,
}

impl Default for Mesh {
    fn default() -> Self {
        Self { vertices: VertexAllocId::default(), indices: IndexAllocId::default() }
    }
}
