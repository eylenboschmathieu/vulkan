#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::unnecessary_wraps)]

use vulkanalia::vk::{self, HasBuilder};

type Vec2 = cgmath::Vector2<f32>;
type Vec3 = cgmath::Vector3<f32>;

#[derive(Debug, Clone)]
pub enum VertexFormat {
    Vertex2D_Color,
    Vertex3D_Color,
    Vertex3D_Color_Texture,
    Vertex3D_TextureArray,
}

impl VertexFormat {
    pub fn binding_description(&self, binding: u32) -> vk::VertexInputBindingDescription {
        match self {
            VertexFormat::Vertex2D_Color => Vertex_2D_Color::binding_description(binding),
            VertexFormat::Vertex3D_Color => Vertex_3D_Color::binding_description(binding),
            VertexFormat::Vertex3D_Color_Texture => Vertex_3D_Color_Texture::binding_description(binding),
            VertexFormat::Vertex3D_TextureArray => Vertex_3D_TextureArray::binding_description(binding),
        }
    }

    pub fn attribute_description(&self, binding: u32) -> Vec<vk::VertexInputAttributeDescription> {
        match self {
            VertexFormat::Vertex2D_Color => Vertex_2D_Color::attribute_description(binding).to_vec(),
            VertexFormat::Vertex3D_Color => Vertex_3D_Color::attribute_description(binding).to_vec(),
            VertexFormat::Vertex3D_Color_Texture => Vertex_3D_Color_Texture::attribute_description(binding).to_vec(),
            VertexFormat::Vertex3D_TextureArray => Vertex_3D_TextureArray::attribute_description(binding).to_vec(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct Vertex_2D_Color {
    pos: Vec2,
    color: Vec3,
}

impl Vertex_2D_Color {
    pub const fn new (pos: Vec2, color: Vec3) -> Self {
        Self { pos, color }
    }

    pub fn binding_description(binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(binding)
            .stride(size_of::<Self>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    pub fn attribute_description(binding: u32) -> [vk::VertexInputAttributeDescription; 2] {
        let pos = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        let color = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(size_of::<Vec3>() as u32)
            .build();

        [pos, color]
    }
}


#[repr(C)]
#[derive(Debug, Clone)]
pub struct Vertex_3D_Color {
    pos: Vec3,
    color: Vec3,
}

impl Vertex_3D_Color {
    pub const fn new (pos: Vec3, color: Vec3) -> Self {
        Self { pos, color }
    }

    pub fn binding_description(binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(binding)
            .stride(size_of::<Self>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    pub fn attribute_description(binding: u32) -> [vk::VertexInputAttributeDescription; 2] {
        let pos = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0)
            .build();

        let color = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(size_of::<Vec3>() as u32)
            .build();

        [pos, color]
    }
}


#[repr(C)]
#[derive(Debug, Clone)]
pub struct Vertex_3D_Color_Texture {
    pos: Vec3,
    color: Vec3,
    tex_coord: Vec2,
}

impl Vertex_3D_Color_Texture {
    pub const fn new (pos: Vec3, color: Vec3, tex_coord: Vec2) -> Self {
        Self { pos, color, tex_coord }
    }

    pub fn binding_description(binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(binding)
            .stride(size_of::<Self>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    pub fn attribute_description(binding: u32) -> [vk::VertexInputAttributeDescription; 3] {
        let pos = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0)
            .build();

        let color = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(size_of::<Vec3>() as u32)
            .build();

        let tex_coord = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(2)
            .format(vk::Format::R32G32_SFLOAT)
            .offset((size_of::<Vec3>() + size_of::<Vec3>()) as u32)
            .build();

        [pos, color, tex_coord]
    }
}


#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex_3D_TextureArray {
    pos:    Vec3,
    uv:     Vec2,
    layer:  u32,
    normal: Vec3,
}

impl Vertex_3D_TextureArray {
    pub const fn new(pos: Vec3, uv: Vec2, layer: u32, normal: Vec3) -> Self {
        Self { pos, uv, layer, normal }
    }

    pub fn binding_description(binding: u32) -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(binding)
            .stride(size_of::<Self>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    pub fn attribute_description(binding: u32) -> [vk::VertexInputAttributeDescription; 4] {
        let pos = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0)
            .build();

        let uv = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(size_of::<Vec3>() as u32)
            .build();

        let layer = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(2)
            .format(vk::Format::R32_UINT)
            .offset((size_of::<Vec3>() + size_of::<Vec2>()) as u32)
            .build();

        let normal = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(3)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset((size_of::<Vec3>() + size_of::<Vec2>() + size_of::<u32>()) as u32)
            .build();

        [pos, uv, layer, normal]
    }
}
