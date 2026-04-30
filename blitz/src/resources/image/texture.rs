#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ops::{Deref, Index},
};

use log::*;
use vulkanalia::vk::{self, DeviceV1_0, HasBuilder};
use anyhow::Result;

use crate::{
    TextureData, device::Device, resources::{
        buffers::buffer::TransferDst, image::Image
    },
};

pub type TextureId = usize;

#[derive(Debug)]
pub(crate) struct Texture {
    pub(crate) image: Image,
    pub(crate) view: vk::ImageView,
    pub(crate) sampler: vk::Sampler,
}

impl Texture {
    pub unsafe fn new(device: &Device, width: u32, height: u32) -> Result<Self> {
        let image = Image::new(
            device,
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        let view = Image::build_view(
            device,
            image.handle(),
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageAspectFlags::COLOR
        )?;

        let sampler = Texture::build_sampler(device)?;

        Ok(Self { image, view, sampler })
    }

    pub unsafe fn destroy(&self, device: &Device) {
        device.logical().destroy_sampler(self.sampler, None);
        device.logical().destroy_image_view(self.view, None);
        self.image.destroy(device);
        info!("~ Texture")
    }

    unsafe fn build_sampler(device: &Device) -> Result<vk::Sampler> {
        let create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)  // Requires enabling device feature 'sampler_anisotropy'
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);

        let sampler = device.logical().create_sampler(&create_info, None)?;

        Ok(sampler)
    }

    pub fn view(&self) -> vk::ImageView {
        self.view
    }

    pub fn sampler(&self) -> vk::Sampler {
        self.sampler
    }
}

impl TransferDst for Texture {}

impl Deref for Texture {
    type Target = Image;

    fn deref(&self) -> &Self::Target {
        &self.image
    }
}

#[derive(Debug)]
pub(crate) struct Textures {
    textures: Vec<Texture>,
    free_ids: Vec<usize>,
}

impl Textures {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            textures: vec![],
            free_ids: vec![],
        })
    }

    pub(crate) unsafe fn destroy(&self, device: &Device) {
        for texture in &self.textures {
            texture.destroy(device);
        }
    }

    pub(crate) unsafe fn new_texture(&mut self, device: &Device, data: &TextureData) -> Result<usize> {
        let texture = Texture::new(device, data.width, data.height)?;
        if self.free_ids.len() > 0 {
            let id = self.free_ids.pop().unwrap();
            self.textures[id] = texture;
            return Ok(id);
        }

        self.textures.push(texture);
        Ok(self.textures.len() - 1)
    }

    pub(crate) unsafe fn delete_texture(&mut self, device: &Device, id: TextureId) {
        self.textures[id].destroy(device);
        self.free_ids.push(id);
    }
}

impl Index<usize> for Textures {
    type Output = Texture;

    fn index(&self, index: usize) -> &Self::Output {
        &self.textures[index]
    }
}