#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::fmt::Display;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockType {
    Air   = 0,
    Dirt  = 1,
    Stone = 2,
    Sand  = 3,
    Grass = 4,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Face {
    EAST   = 0, // +X
    WEST   = 1, // -X
    TOP    = 2, // +Y
    BOTTOM = 3, // -Y
    SOUTH  = 4, // +Z
    NORTH  = 5, // -Z
}

impl From<usize> for Face {
    fn from(idx: usize) -> Self {
        match idx {
            0 => Face::EAST,
            1 => Face::WEST,
            2 => Face::TOP,
            3 => Face::BOTTOM,
            4 => Face::SOUTH,
            5 => Face::NORTH,
            _ => panic!("Invalid face index: {idx}"),
        }
    }
}

impl Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Air
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Block {
    pub kind: BlockType
}

impl Block {
    pub fn is_air(self) -> bool {
        matches!(self.kind, BlockType::Air)
    }

    pub fn layer_for_face(self, face: Face) -> u32 {
        match self.kind {
            BlockType::Air   => 0,
            BlockType::Dirt  => 2,
            BlockType::Stone => 3,
            BlockType::Sand  => 3,
            BlockType::Grass => match face {
                Face::TOP    => 0,
                Face::BOTTOM => 2,
                _            => 1,
            },
        }
    }

    pub fn color(self) -> cgmath::Vector3<f32> {
        use cgmath::vec3;
        match self.kind {
            BlockType::Air   => vec3(0.0, 0.0, 0.0),
            BlockType::Dirt  => vec3(0.55, 0.35, 0.15),
            BlockType::Stone => vec3(0.50, 0.50, 0.50),
            BlockType::Sand  => vec3(0.85, 0.80, 0.55),
            BlockType::Grass => vec3(0.30, 0.60, 0.20),
        }
    }
}

impl Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}