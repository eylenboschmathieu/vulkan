#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{collections::HashMap};
use anyhow::Result;
use blitz::{Blitz, Container, TextureArrayId};
use cgmath::Vector3 as ChunkPos;

use crate::{
    block::Block,
    chunk::{CHUNK_SIZE, Chunk},
};

#[derive(Debug)]
pub struct World {
    chunks: HashMap<ChunkPos<i32>, Chunk>,
}

impl World {
    pub fn new() -> Self {
        let positions = [
            ChunkPos{x: -32, y: -32, z: -32},
            ChunkPos{x: -32, y:   0, z: -32},
            ChunkPos{x:   0, y: -32, z: -32},
            ChunkPos{x:   0, y:   0, z: -32},
        ];

        let chunks: HashMap<ChunkPos<i32>, Chunk> = positions.into_iter()
            .map(|pos| {
                let mut chunk = Chunk::new();
                chunk.generate(pos.x, pos.y, pos.z);
                (pos, chunk)
            })
            .collect();

        Self { chunks }
    }

    pub unsafe fn alloc(&mut self, container: &mut Container) -> Result<()> {
        self.chunks
            .iter_mut()
            .for_each(|(pos, chunk)| {
                chunk.recalc([None; 6], container, (pos.x, pos.y, pos.z));
            });
        Ok(())
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz, texture_array_id: TextureArrayId) -> Result<()> {
        self.chunks
            .iter()
            .for_each(|(_, chunk)| {
                chunk.draw(blitz, texture_array_id);
            });

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