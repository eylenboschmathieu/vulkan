#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use crate::{IndexBufferId, VertexBufferId};

#[derive(Debug, Clone, Copy)]
pub struct Mesh {
    pub vertices: VertexBufferId,
    pub indices: IndexBufferId,
}