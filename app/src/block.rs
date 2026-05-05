#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum BlockType {
    Air = 0,
    Dirt = 1,
    Stone = 2,
    Sand = 3,
}

impl Default for BlockType {
    fn default() -> Self {
        BlockType::Dirt
    }
}

#[derive(Clone, Copy, Default)]
pub struct Block {
    type_: BlockType
}