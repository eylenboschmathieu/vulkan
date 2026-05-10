#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::Index;

use log::*;
use anyhow::{anyhow, Result};
use winit::window::Window;
use vulkanalia::{
    vk::{
        self,
        *
    }
};

use crate::{
    globals,
    resources::image::{DepthBuffer, Image}, pipeline::renderpass::Renderpass
};

#[derive(Debug)]
pub struct Swapchain {
    handle: SwapchainKHR,
    images: Vec<SwapchainImage>,
    format: vk::Format,
    extent: vk::Extent2D,
}

impl Swapchain {
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let swapchain_support = globals::device().swapchain_support();

        let format = swapchain_support.get_surface_format().format;
        let extent = swapchain_support.get_extent(window);

        let handle = Swapchain::build(window, None)?;

        let mut this = Self { handle, images: vec![], format, extent };
        this.get_images();

        Ok(this)
    }

    pub unsafe fn destroy(&mut self) {
        self.free_images();
        globals::device().logical().destroy_swapchain_khr(self.handle, None);
        self.handle = vk::SwapchainKHR::null();
        info!("~ Handle");
    }

    unsafe fn build(window: &Window, old_swapchain: Option<vk::SwapchainKHR>) -> Result<vk::SwapchainKHR> {
        let indices = globals::device().queue_family_indices();
        let swapchain_support = globals::device().swapchain_support();

        let mut image_count = swapchain_support.capabilities().min_image_count + 1;
        if swapchain_support.capabilities().max_image_count != 0 && image_count > swapchain_support.capabilities().max_image_count {
            image_count = swapchain_support.capabilities().max_image_count;
        }
        
        let mut queue_family_indices = vec![];
        let image_sharing_mode = if indices.graphics() != indices.present() {
            queue_family_indices.push(indices.graphics());
            queue_family_indices.push(indices.present());
            vk::SharingMode::CONCURRENT
        } else {
            vk::SharingMode::EXCLUSIVE
        };  

        let info = vk::SwapchainCreateInfoKHR::builder()
            .surface(globals::instance().surface())
            .min_image_count(image_count)
            .image_format(swapchain_support.get_surface_format().format)
            .image_color_space(swapchain_support.get_surface_format().color_space)
            .image_extent(swapchain_support.get_extent(window))
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(image_sharing_mode)
            .queue_family_indices(&queue_family_indices)
            .pre_transform(swapchain_support.capabilities().current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(swapchain_support.get_present_mode())
            .clipped(true)
            .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));

        let result = globals::device().logical().create_swapchain_khr(&info, None);

        if let Some(old_swapchain) = old_swapchain {
            globals::device().logical().destroy_swapchain_khr(old_swapchain, None);
        }

        info!("+ Handle");

        match result {
            Ok(handle) => Ok(handle),
            Err(err) => return Err(anyhow!(err))
        }
    }

    pub unsafe fn rebuild(&mut self, window: &Window) -> Result<()> {
        self.free_images();

        self.handle = Swapchain::build(window, Some(self.handle))?;

        let support = globals::device().swapchain_support();
        self.format = support.get_surface_format().format;
        self.extent = support.get_extent(window);

        self.get_images();

        Ok(())
    }

    pub fn handle(&self) -> vk::SwapchainKHR {
        self.handle
    }

    pub fn framebuffer_count(&self) -> usize {
        self.images.len()
    }

    pub unsafe fn format(&self) -> vk::Format {
        self.format
    }

    pub unsafe fn extent(&self) -> vk::Extent2D {
        self.extent
    }

    /// Get the image handles and views from the swapchain
    pub unsafe fn get_images(&mut self) {
        let images = globals::device().logical()
            .get_swapchain_images_khr(self.handle)
            .unwrap_or_else(|err| panic!("{err}"));

        self.images.resize(images.len(), SwapchainImage::default());

        for (image, handle) in std::iter::zip(&mut self.images, images) {
            image.handle = handle;
            image.view = Image::build_view(handle, self.format, vk::ImageAspectFlags::COLOR).unwrap_or_else(|err| panic!("{err}"));
        }

        info!("+ Image::Handles");
        info!("+ Image::Views");
    }

    pub unsafe fn create_framebuffers(&mut self, renderpass: &Renderpass, depth_buffer: &DepthBuffer) {
        self.images.iter_mut().for_each(|image| {
            let attachments = &[image.view, depth_buffer.view()];
            let info = vk::FramebufferCreateInfo::builder()
                .render_pass(renderpass.handle())
                .attachments(attachments)
                .width(self.extent.width)
                .height(self.extent.height)
                .layers(1);

            image.framebuffer = globals::device().logical().create_framebuffer(&info, None).unwrap();
        });
        info!("+ Image::Framebuffers")
    }

    /// Destroys views and framebuffers
    unsafe fn free_images(&mut self) {
        self.images
            .iter_mut()
            .for_each(|img| {
                globals::device().logical().destroy_framebuffer(img.framebuffer, None);
                globals::device().logical().destroy_image_view(img.view, None);
                // VkImage handles don't need cleaning up. Owned by VkSwapchain
            });
        self.images.clear();
        info!("~ Image::Framebuffers");
        info!("~ Image::Views");
        info!("~ Image::Handles")
    }
}

impl Index<usize> for Swapchain {
    type Output = SwapchainImage;

    // Index must be in range of [0, PoolSize-1]
    fn index(&self, index: usize) -> &Self::Output  {
        &self.images[index]
    }
}

#[derive(Clone, Debug, Default)]
pub struct SwapchainImage {
    handle: vk::Image,
    view: vk::ImageView,
    framebuffer: vk::Framebuffer,
}

impl SwapchainImage {
    pub fn framebuffer(&self) -> vk::Framebuffer {
        self.framebuffer
    }
}