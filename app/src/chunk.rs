#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use crate::block::Block;

pub const CHUNK_SIZE: usize = 32;

type Blocks = [[[Block; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE];

pub struct Chunk {
    blocks: Blocks,
    mesh: ChunkMesh,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            blocks: [[[Block::default(); 32]; 32]; 32],
            mesh: ChunkMesh {},
        }
    }

    pub fn blocks(&self) -> &Blocks {
        &self.blocks
    }

    pub fn mesh(&self) -> &ChunkMesh {
        &self.mesh
    }
}

pub struct ChunkMesh {
    // contains vertex and index buffer id's.
}

impl ChunkMesh {
    pub fn recalc(&self, blocks: Blocks) {
        // Code to recalculate the mesh

        // ask vulkan for new vertex and index buffers

        // store the buffer ids
    }

    pub unsafe fn record(&self) {
        // record the buffers
    }
}
