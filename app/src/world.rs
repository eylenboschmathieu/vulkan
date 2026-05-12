#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{collections::HashMap};
use anyhow::Result;
use blitz::{Blitz, Container, TextureArrayId};
use cgmath::{Point3, Vector3};
use crate::{
    block::{Block, BlockType, Face},
    chunk::{CHUNK_SIZE, Chunk},
};

#[derive(Debug)]
pub struct World {
    chunks: HashMap<Vector3<i32>, Chunk>,
}

impl World {
    pub fn new() -> Self {
        let positions = [
            Vector3{x: -32, y: -32, z: -32},
            Vector3{x: -32, y: -32, z:   0},
            Vector3{x:   0, y: -32, z: -32},
            Vector3{x:   0, y: -32, z:   0},
        ];

        let chunks: HashMap<Vector3<i32>, Chunk> = positions.into_iter()
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

    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Option<&Block> {
        let cs = CHUNK_SIZE as i32;
        let chunk_pos = Vector3 {
            x: x.div_euclid(cs) * cs,
            y: y.div_euclid(cs) * cs,
            z: z.div_euclid(cs) * cs,
        };

        let lx = x.rem_euclid(cs) as usize;
        let ly = y.rem_euclid(cs) as usize;
        let lz = z.rem_euclid(cs) as usize;

        if let Some(chunk) = self.chunks.get(&chunk_pos) {
            return Some(&chunk.blocks()[lx][ly][lz]);
        }

        None
    }

    pub unsafe fn remove_block(&mut self, container: &mut Container, position: Vector3<i32>) {
        let cs = CHUNK_SIZE as i32;
        let chunk_pos = Vector3 {
            x: position.x.div_euclid(cs) * cs,
            y: position.y.div_euclid(cs) * cs,
            z: position.z.div_euclid(cs) * cs,
        };
        let lx = position.x.rem_euclid(cs) as usize;
        let ly = position.y.rem_euclid(cs) as usize;
        let lz = position.z.rem_euclid(cs) as usize;

        if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
            if chunk.blocks()[lx][ly][lz].is_air() { return; }
            chunk.update_block(lx, ly, lz, BlockType::Air);
            chunk.dirty = true;
            chunk.recalc([None; 6], container, (chunk_pos.x, chunk_pos.y, chunk_pos.z));
        }
    }

    pub fn raycast(&self, origin: Point3<f32>, direction: Vector3<f32>, max_distance: f32) -> Option<(Vector3<i32>, Face)> {
        const VOXEL_SIZE: f32 = 1.0;
        let mut block_pos = Vector3::new(
            origin.x.floor() as i32,
            origin.y.floor() as i32,
            origin.z.floor() as i32,
        );

        // Which direction we step in each axis (-1 or +1)
        let step = Vector3::new(
            if direction.x >= 0.0 { 1 } else { -1 },
            if direction.y >= 0.0 { 1 } else { -1 },
            if direction.z >= 0.0 { 1 } else { -1 },
        );

        // How far along the ray we must travel to cross one voxel in each axis
        let delta = Vector3::new(
            (VOXEL_SIZE / direction.x).abs(),
            (VOXEL_SIZE / direction.y).abs(),
            (VOXEL_SIZE / direction.z).abs(),
        );

        // How far along the ray to the first voxel boundary in each axis
        let mut t_max = Vector3::new(
            if direction.x >= 0.0 { (block_pos.x as f32 + VOXEL_SIZE - origin.x) / direction.x }
            else { (origin.x - block_pos.x as f32) / -direction.x },
            if direction.y >= 0.0 { (block_pos.y as f32 + VOXEL_SIZE - origin.y) / direction.y }
            else { (origin.y - block_pos.y as f32) / -direction.y },
            if direction.z >= 0.0 { (block_pos.z as f32 + VOXEL_SIZE - origin.z) / direction.z }
            else { (origin.z - block_pos.z as f32) / -direction.z },
        );

        let mut last_face = Face::TOP;

        loop {
            // Check current voxel
            if let Some(block) = self.block_at(block_pos.x, block_pos.y, block_pos.z) {
                if !block.is_air() {
                    return Some((block_pos, last_face));
                }
            }

            // Step to next voxel boundary - whichever axis is closest
            if t_max.x < t_max.y && t_max.x < t_max.z {
                if t_max.x > max_distance { return None; }
                block_pos.x += step.x;
                t_max.x += delta.x;
                last_face = if step.x > 0 { Face::WEST } else { Face::EAST };
            } else if t_max.y < t_max.z {
                if t_max.y > max_distance { return None; }
                block_pos.y += step.y;
                t_max.y += delta.y;
                last_face = if step.y > 0 { Face::BOTTOM } else { Face::TOP };
            } else {
                if t_max.z > max_distance { return None; }
                block_pos.z += step.z;
                t_max.z += delta.z;
                last_face = if step.z > 0 { Face::NORTH } else { Face::SOUTH };
            }
        }
    }
}