#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    fs::File, ops::Deref,
};

use log::*;
use vulkanalia::vk::{self, DeviceV1_0, HasBuilder};
use anyhow::Result;

use crate::{
    buffers::{
        buffer::TransferDst,
    }, context::Context, device::Device, image::Image
};

#[derive(Debug)]
pub struct Texture {
    image: Image,
    view: vk::ImageView,
    sampler: vk::Sampler,
}

impl Texture {
    pub unsafe fn new(context: &Context, path: &str) -> Result<Self> {
        let file = File::open(path)?;

        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info()?;

        let mut pixels = vec![0; reader.info().raw_bytes()];
        reader.next_frame(&mut pixels)?;

        let size = reader.info().raw_bytes() as u64;
        let (width, height) = reader.info().size();

        // Handle + Memory

        let image = Image::new(
            context,
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        context.transfer_manager.buffer_to_image(context, &pixels, size, &image)?;

        let view = Image::build_view(
            &context.device,
            image.handle(),
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageAspectFlags::COLOR
        )?;

        let sampler = Texture::build_sampler(&context.device)?;

        info!("+ Texture");

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

/*impl DerefMut for Texture {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.image
    }
}*/