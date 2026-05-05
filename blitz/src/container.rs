#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::fs::File;

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0, Handle, HasBuilder};

use crate::{
    DescriptorSetUpdateInfo, TextureId,
    commands::CommandBuffer,
    globals,
    mesh::Mesh,
    resources::image::ImageMemoryBarrierQueueFamilyIndices,
    resources::vertices::Vertex_3D_Color_Texture,
};

pub(crate) struct TextureData {
    pub pixels: Vec<u8>,
    pub width:  u32,
    pub height: u32,
}

// Eagerly loads meshes and textures
pub struct Container {
    meshes:    Vec<(Mesh, Vec<Vertex_3D_Color_Texture>, Vec<u16>)>,
    textures:  Vec<(TextureId, TextureData)>,
    semaphore: vk::Semaphore,
}

impl Container {
    pub(crate) unsafe fn new() -> Result<Self> {
        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let semaphore = globals::device().logical().create_semaphore(&semaphore_info, None)?;
        Ok(Self { meshes: vec![], textures: vec![], semaphore })
    }

    /// Eagerly allocates GPU buffer slots and returns the live Mesh immediately.
    /// Data is staged to the GPU when the upload closure returns.
    pub unsafe fn load_mesh(&mut self, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) -> Mesh {
        let v_id = globals::vertex_buffer_mut()
            .alloc(vertices.len() * size_of::<Vertex_3D_Color_Texture>())
            .expect("Vertex buffer OOM");
        let i_id = globals::index_buffer_mut()
            .alloc(indices.len())
            .expect("Index buffer OOM");
        let mesh = Mesh { vertices: v_id, indices: i_id };
        self.meshes.push((mesh, vertices.to_vec(), indices.to_vec()));
        mesh
    }

    /// Eagerly creates the Vulkan image/view/sampler and returns the live TextureId.
    /// Pixel data is staged to the GPU when the upload closure returns.
    pub unsafe fn load_texture(&mut self, path: &str) -> Result<TextureId> {
        let file = File::open(path)?;
        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info()?;
        let mut pixels = vec![0; reader.info().raw_bytes()];
        reader.next_frame(&mut pixels)?;
        let (width, height) = reader.info().size();

        let pixels: Vec<u8> = match (reader.info().color_type, reader.info().bit_depth) {
            (png::ColorType::Rgba, png::BitDepth::Eight)   => pixels,
            (png::ColorType::Rgb,  png::BitDepth::Eight)   => pixels.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            (png::ColorType::Rgba, png::BitDepth::Sixteen) => pixels.chunks(8).flat_map(|p| [p[0], p[2], p[4], p[6]]).collect(),
            (png::ColorType::Rgb,  png::BitDepth::Sixteen) => pixels.chunks(6).flat_map(|p| [p[0], p[2], p[4], 255]).collect(),
            (t, d) => return Err(anyhow::anyhow!("Unsupported PNG format: {:?} {:?}", t, d)),
        };

        let tex_data = TextureData { pixels, width, height };
        let id = globals::textures_mut().new_texture(&tex_data)?;
        self.textures.push((id, tex_data));
        Ok(id)
    }

    pub(crate) unsafe fn process(&mut self) -> Result<()> {
        if !self.meshes.is_empty() {
            let command_buffer = globals::command_manager().begin_one_time_submit(vk::QueueFlags::TRANSFER)?;
            let s_id = self.upload_meshes(&command_buffer)?;
            command_buffer.end_one_time_submit(globals::queue_manager().transfer(), None)?;
            globals::staging_buffer_mut().free(s_id);
        }

        if !self.textures.is_empty() {
            let command_buffer = globals::command_manager().begin_one_time_submit(vk::QueueFlags::TRANSFER)?;
            let s_id = self.upload_textures(&command_buffer)?;
            command_buffer.end_one_time_submit(globals::queue_manager().transfer(), Some(self.semaphore))?;

            let command_buffer = globals::command_manager().begin_one_time_submit(vk::QueueFlags::GRAPHICS)?;
            self.ownership_transfer(&command_buffer)?;
            command_buffer.end_one_time_submit(globals::queue_manager().graphics(), Some(self.semaphore))?;

            globals::staging_buffer_mut().free(s_id);

            let update_info: Vec<DescriptorSetUpdateInfo> = self.textures.iter()
                .map(|(id, _)| DescriptorSetUpdateInfo {
                    binding: 0,
                    descriptor_set: vk::DescriptorSet::null(),
                    descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    id: *id,
                })
                .collect();
            globals::descriptor_pool().update(&update_info);
        }

        Ok(())
    }

    unsafe fn upload_meshes(&self, command_buffer: &CommandBuffer) -> Result<usize> {
        let sb = globals::staging_buffer_mut();
        let vb = globals::vertex_buffer_mut();
        let ib = globals::index_buffer_mut();

        let size: usize = self.meshes.iter()
            .map(|(_, v, i)| v.len() * size_of::<Vertex_3D_Color_Texture>() + i.len() * size_of::<u16>())
            .sum();

        let s_id = sb.alloc(size)?;

        let mut offset = 0;
        for (_, vertices, indices) in &self.meshes {
            sb.copy_to_staging_at(s_id, vertices.as_ref(), offset)?;
            offset += vertices.len() * size_of::<Vertex_3D_Color_Texture>();
            sb.copy_to_staging_at(s_id, indices.as_ref(), offset)?;
            offset += indices.len() * size_of::<u16>();
        }

        offset = 0;
        for (mesh, vertices, indices) in &self.meshes {
            sb.copy_to_buffer(command_buffer, vb, vb.alloc_info(mesh.vertices), offset as u64)?;
            offset += vertices.len() * size_of::<Vertex_3D_Color_Texture>();
            sb.copy_to_buffer(command_buffer, ib, ib.alloc_info(mesh.indices), offset as u64)?;
            offset += indices.len() * size_of::<u16>();
        }

        Ok(s_id)
    }

    unsafe fn upload_textures(&self, command_buffer: &CommandBuffer) -> Result<usize> {
        let queue_family_indices = globals::device().queue_family_indices();

        let size: usize = self.textures.iter().map(|(_, t)| t.pixels.len()).sum();
        let s_id = globals::staging_buffer_mut().alloc(size)?;

        let mut offset = 0;
        for (id, tex_data) in &self.textures {
            globals::staging_buffer_mut().copy_to_staging_at(s_id, tex_data.pixels.as_ref(), offset)?;
            let image = globals::textures()[*id].image.clone();

            image.transition_image_layout(
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                None,
            )?;
            globals::staging_buffer_mut().copy_to_image_at(command_buffer, s_id, &image, offset as u64)?;
            image.transition_image_layout(
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                Some(ImageMemoryBarrierQueueFamilyIndices {
                    src_queue_family_index: queue_family_indices.transfer(),
                    dst_queue_family_index: queue_family_indices.graphics(),
                }),
            )?;
            offset += tex_data.pixels.len();
        }

        Ok(s_id)
    }

    unsafe fn ownership_transfer(&self, command_buffer: &CommandBuffer) -> Result<()> {
        let queue_family_indices = globals::device().queue_family_indices();
        for (id, _) in &self.textures {
            let image = globals::textures()[*id].image.clone();
            image.transition_image_layout(
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                Some(ImageMemoryBarrierQueueFamilyIndices {
                    src_queue_family_index: queue_family_indices.transfer(),
                    dst_queue_family_index: queue_family_indices.graphics(),
                }),
            )?;
        }
        Ok(())
    }

    pub(crate) unsafe fn destroy(&self) {
        globals::device().logical().destroy_semaphore(self.semaphore, None);
    }
}
