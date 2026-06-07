#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::collections::HashMap;
use anyhow::Result;
use blitz::{Blitz, Container, LightingUbo, Mesh, VERTEX_3D_RGBA};
use cgmath::{vec3, vec4, InnerSpace, Matrix4, Point3, Vector3, Vector4};
use crate::{
    block::{Block, BlockType, Face},
    camera::FpCamera,
    chunk::{Blocks, CHUNK_SIZE, Chunk},
    input::{Action, InputManager},
};

struct Sun {
    angle: f32,
    mesh:  Mesh,
}

impl Sun {
    fn new() -> Self {
        Self { angle: 0.3, mesh: Mesh::default() }
    }

    unsafe fn alloc(&mut self, container: &mut Container) -> Result<()> {
        let gold = vec4(1.0, 0.84, 0.0, 1.0);
        self.mesh = container.alloc_mesh(blitz::WORLD_VB,
            &[
                VERTEX_3D_RGBA::new(vec3(-0.5, -0.5, 0.0), gold),
                VERTEX_3D_RGBA::new(vec3( 0.5, -0.5, 0.0), gold),
                VERTEX_3D_RGBA::new(vec3( 0.5,  0.5, 0.0), gold),
                VERTEX_3D_RGBA::new(vec3(-0.5,  0.5, 0.0), gold),
            ],
            &[2u16, 1, 0, 0, 3, 2],
        );
        Ok(())
    }

    fn update(&mut self, dt: f32) {
        self.angle = (self.angle + dt * std::f32::consts::TAU / 30.0) % std::f32::consts::TAU;
    }

    fn sun_dir(&self) -> Vector4<f32> {
        vec4(0.0, self.angle.sin(), self.angle.cos(), 0.0)
    }

    unsafe fn draw(&self, blitz: &mut Blitz, camera: &FpCamera) {
        let sun_dir_v = vec3(0.0, self.angle.sin(), self.angle.cos());
        let fwd       = camera.forward();
        let cam_right = camera.right();
        let look      = -fwd;
        let up        = cam_right.cross(look).normalize();
        let eye       = vec3(camera.eye.x, camera.eye.y, camera.eye.z);
        let sun_pos   = eye + sun_dir_v * 60.0;
        let model = Matrix4::from_cols(
            (-cam_right * 8.0).extend(0.0),
            (up         * 8.0).extend(0.0),
            look.extend(0.0),
            sun_pos.extend(1.0),
        );
        blitz.draw_dynamic_color(self.mesh, model);
    }
}

pub struct World {
    chunks: HashMap<Vector3<i32>, Chunk>,
    dirty_chunks: Vec<Vector3<i32>>,
    sun: Sun,
    texture_array_id: blitz::TextureArrayId,
}

const WORLD_Y_MIN: i32 = -64;
const WORLD_Y_MAX: i32 = 63;

const CHUNK_SIZE_I: i32 = CHUNK_SIZE as i32;
const NEIGHBOR_OFFSETS: [Vector3<i32>; 6] = [
    Vector3 { x:  CHUNK_SIZE_I, y: 0,            z: 0            }, // +X East
    Vector3 { x: -CHUNK_SIZE_I, y: 0,            z: 0            }, // -X West
    Vector3 { x: 0,             y:  CHUNK_SIZE_I, z: 0            }, // +Y top
    Vector3 { x: 0,             y: -CHUNK_SIZE_I, z: 0            }, // -Y bottom
    Vector3 { x: 0,             y: 0,            z:  CHUNK_SIZE_I }, // +Z South
    Vector3 { x: 0,             y: 0,            z: -CHUNK_SIZE_I }, // -Z North
];

impl World {
    pub fn new(blitz: &mut Blitz) -> Result<Self> {
        let cs = CHUNK_SIZE as i32;
        let chunks = (-1..=0).flat_map(|cx| (-1..=0).map(move |cz| {
            let pos = Vector3 { x: cx * cs, y: -cs, z: cz * cs };
            let mut chunk = Chunk::new();
            chunk.generate(pos.x, pos.y, pos.z);
            (pos, chunk)
        })).collect();

        let mut this = Self { chunks, dirty_chunks: Vec::new(), sun: Sun::new(), texture_array_id: 0 };

        unsafe {
            blitz.upload(|container| {
                this.texture_array_id = container.alloc_texture_array(&[
                    "app/img/tiles/grass.png",
                    "app/img/tiles/grass_side.png",
                    "app/img/tiles/dirt.png",
                    "app/img/tiles/cobble.png",
                ])?;
                this.alloc(container)?;
                Ok(())
            })?;
        }

        Ok(this)
    }

    pub unsafe fn alloc(&mut self, container: &mut Container) -> Result<()> {
        let positions: Vec<Vector3<i32>> = self.chunks.keys().copied().collect();
        for pos in positions {
            let chunk_ptr = match self.chunks.get_mut(&pos) {
                Some(c) => c as *mut Chunk,
                None => continue,
            };
            let neighbors: [Option<&Blocks>; 6] = std::array::from_fn(|i| {
                self.chunks.get(&(pos + NEIGHBOR_OFFSETS[i])).map(|c| c.blocks())
            });
            (*chunk_ptr).recalc(neighbors, container, (pos.x, pos.y, pos.z));
        }
        self.sun.alloc(container)?;
        Ok(())
    }

    pub fn update(&mut self, dt: f32) {
        self.sun.update(dt);
    }

    pub fn lighting_ubo(&self) -> LightingUbo {
        LightingUbo { sun_dir: self.sun.sun_dir() }
    }

    pub fn sky_color(&self) -> [f32; 4] {
        let t     = self.sun.sun_dir().y.max(0.0);
        let day   = [0.22_f32, 0.48, 0.72, 1.0];
        let night = [0.01_f32, 0.01, 0.05, 1.0];
        std::array::from_fn(|i| night[i] + (day[i] - night[i]) * t)
    }

    pub fn handle_input(&mut self, input: &InputManager, camera: &FpCamera) {
        if input.is_pressed(Action::PrimaryAction) {
            if let Some((pos, face)) = self.raycast(camera.eye, camera.forward(), 4.0) {
                let block = self.block_at(pos.x, pos.y, pos.z).unwrap();
                self.add_block(pos, face);
            }
        }

        if input.is_pressed(Action::SecondaryAction) {
            if let Some((pos, _face)) = self.raycast(camera.eye, camera.forward(), 4.0) {
                self.remove_block(pos);
            }
        }
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz, camera: &FpCamera) -> Result<()> {
        self.chunks
            .iter()
            .for_each(|(_, chunk)| {
                chunk.draw(blitz, self.texture_array_id);
            });
        self.sun.draw(blitz, camera);
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

    pub fn add_block(&mut self, mut position: Vector3<i32>, face: Face) {
        let cs = CHUNK_SIZE as i32;
        match face {
            Face::EAST => position.x += 1,
            Face::WEST => position.x -=1,
            Face::TOP => position.y += 1,
            Face::BOTTOM => position.y -= 1,
            Face::SOUTH => position.z += 1,
            Face::NORTH => position.z -= 1,
        }

        if position.y < WORLD_Y_MIN || position.y > WORLD_Y_MAX {
            println!("Reached world floor/ceiling");
            return;
        }

        let chunk_pos = Vector3 {
            x: position.x.div_euclid(cs) * cs,
            y: position.y.div_euclid(cs) * cs,
            z: position.z.div_euclid(cs) * cs,
        };
        let lx = position.x.rem_euclid(cs) as usize;
        let ly = position.y.rem_euclid(cs) as usize;
        let lz = position.z.rem_euclid(cs) as usize;

        if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
            if !chunk.blocks()[lx][ly][lz].is_air() { return; }
            chunk.update_block(lx, ly, lz, BlockType::Dirt);
        } else {
            let mut chunk = Chunk::new();
            chunk.update_block(lx, ly, lz, BlockType::Dirt);
            self.chunks.insert(chunk_pos, chunk);
        }

        if !self.dirty_chunks.contains(&chunk_pos) {
            self.dirty_chunks.push(chunk_pos)
        }

    }

    pub fn remove_block(&mut self, position: Vector3<i32>) {
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
            if !self.dirty_chunks.contains(&chunk_pos) {
                self.dirty_chunks.push(chunk_pos);
            }
        }
    }

    pub fn has_dirty_chunks(&self) -> bool {
        !self.dirty_chunks.is_empty()
    }

    pub unsafe fn flush_dirty(&mut self, container: &mut Container) {
        for chunk_pos in self.dirty_chunks.drain(..) {
            let chunk_ptr = match self.chunks.get_mut(&chunk_pos) {
                Some(c) => c as *mut Chunk,
                None => continue,
            };

            // Safe: neighbors are distinct entries in the map; no insert/remove occurs.
            let neighbors: [Option<&Blocks>; 6] = std::array::from_fn(|i| {
                self.chunks.get(&(chunk_pos + NEIGHBOR_OFFSETS[i])).map(|c| c.blocks())
            });

            (*chunk_ptr).recalc(neighbors, container, (chunk_pos.x, chunk_pos.y, chunk_pos.z));
        }
    }

    // Amanatides & Woo fast voxel traversal (1987).
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