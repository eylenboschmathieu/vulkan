#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    ops::{Deref, Index},
};

use log::*;
use vulkanalia::vk::{self, DeviceV1_0, Handle, HasBuilder};
use anyhow::Result;

use crate::{
    TextureData, globals, pipeline::{
        DescriptorSetLayout,
        descriptor_set_layout::DescriptorSetLayoutBuildInfo,
    }, resources::{
        buffers::buffer::TransferDst,
        image::Image,
    }
};

pub type TextureId = usize;

#[derive(Debug)]
pub(crate) struct Texture {
    pub(crate) image: Image,
    pub(crate) view: vk::ImageView,
    pub(crate) sampler: vk::Sampler,
    pub(crate) descriptor_set: vk::DescriptorSet,
}

impl Texture {
    pub unsafe fn new(layout: vk::DescriptorSetLayout, width: u32, height: u32) -> Result<Self> {
        let image = Image::new(
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        let view = Image::build_view(
            image.handle(),
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageAspectFlags::COLOR
        )?;

        let sampler = Texture::build_sampler()?;

        let descriptor_set = globals::descriptor_pool_mut().alloc(layout, 1)?[0];

        Ok(Self { image, view, sampler, descriptor_set })
    }

    pub unsafe fn destroy(&self) {
        globals::device().logical().destroy_sampler(self.sampler, None);
        globals::device().logical().destroy_image_view(self.view, None);
        self.image.destroy();
        info!("~ Texture")
    }

    unsafe fn build_sampler() -> Result<vk::Sampler> {
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

        let sampler = globals::device().logical().create_sampler(&create_info, None)?;

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
    pub descriptor_set_layout: vk::DescriptorSetLayout,
}

impl Textures {
    pub unsafe fn new() -> Result<Self> {
        let build_info = vec![
            DescriptorSetLayoutBuildInfo { binding: 0, count: 1, descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER }, // Texture
        ];
        let descriptor_set_layout = DescriptorSetLayout::alloc(  // Texture descriptor set layout
            build_info,
        )?;

        Ok(Self {
            textures: vec![],
            free_ids: vec![],
            descriptor_set_layout,
        })
    }

    pub unsafe fn destroy(&mut self) {
        for texture in &self.textures {
            texture.destroy();
        }
        DescriptorSetLayout::free(self.descriptor_set_layout);
        self.descriptor_set_layout = vk::DescriptorSetLayout::null();
    }

    pub unsafe fn new_texture(&mut self, data: &TextureData) -> Result<usize> {
        let texture = Texture::new(self.descriptor_set_layout, data.width, data.height)?;
        if let Some(id) = self.free_ids.pop() {
            self.textures[id] = texture;
            return Ok(id);
        }

        self.textures.push(texture);
        Ok(self.textures.len() - 1)
    }

    pub(crate) unsafe fn delete_texture(&mut self, id: TextureId) {
        self.textures[id].destroy();
        self.free_ids.push(id);
    }
}

impl Index<usize> for Textures {
    type Output = Texture;

    fn index(&self, index: usize) -> &Self::Output {
        &self.textures[index]
    }
}