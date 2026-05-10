#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{vec2, vec3};
use blitz::{Blitz, Container, Mesh, Vertex_3D_TextureArray, TextureArrayId};

use crate::block::{Block, BlockType};

pub const CHUNK_SIZE: usize = 32;

type Blocks = [[[Block; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE];

struct FaceDesc {
    normal_axis: usize,
    positive:    bool,
    u_axis:      usize,
    v_axis:      usize,
    u_flip:      bool,
}

// Order matches NEIGHBOR_OFFSETS: +X, -X, +Y, -Y, +Z, -Z
const FACE_DESCS: [FaceDesc; 6] = [
    FaceDesc { normal_axis: 0, positive: true,  u_axis: 1, v_axis: 2, u_flip: false }, // +X
    FaceDesc { normal_axis: 0, positive: false, u_axis: 1, v_axis: 2, u_flip: true  }, // -X
    FaceDesc { normal_axis: 1, positive: true,  u_axis: 0, v_axis: 2, u_flip: true  }, // +Y
    FaceDesc { normal_axis: 1, positive: false, u_axis: 0, v_axis: 2, u_flip: false }, // -Y
    FaceDesc { normal_axis: 2, positive: true,  u_axis: 0, v_axis: 1, u_flip: false }, // +Z (top)
    FaceDesc { normal_axis: 2, positive: false, u_axis: 0, v_axis: 1, u_flip: true  }, // -Z (bottom)
];

#[derive(Debug)]
pub struct Chunk {
    dirty: bool,
    blocks: Blocks,
    mesh: ChunkMesh,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            dirty: false,
            blocks: [[[Block::default(); CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE],
            mesh: ChunkMesh::new(),
        }
    }

    pub fn blocks(&self) -> &Blocks {
        &self.blocks
    }

    pub fn generate(&mut self, chunk_x: i32, chunk_y: i32, chunk_z: i32) {
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                let surface = surface_z(chunk_x + x as i32, chunk_y + y as i32);
                for z in 0..CHUNK_SIZE {
                    let wz = chunk_z + z as i32;
                    self.blocks[x][y][z] = Block {
                        type_: if wz > surface {
                            BlockType::Air
                        } else if wz == surface {
                            BlockType::Grass
                        } else if wz >= surface - 3 {
                            BlockType::Dirt
                        } else {
                            BlockType::Stone
                        },
                    };
                }
            }
        }
    }

    pub fn mesh(&self) -> &ChunkMesh {
        &self.mesh
    }

    pub unsafe fn recalc(&mut self, neighbors: [Option<&Blocks>; 6], container: &mut Container, offset: (i32, i32, i32)) {
        self.mesh.recalc(&self.blocks, neighbors, container, offset);
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz, texture_array_id: TextureArrayId) {
        if let Some(mesh) = self.mesh.mesh() {
            blitz.draw_array(mesh, texture_array_id);
        }
    }
}

#[derive(Debug)]
pub struct ChunkMesh {
    mesh: Option<Mesh>,
}

impl ChunkMesh {
    pub fn new() -> Self {
        Self { mesh: None }
    }

    pub fn mesh(&self) -> Option<Mesh> {
        self.mesh
    }

    pub unsafe fn recalc(&mut self, blocks: &Blocks, neighbors: [Option<&Blocks>; 6], container: &mut Container, offset: (i32, i32, i32)) {
        if let Some(mesh) = self.mesh {
            container.free_mesh(mesh);
            self.mesh = None;
        }

        let mut vertices: Vec<Vertex_3D_TextureArray> = vec![];
        let mut indices:  Vec<u16> = vec![];

        for (face_idx, desc) in FACE_DESCS.iter().enumerate() {
            for d in 0..CHUNK_SIZE {

                // Build face mask: which (u, v) cells expose a face this slice
                let mut mask = [[None::<BlockType>; CHUNK_SIZE]; CHUNK_SIZE];

                for u in 0..CHUNK_SIZE {
                    for v in 0..CHUNK_SIZE {
                        let mut pos = [0usize; 3];
                        pos[desc.normal_axis] = d;
                        pos[desc.u_axis]      = u;
                        pos[desc.v_axis]      = v;

                        let block = blocks[pos[0]][pos[1]][pos[2]];
                        if block.is_air() { continue; }

                        let neighbor_is_air = if desc.positive {
                            if d + 1 < CHUNK_SIZE {
                                let mut np = pos;
                                np[desc.normal_axis] = d + 1;
                                blocks[np[0]][np[1]][np[2]].is_air()
                            } else {
                                let mut np = pos;
                                np[desc.normal_axis] = 0;
                                neighbors[face_idx].map_or(true, |nb| nb[np[0]][np[1]][np[2]].is_air())
                            }
                        } else {
                            if d > 0 {
                                let mut np = pos;
                                np[desc.normal_axis] = d - 1;
                                blocks[np[0]][np[1]][np[2]].is_air()
                            } else {
                                let mut np = pos;
                                np[desc.normal_axis] = CHUNK_SIZE - 1;
                                neighbors[face_idx].map_or(true, |nb| nb[np[0]][np[1]][np[2]].is_air())
                            }
                        };

                        if neighbor_is_air {
                            mask[u][v] = Some(block.type_);
                        }
                    }
                }

                // Greedy merge
                let mut visited = [[false; CHUNK_SIZE]; CHUNK_SIZE];

                for u in 0..CHUNK_SIZE {
                    for v in 0..CHUNK_SIZE {
                        if visited[u][v] { continue; }
                        let block_type = match mask[u][v] {
                            Some(bt) => bt,
                            None     => continue,
                        };

                        // Expand width along u
                        let mut w = 1;
                        while u + w < CHUNK_SIZE
                            && !visited[u + w][v]
                            && mask[u + w][v] == Some(block_type)
                        {
                            w += 1;
                        }

                        // Expand height along v
                        let mut h = 1;
                        'expand_v: while v + h < CHUNK_SIZE {
                            for du in 0..w {
                                if visited[u + du][v + h] || mask[u + du][v + h] != Some(block_type) {
                                    break 'expand_v;
                                }
                            }
                            h += 1;
                        }

                        // Mark rectangle visited
                        for du in 0..w {
                            for dv in 0..h {
                                visited[u + du][v + dv] = true;
                            }
                        }

                        // Emit quad
                        let layer  = Block { type_: block_type }.layer_for_face(face_idx);
                        let normal = {
                            let mut n = [0.0f32; 3];
                            n[desc.normal_axis] = if desc.positive { 1.0 } else { -1.0 };
                            vec3(n[0], n[1], n[2])
                        };
                        let face_d = if desc.positive { d + 1 } else { d } as f32;
                        let (u0, u1) = if desc.u_flip {
                            ((u + w) as f32, u as f32)
                        } else {
                            (u as f32, (u + w) as f32)
                        };
                        let (v0, v1) = (v as f32, (v + h) as f32);

                        // Corner positions in (u, v) space, then mapped to 3D
                        let uv_pairs = [(u0, v0), (u1, v0), (u1, v1), (u0, v1)];

                        // UVs tile proportionally; side faces flip v so texture reads top-down
                        let tex_uvs: [(f32, f32); 4] = if face_idx < 4 {
                            [(0.0, h as f32), (w as f32, h as f32), (w as f32, 0.0), (0.0, 0.0)]
                        } else {
                            [(0.0, 0.0), (w as f32, 0.0), (w as f32, h as f32), (0.0, h as f32)]
                        };

                        let base = vertices.len() as u16;
                        for (i, (cu, cv)) in uv_pairs.iter().enumerate() {
                            let mut p = [0.0f32; 3];
                            p[desc.normal_axis] = face_d;
                            p[desc.u_axis]      = *cu;
                            p[desc.v_axis]      = *cv;
                            vertices.push(Vertex_3D_TextureArray::new(
                                vec3(offset.0 as f32 + p[0], offset.1 as f32 + p[1], offset.2 as f32 + p[2]),
                                vec2(tex_uvs[i].0, tex_uvs[i].1),
                                layer,
                                normal,
                            ));
                        }
                        indices.extend_from_slice(&[base, base+1, base+2, base+2, base+3, base]);
                    }
                }
            }
        }

        if !vertices.is_empty() {
            assert!(vertices.len() <= u16::MAX as usize, "chunk mesh exceeds u16 vertex limit");
            self.mesh = Some(container.alloc_mesh(&vertices, &indices));
        }
    }
}

fn surface_z(wx: i32, wy: i32) -> i32 {
    let fx = wx as f32;
    let fy = wy as f32;
    let h = (fx * 0.08).sin() * 6.0
          + (fy * 0.08).sin() * 6.0
          + (fx * 0.13 + fy * 0.09).sin() * 3.0
          + (fx * 0.05 - fy * 0.11).cos() * 2.0;
    (-13 + h as i32).clamp(-30, -2)
}
