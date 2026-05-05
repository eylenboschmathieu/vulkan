#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{collections::HashMap};
use anyhow::Result;
use cgmath::Vector3 as ChunkPos;

use crate::{
    block::Block,
    chunk::{CHUNK_SIZE, Chunk},
};

pub struct World {
    chunks: HashMap<ChunkPos<i32>, Chunk>,
}

impl World {
    pub fn new() -> Self {
        // Read chunk data from file, or something...

        let chunks: HashMap<ChunkPos<i32>, Chunk> = HashMap::from([
            (ChunkPos{x: 0, y: 0, z: 0}, Chunk::new()),
            (ChunkPos{x: 0, y: 32, z: 0}, Chunk::new()),
            (ChunkPos{x: 32, y: 0, z: 0}, Chunk::new()),
            (ChunkPos{x: 32, y: 32, z: 0}, Chunk::new()),
        ]);

        Self { chunks }
    }

    pub fn render(&self) -> Result<()> {
        // Tell vulkan to start the render

        let mut  mesh_indices = vec![];
        self.chunks
            .iter()
            .for_each(|chunk| {
                mesh_indices.push(chunk.1.mesh());
            });

        // Send mesh_indices to vulkan for recording
        Ok(())
    }

    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Block {
        let chunk_pos = ChunkPos { x, y, z };
        
        let x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let y = y.rem_euclid(CHUNK_SIZE as i32) as usize;
        let z = z.rem_euclid(CHUNK_SIZE as i32) as usize;

        self.chunks.get(&chunk_pos).map(|c| c.blocks()[x][y][z].clone()).unwrap()
    }
}