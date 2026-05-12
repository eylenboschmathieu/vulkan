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
    TOP = 0,
    BOTTOM = 1,
    EAST = 2,
    WEST = 3,
    SOUTH = 4,
    NORTH = 5,
}

impl Display for BlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Dirt
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

    // Face indices: 0=+X(East), 1=-X(West), 2=+Y(top), 3=-Y(bottom), 4=+Z(South), 5=-Z(North)
    pub fn layer_for_face(self, face: usize) -> u32 {
        match self.kind {
            BlockType::Air   => 0,
            BlockType::Dirt  => 2,
            BlockType::Stone => 3,
            BlockType::Sand  => 3,
            BlockType::Grass => match face {
                2 => 0, // +Y top: grass.png
                3 => 2, // -Y bottom: dirt.png
                _ => 1, // sides: grass_side.png
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