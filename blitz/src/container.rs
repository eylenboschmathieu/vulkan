#![allow(dead_code, unsafe_op_in_unsafe_fn, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{fs::File, marker::PhantomData};

use anyhow::Result;
use vulkanalia::vk::{self, DeviceV1_0, HasBuilder, Semaphore};

use crate::{
    TextureId,
    commands::{
        CommandBuffer,
        CommandManager
    },
    device::Device,
    mesh::Mesh,
    queues::QueueManager,
    resources::{
        vertices::Vertex_3D_Color_Texture,
        image::ImageMemoryBarrierQueueFamilyIndices,
        resource_manager::ResourceManager
    }
};

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
    pub unsafe fn new(device: &Device) -> Result<Self> {
        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let semaphore = device.logical().create_semaphore(&semaphore_info, None)?;

        Ok(Self {
            _state: PhantomData,
            meshes: vec![],
            textures: vec![],
            resolved_meshes: vec![],
            resolved_textures: vec![],
            semaphore,
        })
    }

    /// Returns Mesh where vertices is the Id to internal mesh vector
    pub fn load_mesh (&mut self, vertices: &[Vertex_3D_Color_Texture], indices: &[u16]) -> Result<Mesh> {
        let mesh = MeshData {
            vertices: vertices.to_vec(),
            vertices_size: vertices.len() * size_of::<Vertex_3D_Color_Texture>(),
            indices: indices.to_vec(),
            indices_size: indices.len() * size_of::<u16>(),
        };
        self.meshes.push(mesh);
        
        Ok(Mesh { vertices: self.meshes.len() - 1, indices: 0})
    }

    pub fn load_texture(&mut self, path: &str) -> Result<TextureId> {
        let file = File::open(path)?;

        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info()?;

        let mut pixels = vec![0; reader.info().raw_bytes()];
        reader.next_frame(&mut pixels)?;

        let size = reader.info().raw_bytes() as u64;
        let (width, height) = reader.info().size();

        let texture = TextureData {
            width,
            height,
            pixels,
        };

        self.textures.push(texture);

        Ok(self.textures.len() - 1)
    }
}

// App passes a container back to the vulkan lib, which then processes it by
// creating staging buffers and uploading the data
impl Container<Transfer> {
    pub unsafe fn process(
        &mut self, device: &Device,
        command_manager: &CommandManager,
        resource_manager: &mut ResourceManager,
        queue_manager: &QueueManager
    ) -> Result<()> {
        let command_buffer = command_manager.begin_one_time_submit(device, vk::QueueFlags::TRANSFER)?;

        let buffer_id = self.process_buffers(device, &command_buffer, resource_manager)?;
        
        let mut texture_id = 0;
        if self.textures.len() > 0 {
            texture_id = self.process_textures(device, &command_buffer, resource_manager)?;
        }

        command_buffer.end_one_time_submit(
            device,
            queue_manager.transfer(),
            if self.textures.len() > 0 { Some(self.semaphore) } else { None }
        )?;

        // Ownership transfer for textures

        if self.textures.len() > 0 {
            let command_buffer = command_manager.begin_one_time_submit(device, vk::QueueFlags::GRAPHICS)?;

            self.ownership_transfer(device, &command_buffer, resource_manager)?;

            command_buffer.end_one_time_submit(device, queue_manager.graphics(), Some(self.semaphore))?;
            resource_manager.staging_buffer.free(texture_id);
        }

        resource_manager.staging_buffer.free(buffer_id);


        Ok(())
    }

    // Returns the staging buffer to free at the end
    unsafe fn process_buffers(&mut self, device: &Device, command_buffer: &CommandBuffer, resource_manager: &mut ResourceManager) -> Result<usize> {
        let sb = &mut resource_manager.staging_buffer;
        let vb = &mut resource_manager.vertex_buffer;
        let ib = &mut resource_manager.index_buffer;

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

            sb.copy_to_buffer(device, // Copy data from staging buffer to vertex buffer
                command_buffer,
                vb,
                v_alloc,
                offset as u64,
            )?;
            offset += mesh.vertices_size;
            sb.copy_to_buffer( // Copy data from staging buffer to index buffer
                device,
                command_buffer,
                ib,
                i_alloc,
                offset as u64,
            )?;
            offset += mesh.indices_size;
        }

        Ok(s_id)
    }

    // Returns the staging buffer to free at the end
    unsafe fn process_textures(&mut self, device: &Device, command_buffer: &CommandBuffer, resource_manager: &mut ResourceManager) -> Result<usize> {
        let sb = &mut resource_manager.staging_buffer;
        let queue_family_indices = device.queue_family_indices();

        // Create staging buffer for textures and copy data into it
        let mut size = 0;
        for texture in &self.textures {
            size += texture.pixels.len() * size_of::<u8>();
        }
        let s_id = sb.alloc(size)?; // Staging buffer id
        
        // Upload data
        let mut offset = 0;
        for tex_data in &self.textures {
            sb.copy_to_staging_at(s_id, tex_data.pixels.as_ref(), offset)?;
            offset += tex_data.pixels.len() * size_of::<u8>();
        }
        offset = 0;
        for tex_data in &self.textures {
            let id = resource_manager.textures.new_texture(device, tex_data)?;
            self.resolved_textures.push(id);
            let image = &resource_manager.textures[id].image;
            
            image.transition_image_layout(
                device,
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                None,
            )?;
            sb.copy_to_image_at(
                device,
                &command_buffer,
                s_id,
                image,
                offset as u64,
            )?;
            image.transition_image_layout(
                device,
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                Some(ImageMemoryBarrierQueueFamilyIndices {  // Release queue ownership
                    src_queue_family_index: queue_family_indices.transfer(),
                    dst_queue_family_index: queue_family_indices.graphics(),
                }),
            )?;
        }

        Ok(s_id)
    }

    unsafe fn ownership_transfer(&self, device: &Device, command_buffer: &CommandBuffer, resource_manager: &ResourceManager) -> Result<()> {
        let queue_family_indices = device.queue_family_indices();
        for id in &self.resolved_textures {
            let image = &resource_manager.textures[*id].image;
            image.transition_image_layout(
                device,
                command_buffer,
                vk::Format::R8G8B8A8_SRGB,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                Some(ImageMemoryBarrierQueueFamilyIndices {  // Acquire queue ownership
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
    pub fn resolve_mesh(&self, id: usize) -> Mesh {
        self.resolved_meshes[id]
    }

    pub fn resolve_texture(&self, id: usize) -> usize {
        // self.resolved_textures[id]
        0
    }

    pub unsafe fn destroy(&self, device: &Device) {
        device.logical().destroy_semaphore(self.semaphore, None);
    }
}