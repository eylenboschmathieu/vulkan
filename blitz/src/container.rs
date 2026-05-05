#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{fs::File, marker::PhantomData};

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0, Handle, HasBuilder, Semaphore};

use crate::{
    DescriptorSetUpdateInfo, commands::CommandBuffer, globals, mesh::Mesh, resources::{
        image::ImageMemoryBarrierQueueFamilyIndices, vertices::Vertex_3D_Color_Texture
    }
};

#[derive(Debug, Clone, Copy)]
pub struct MeshHandle(pub usize);

#[derive(Debug, Clone, Copy)]
pub struct TextureHandle(pub usize);

pub(crate) struct MeshData {
    pub vertices: Vec<Vertex_3D_Color_Texture>,
    pub vertices_size: usize,
    pub indices: Vec<u16>,
    pub indices_size: usize,
}

pub(crate) struct TextureData {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct Loading;
pub struct Transfer;
pub struct Resolved;

/// Data transfer class.
/// 1. Get a container from blitz
/// 2. Load the container with data
/// 3. Tell blitz to process the container
/// 4. Resolve the indices
pub struct Container<State> {
    pub(crate) _state: std::marker::PhantomData<State>,

    pub(crate) meshes: Vec<MeshData>,
    pub(crate) textures: Vec<TextureData>,

    pub(crate) resolved_meshes: Vec<Mesh>,
    pub(crate) resolved_textures: Vec<usize>,

    pub(crate) semaphore: Semaphore,
}

impl<S> Container<S> {
    pub(crate) fn transition<T>(self) -> Container<T> {
        Container {
            _state: PhantomData,
            meshes: self.meshes,
            resolved_meshes: self.resolved_meshes,
            textures: self.textures,
            resolved_textures: self.resolved_textures,
            semaphore: self.semaphore,
        }
    }
}

// App requests one of these and fills it up with data
impl Container<Loading> {
    pub unsafe fn new() -> Result<Self> {
        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let semaphore = globals::device().logical().create_semaphore(&semaphore_info, None)?;

        Ok(Self {
            _state: PhantomData,
            meshes: vec![],
            textures: vec![],
            resolved_meshes: vec![],
            resolved_textures: vec![],
            semaphore,
        })
    }

    pub fn load_mesh(&mut self, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) -> Result<MeshHandle> {
        self.meshes.push(MeshData {
            vertices: vertices.to_vec(),
            vertices_size: vertices.len() * size_of::<Vertex_3D_Color_Texture>(),
            indices: indices.to_vec(),
            indices_size: indices.len() * size_of::<u16>(),
        });
        Ok(MeshHandle(self.meshes.len() - 1))
    }

    pub fn load_texture(&mut self, path: &str) -> Result<TextureHandle> {
        let file = File::open(path)?;

        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info()?;

        let mut pixels = vec![0; reader.info().raw_bytes()];
        reader.next_frame(&mut pixels)?;

        let (width, height) = reader.info().size();

        let pixels: Vec<u8> = match (reader.info().color_type, reader.info().bit_depth) {
            (png::ColorType::Rgba, png::BitDepth::Eight)    => pixels,
            (png::ColorType::Rgb,  png::BitDepth::Eight)    => pixels
                .chunks(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            (png::ColorType::Rgba, png::BitDepth::Sixteen)  => pixels
                .chunks(8)   // 4 channels × 2 bytes each
                .flat_map(|p| [p[0], p[2], p[4], p[6]])    // high byte of each channel
                .collect(),
            (png::ColorType::Rgb,  png::BitDepth::Sixteen)  => pixels
                .chunks(6)   // 3 channels × 2 bytes each
                .flat_map(|p| [p[0], p[2], p[4], 255])
                .collect(),
            (t, d) => return Err(anyhow::anyhow!("Unsupported PNG format: {:?} {:?}", t, d)),
        };

        let texture = TextureData {
            width,
            height,
            pixels,
        };

        self.textures.push(texture);

        Ok(TextureHandle(self.textures.len() - 1))
    }
}

// App passes a container back to the vulkan lib, which then processes it by
// creating staging buffers and uploading the data
impl Container<Transfer> {
    pub unsafe fn process(&mut self) -> Result<()> {
        let command_buffer = globals::command_manager().begin_one_time_submit(vk::QueueFlags::TRANSFER)?;
        let mut update_info = vec![];

        let staging_buffer_id = self.process_buffers(&command_buffer)?;

        let mut staging_texture_id = 0;
        if self.textures.len() > 0 {
            staging_texture_id = self.process_textures(&command_buffer)?;
        }

        command_buffer.end_one_time_submit(
            globals::queue_manager().transfer(),
            if self.textures.len() > 0 { Some(self.semaphore) } else { None },
        )?;

        // Ownership transfer for textures

        if self.textures.len() > 0 {
            let command_buffer = globals::command_manager().begin_one_time_submit(vk::QueueFlags::GRAPHICS)?;

            self.ownership_transfer(&command_buffer)?;

            command_buffer.end_one_time_submit(globals::queue_manager().graphics(), Some(self.semaphore))?;
            globals::staging_buffer_mut().free(staging_texture_id);
        }

        // Update descriptor sets

        for &id in &self.resolved_textures {
            update_info.push(DescriptorSetUpdateInfo {
                binding: 0,
                descriptor_set: vk::DescriptorSet::null(), // Fetched from the texture directly
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                id,
            });
        }
        globals::descriptor_pool().update(&update_info);

        globals::staging_buffer_mut().free(staging_buffer_id);

        Ok(())
    }

    // Returns the staging buffer id to free at the end
    unsafe fn process_buffers(&mut self, command_buffer: &CommandBuffer) -> Result<usize> {
        let sb = globals::staging_buffer_mut();
        let vb = globals::vertex_buffer_mut();
        let ib = globals::index_buffer_mut();

        // Calculate size of staging buffer
        let mut size = 0;
        for mesh in &self.meshes {
            size += mesh.vertices_size + mesh.indices_size;
        }

        // Create staging buffer for buffers and copy data into it
        let s_id = sb.alloc(size)?; // staging buffer id
        let mut offset = 0;
        for mesh in &self.meshes {
            sb.copy_to_staging_at(s_id, mesh.vertices.as_ref(), offset)?;
            offset += mesh.vertices_size;
            sb.copy_to_staging_at(s_id, mesh.indices.as_ref(), offset)?;
            offset += mesh.indices_size;
        }

        // Copy vertex data
        offset = 0;
        for mesh in &self.meshes {
            let v_id = vb.alloc(mesh.vertices_size)?;
            let i_id = ib.alloc(mesh.indices.len())?;
            self.resolved_meshes.push(Mesh { vertices: v_id, indices: i_id });

            let v_alloc = vb.alloc_info(v_id);
            let i_alloc = ib.alloc_info(i_id);

            sb.copy_to_buffer( // Copy data from staging buffer to vertex buffer
                command_buffer,
                vb,
                v_alloc,
                offset as u64,
            )?;
            offset += mesh.vertices_size;
            sb.copy_to_buffer( // Copy data from staging buffer to index buffer
                command_buffer,
                ib,
                i_alloc,
                offset as u64,
            )?;
            offset += mesh.indices_size;
        }

        Ok(s_id)
    }

    // Returns the staging buffer id to free at the end
    unsafe fn process_textures(&mut self, command_buffer: &CommandBuffer) -> Result<usize> {
        let queue_family_indices = globals::device().queue_family_indices();

        // Create staging buffer for textures and copy data into it
        let mut size = 0;
        for texture in &self.textures {
            size += texture.pixels.len() * size_of::<u8>();
        }
        let s_id = globals::staging_buffer_mut().alloc(size)?; // Staging buffer id

        // Upload data
        let mut offset = 0;
        for tex_data in &self.textures {
            globals::staging_buffer_mut().copy_to_staging_at(s_id, tex_data.pixels.as_ref(), offset)?;
            offset += tex_data.pixels.len() * size_of::<u8>();
        }
        offset = 0;
        for tex_data in &self.textures {
            let id = globals::textures_mut().new_texture(tex_data)?;
            self.resolved_textures.push(id);
            let image = globals::textures()[id].image.clone();

            image.transition_image_layout(
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                None,
            )?;
            globals::staging_buffer_mut().copy_to_image_at(
                command_buffer,
                s_id,
                &image,
                offset as u64,
            )?;
            image.transition_image_layout(
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                Some(ImageMemoryBarrierQueueFamilyIndices { // Release queue ownership
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
        for id in &self.resolved_textures {
            let image = globals::textures()[*id].image.clone();
            image.transition_image_layout(
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                Some(ImageMemoryBarrierQueueFamilyIndices { // Acquire queue ownership
                    src_queue_family_index: queue_family_indices.transfer(),
                    dst_queue_family_index: queue_family_indices.graphics(),
                }),
            )?;
        }

        Ok(())
    }
}

// Resolve the temporary ids returned by the container into real gpu buffer ids
impl Container<Resolved> {
    pub fn resolve_mesh(&self, handle: MeshHandle) -> Mesh {
        self.resolved_meshes[handle.0]
    }

    pub fn resolve_texture(&self, handle: TextureHandle) -> usize {
        self.resolved_textures[handle.0]
    }

    pub unsafe fn destroy(&self) {
        globals::device().logical().destroy_semaphore(self.semaphore, None);
    }
}