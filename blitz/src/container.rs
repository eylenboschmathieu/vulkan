#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::fs::File;

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0, Handle, HasBuilder};

use crate::{
    DescriptorSetUpdateInfo, TextureId, TextureArrayId,
    commands::CommandBuffer,
    globals,
    mesh::Mesh,
    resources::image::{Image, ImageMemoryBarrierQueueFamilyIndices},
    resources::vertices::Vertex_3D_Color_Texture,
};

pub(crate) struct TextureData {
    pub pixels: Vec<u8>,
    pub width:  u32,
    pub height: u32,
}

/// Batches GPU resource uploads for a single [`Blitz::upload`] call.
///
/// IDs are allocated eagerly so callers can store them immediately, but the
/// actual DMA transfers are deferred until [`Container::process`] runs at the
/// end of the closure.  Meshes go through the transfer queue only; textures go
/// transfer → graphics with an explicit queue-family ownership transfer.
pub struct Container {
    meshes:         Vec<(Mesh, Vec<u8>, Vec<u16>)>,
    textures:       Vec<(TextureId, TextureData)>,
    texture_arrays: Vec<(TextureArrayId, Vec<TextureData>)>,
    semaphore:      vk::Semaphore,
}

impl Container {
    pub(crate) unsafe fn new() -> Result<Self> {
        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let semaphore = globals::device().logical().create_semaphore(&semaphore_info, None)?;
        Ok(Self { meshes: vec![], textures: vec![], texture_arrays: vec![], semaphore })
    }

    /// Eagerly allocates GPU buffer slots and returns the live Mesh immediately.
    /// Data is staged to the GPU when the upload closure returns.
    pub unsafe fn alloc_mesh<V>(&mut self, vertices: &[V], indices: &[u16]) -> Mesh {
        let byte_len = vertices.len() * size_of::<V>();
        let v_id = globals::vertex_buffer_mut()
            .alloc(byte_len)
            .expect("Vertex buffer OOM");
        let i_id = globals::index_buffer_mut()
            .alloc(indices.len())
            .expect("Index buffer OOM");
        let mesh = Mesh { vertices: v_id, indices: i_id };
        let raw = std::slice::from_raw_parts(vertices.as_ptr() as *const u8, byte_len).to_vec();
        self.meshes.push((mesh, raw, indices.to_vec()));
        mesh
    }

    /// Return the vertex and index buffer slots back to the free-list.
    pub unsafe fn free_mesh(&self, mesh: Mesh) {
        globals::vertex_buffer_mut().free(mesh.vertices);
        globals::index_buffer_mut().free(mesh.indices);
    }

    /// Eagerly creates the Vulkan image/view/sampler and returns the live TextureId.
    /// Pixel data is staged to the GPU when the upload closure returns.
    pub unsafe fn alloc_texture(&mut self, path: &str) -> Result<TextureId> {
        let file = File::open(path)?;
        let mut decoder = png::Decoder::new(file);
        decoder.set_transformations(png::Transformations::EXPAND);
        let mut reader = decoder.read_info()?;
        let mut pixels = vec![0; reader.output_buffer_size()];
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

    /// Create a texture from an already-decoded RGBA8 pixel buffer.
    pub unsafe fn alloc_texture_from_pixels(&mut self, pixels: Vec<u8>, width: u32, height: u32) -> Result<TextureId> {
        let tex_data = TextureData { pixels, width, height };
        let id = globals::textures_mut().new_texture(&tex_data)?;
        self.textures.push((id, tex_data));
        Ok(id)
    }

    /// Build a `sampler2DArray` from a slice of PNG paths.
    ///
    /// All images must have the same dimensions; returns an error otherwise.
    /// Supports RGBA8, RGB8, RGBA16, RGB16 and Grayscale8 PNG formats.
    pub unsafe fn alloc_texture_array(&mut self, paths: &[&str]) -> Result<TextureArrayId> {
        let mut tiles: Vec<TextureData> = Vec::with_capacity(paths.len());
        let mut base_width = 0u32;
        let mut base_height = 0u32;

        for (i, path) in paths.iter().enumerate() {
            let file = File::open(path)?;
            let mut decoder = png::Decoder::new(file);
            decoder.set_transformations(png::Transformations::EXPAND);
            let mut reader = decoder.read_info()?;
            let mut pixels = vec![0; reader.output_buffer_size()];
            reader.next_frame(&mut pixels)?;
            let (width, height) = reader.info().size();

            let pixels: Vec<u8> = match (reader.info().color_type, reader.info().bit_depth) {
                (png::ColorType::Rgba,      png::BitDepth::Eight)    => pixels,
                (png::ColorType::Rgb,       png::BitDepth::Eight)    => pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
                (png::ColorType::Rgba,      png::BitDepth::Sixteen)  => pixels.chunks_exact(8).flat_map(|p| [p[0], p[2], p[4], p[6]]).collect(),
                (png::ColorType::Rgb,       png::BitDepth::Sixteen)  => pixels.chunks_exact(6).flat_map(|p| [p[0], p[2], p[4], 255]).collect(),
                (png::ColorType::Grayscale, png::BitDepth::Eight)    => pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
                (t, d) => return Err(anyhow::anyhow!("Unsupported PNG format: {:?} {:?}", t, d)),
            };

            if i == 0 {
                base_width = width;
                base_height = height;
            } else if width != base_width || height != base_height {
                return Err(anyhow::anyhow!(
                    "Texture array tile size mismatch at '{}': expected {}x{}, got {}x{}",
                    path, base_width, base_height, width, height
                ));
            }

            tiles.push(TextureData { pixels, width, height });
        }

        let id = globals::textures_mut().new_texture_array(paths.len() as u32, base_width, base_height)?;
        self.texture_arrays.push((id, tiles));
        Ok(id)
    }

    pub(crate) unsafe fn process(&mut self) -> Result<()> {
        if !self.meshes.is_empty() {
            let command_buffer = globals::commands().begin_one_time_submit(vk::QueueFlags::TRANSFER)?;
            let s_id = self.upload_meshes(&command_buffer)?;
            command_buffer.end_one_time_submit(globals::queues().transfer(), None)?;
            globals::staging_buffer_mut().free(s_id);
        }

        let has_images = !self.textures.is_empty() || !self.texture_arrays.is_empty();
        if has_images {
            let command_buffer = globals::commands().begin_one_time_submit(vk::QueueFlags::TRANSFER)?;

            let tex_s_id = if !self.textures.is_empty() {
                Some(self.upload_textures(&command_buffer)?)
            } else { None };

            let arr_s_id = if !self.texture_arrays.is_empty() {
                Some(self.upload_texture_arrays(&command_buffer)?)
            } else { None };

            command_buffer.end_one_time_submit(globals::queues().transfer(), Some(self.semaphore))?;

            let images: Vec<Image> = self.textures.iter()
                .map(|(id, _)| globals::textures()[*id].image.clone())
                .chain(self.texture_arrays.iter().map(|(id, _)| globals::textures().texture_array(*id).image.clone()))
                .collect();

            let command_buffer = globals::commands().begin_one_time_submit(vk::QueueFlags::GRAPHICS)?;
            self.ownership_transfer(&command_buffer, &images)?;
            command_buffer.end_one_time_submit(globals::queues().graphics(), Some(self.semaphore))?;

            if let Some(s_id) = tex_s_id { globals::staging_buffer_mut().free(s_id); }
            if let Some(s_id) = arr_s_id { globals::staging_buffer_mut().free(s_id); }

            if !self.textures.is_empty() {
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

            for (id, _) in &self.texture_arrays {
                let arr = globals::textures().texture_array(*id);
                globals::descriptor_pool().update_image_sampler(arr.descriptor_set, 0, arr.view, arr.sampler);
            }
        }

        Ok(())
    }

    unsafe fn upload_meshes(&self, command_buffer: &CommandBuffer) -> Result<usize> {
        let sb = globals::staging_buffer_mut();
        let vb = globals::vertex_buffer_mut();
        let ib = globals::index_buffer_mut();

        let size: usize = self.meshes.iter()
            .map(|(_, v, i)| v.len() + i.len() * size_of::<u16>())
            .sum();

        let s_id = sb.alloc(size)?;

        let mut offset = 0;
        for (_, v_bytes, indices) in &self.meshes {
            sb.copy_to_staging_at(s_id, v_bytes.as_slice(), offset)?;
            offset += v_bytes.len();
            sb.copy_to_staging_at(s_id, indices.as_ref(), offset)?;
            offset += indices.len() * size_of::<u16>();
        }

        offset = 0;
        for (mesh, v_bytes, indices) in &self.meshes {
            sb.copy_to_buffer(command_buffer, vb, vb.alloc_info(mesh.vertices), offset as u64)?;
            offset += v_bytes.len();
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

    /// Transfer ownership of `images` from the transfer queue to the graphics queue,
    /// transitioning them to `SHADER_READ_ONLY_OPTIMAL` in the process.
    unsafe fn ownership_transfer(&self, command_buffer: &CommandBuffer, images: &[Image]) -> Result<()> {
        let queue_family_indices = globals::device().queue_family_indices();
        for image in images {
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

    unsafe fn upload_texture_arrays(&self, command_buffer: &CommandBuffer) -> Result<usize> {
        let queue_family_indices = globals::device().queue_family_indices();

        let size: usize = self.texture_arrays.iter()
            .flat_map(|(_, tiles): &(TextureArrayId, Vec<TextureData>)| tiles.iter())
            .map(|t| t.pixels.len())
            .sum();
        let s_id = globals::staging_buffer_mut().alloc(size)?;

        let mut offset = 0usize;
        for (id, tiles) in &self.texture_arrays {
            let image = globals::textures().texture_array(*id).image.clone();

            image.transition_image_layout(
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                None,
            )?;

            for (layer, tile) in tiles.iter().enumerate().map(|(i, t): (usize, &TextureData)| (i, t)) {
                globals::staging_buffer_mut().copy_to_staging_at(s_id, tile.pixels.as_ref(), offset)?;
                globals::staging_buffer_mut().copy_to_image_layer(command_buffer, s_id, &image, offset as u64, layer as u32)?;
                offset += tile.pixels.len();
            }

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
        }

        Ok(s_id)
    }


    pub(crate) unsafe fn destroy(&self) {
        globals::device().logical().destroy_semaphore(self.semaphore, None);
    }
}
