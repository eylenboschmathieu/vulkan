#![allow(non_camel_case_types, unsafe_op_in_unsafe_fn, unused_variables, clippy::unnecessary_wraps)]

use vulkanalia::vk::{self, HasBuilder};

type Vec2 = cgmath::Vector2<f32>;
type Vec3 = cgmath::Vector3<f32>;
type Vec4 = cgmath::Vector4<f32>;

pub type Pos2 = cgmath::Vector2<f32>;
pub type Pos3 = cgmath::Vector3<f32>;
pub type Rgba = cgmath::Vector4<f32>;
pub type UV   = cgmath::Vector2<f32>;

#[derive(Clone)]
pub enum VertexFormat {
    VERTEX_2D_RGBA,
    VERTEX_2D_TEXTURE,
    VERTEX_3D_RGBA,
    VERTEX_3D_RGBA_TEXTURE,
    VERTEX_3D_TEXTURE_ARRAY_NORMAL,
}

impl VertexFormat {
    pub fn binding_description(&self, binding: u32) -> vk::VertexInputBindingDescription {
        match self {
            VertexFormat::VERTEX_2D_RGBA                 => VERTEX_2D_RGBA::binding_description(binding),
            VertexFormat::VERTEX_2D_TEXTURE              => VERTEX_2D_TEXTURE::binding_description(binding),
            VertexFormat::VERTEX_3D_RGBA                 => VERTEX_3D_RGBA::binding_description(binding),
            VertexFormat::VERTEX_3D_RGBA_TEXTURE         => VERTEX_3D_RGBA_TEXTURE::binding_description(binding),
            VertexFormat::VERTEX_3D_TEXTURE_ARRAY_NORMAL => VERTEX_3D_TEXTURE_ARRAY::binding_description(binding),
        }
    }

    pub fn attribute_description(&self, binding: u32) -> Vec<vk::VertexInputAttributeDescription> {
        match self {
            VertexFormat::VERTEX_2D_RGBA                 => VERTEX_2D_RGBA::attribute_description(binding).to_vec(),
            VertexFormat::VERTEX_2D_TEXTURE              => VERTEX_2D_TEXTURE::attribute_description(binding).to_vec(),
            VertexFormat::VERTEX_3D_RGBA                 => VERTEX_3D_RGBA::attribute_description(binding).to_vec(),
            VertexFormat::VERTEX_3D_RGBA_TEXTURE         => VERTEX_3D_RGBA_TEXTURE::attribute_description(binding).to_vec(),
            VertexFormat::VERTEX_3D_TEXTURE_ARRAY_NORMAL => VERTEX_3D_TEXTURE_ARRAY::attribute_description(binding).to_vec(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VERTEX_2D_TEXTURE {
    pos: Vec2,
    uv:  Vec2,
}

impl VERTEX_2D_TEXTURE {
    pub const fn new(pos: Vec2, uv: Vec2) -> Self {
        Self { pos, uv }
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

        let uv = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(size_of::<Vec2>() as u32)
            .build();

        [pos, uv]
    }
}

// Primarily used for the UI.
// To handle text as well ui widgets, we need the uv to be a single white texel.
// "Technically" we're using a texture, but in reality it's that single texel.
// The rest are characters in an atlas, not actual textures in the traditional sense.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VERTEX_2D_RGBA {
    pos:   Vec2,
    uv:    Vec2,
    color: Vec4,
}

impl VERTEX_2D_RGBA {
    pub const fn new(pos: Vec2, uv: Vec2, color: Vec4) -> Self {
        Self { pos, uv, color }
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
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0)
            .build();

        let uv = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(size_of::<Vec2>() as u32)
            .build();

        let color = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(2)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset((size_of::<Vec2>() * 2) as u32)
            .build();

        [pos, uv, color]
    }
}


#[repr(C)]
#[derive(Clone, Copy)]
pub struct VERTEX_3D_RGBA {
    pos:   Vec3,
    color: Vec4,
}

impl VERTEX_3D_RGBA {
    pub const fn new(pos: Vec3, color: Vec4) -> Self {
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
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(size_of::<Vec3>() as u32)
            .build();

        [pos, color]
    }
}


#[repr(C)]
#[derive(Clone, Copy)]
pub struct VERTEX_3D_RGBA_TEXTURE {
    pos:   Vec3,
    color: Vec4,
    uv:    Vec2,
}

impl VERTEX_3D_RGBA_TEXTURE {
    pub const fn new(pos: Vec3, color: Vec4, uv: Vec2) -> Self {
        Self { pos, color, uv }
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
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(size_of::<Vec3>() as u32)
            .build();

        let tex_coord = vk::VertexInputAttributeDescription::builder()
            .binding(binding)
            .location(2)
            .format(vk::Format::R32G32_SFLOAT)
            .offset((size_of::<Vec3>() + size_of::<Vec4>()) as u32)
            .build();

        [pos, color, tex_coord]
    }
}


#[repr(C)]
#[derive(Clone, Copy)]
pub struct VERTEX_3D_TEXTURE_ARRAY {
    pos:    Vec3,
    uv:     Vec2,
    layer:  u32,
    normal: Vec3,
}

impl VERTEX_3D_TEXTURE_ARRAY {
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
