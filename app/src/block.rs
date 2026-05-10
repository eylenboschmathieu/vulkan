#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockType {
    Air   = 0,
    Dirt  = 1,
    Stone = 2,
    Sand  = 3,
    Grass = 4,
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Dirt
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Block {
    pub type_: BlockType
}

impl Block {
    pub fn is_air(self) -> bool {
        matches!(self.type_, BlockType::Air)
    }

    // Face indices match NEIGHBOR_OFFSETS in chunk.rs: 0=+X, 1=-X, 2=+Y, 3=-Y, 4=+Z(top), 5=-Z(bottom)
    pub fn layer_for_face(self, face: usize) -> u32 {
        match self.type_ {
            BlockType::Air   => 0,
            BlockType::Dirt  => 2,
            BlockType::Stone => 3,
            BlockType::Sand  => 3,
            BlockType::Grass => match face {
                4 => 0, // +Z top: grass.png
                5 => 2, // -Z bottom: dirt.png
                _ => 1, // sides: grass_side.png
            },
        }
    }

    pub fn color(self) -> cgmath::Vector3<f32> {
        use cgmath::vec3;
        match self.type_ {
            BlockType::Air   => vec3(0.0, 0.0, 0.0),
            BlockType::Dirt  => vec3(0.55, 0.35, 0.15),
            BlockType::Stone => vec3(0.50, 0.50, 0.50),
            BlockType::Sand  => vec3(0.85, 0.80, 0.55),
            BlockType::Grass => vec3(0.30, 0.60, 0.20),
        }
    }
}