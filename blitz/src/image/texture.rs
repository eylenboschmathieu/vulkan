#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    fs::File, ops::Deref,
};

use log::*;
use vulkanalia::vk;
use anyhow::Result;

use crate::{
    buffers::{
        buffer::TransferDst,
        staging_buffer::StagingBuffer,
    }, context::Context, device::Device, image::Image
};

#[derive(Debug)]
pub struct Texture {
    image: Image,
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
        info!("+ Texture");

        Ok(Self { image })
    }

    pub unsafe fn destroy(&self, device: &Device) {
        self.image.destroy(device);
        info!("~ Texture")
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