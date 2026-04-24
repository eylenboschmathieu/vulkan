#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::ops::Index;

use log::*;
use anyhow::{anyhow, Result};
use winit::window::Window;
use vulkanalia::{
    vk::{
        self, DeviceV1_0, Handle, HasBuilder, KhrSwapchainExtensionDeviceCommands, SwapchainKHR
    }
};

use crate::{
    Device,
    Instance,
    instance::{SwapchainSupport, QueueFamilyIndices},
    renderpass::Renderpass,
};

#[derive(Debug)]
pub struct Swapchain {
    device: Device,
    handle: SwapchainKHR,
    images: Vec<Image>,
    format: vk::Format,
    extent: vk::Extent2D,
}

impl Swapchain {
    pub unsafe fn new(window: &Window, instance: &Instance, device: &Device) -> Result<Self> {
        let swapchain_support = SwapchainSupport::get(instance, device.physical())?;

        let format = swapchain_support.get_surface_format().format;
        let extent = swapchain_support.get_extent(window);

        let handle = Swapchain::build(window, instance, device, &swapchain_support, None)?;

        let mut this = Self { device: device.clone(), handle, images: vec![], format, extent };
        this.get_images();

        Ok(this)
    }

    pub unsafe fn destroy(&mut self) {
        self.free_images();
        self.device.logical().destroy_swapchain_khr(self.handle, None);
        self.handle = vk::SwapchainKHR::null();
        info!("~ Handle");
    }

    unsafe fn build(window: &Window, instance: &Instance, device: &Device, support: &SwapchainSupport, old_swapchain: Option<vk::SwapchainKHR>) -> Result<vk::SwapchainKHR> {
        let indices = QueueFamilyIndices::get(instance, device.physical())?;

        let mut image_count = support.capabilities().min_image_count + 1;
        if support.capabilities().max_image_count != 0 && image_count > support.capabilities().max_image_count {
            image_count = support.capabilities().max_image_count;
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
            .surface(instance.surface())
            .min_image_count(image_count)
            .image_format(support.get_surface_format().format)
            .image_color_space(support.get_surface_format().color_space)
            .image_extent(support.get_extent(window))
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(image_sharing_mode)
            .queue_family_indices(&queue_family_indices)
            .pre_transform(support.capabilities().current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(support.get_present_mode())
            .clipped(true)
            .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));
        
        let result = device.logical().create_swapchain_khr(&info, None);

        if let Some(old_swapchain) = old_swapchain {
            device.logical().destroy_swapchain_khr(old_swapchain, None);
        }

        info!("+ Handle");

        match result {
            Ok(handle) => Ok(handle),
            Err(err) => return Err(anyhow!(err))
        }
    }

    pub unsafe fn rebuild(&mut self, window: &Window, instance: &Instance) -> Result<()> {
        // Destroy old swapchain
        self.free_images();

        let swapchain_support = SwapchainSupport::get(instance, self.device.physical())?;
        self.handle = Swapchain::build(window, instance, &self.device, &swapchain_support, Some(self.handle))?;

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
        let components = vk::ComponentMapping::builder()
            .r(vk::ComponentSwizzle::IDENTITY)
            .g(vk::ComponentSwizzle::IDENTITY)
            .b(vk::ComponentSwizzle::IDENTITY)
            .a(vk::ComponentSwizzle::IDENTITY);

        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);

        let images = self.device.logical()
            .get_swapchain_images_khr(self.handle)
            .unwrap_or_else(|err| panic!("{err}"));

        self.images.resize(images.len(), Image::default());

        for (image, img) in std::iter::zip(&mut self.images, images) {
            let info = vk::ImageViewCreateInfo::builder()
                .image(img)
                .view_type(vk::ImageViewType::_2D)
                .format(self.format)
                .components(components)
                .subresource_range(subresource_range);

            image.handle = img;
            image.view = self.device.logical().create_image_view(&info, None).unwrap_or_else(|err| panic!("{err}"));
        }

        info!("+ Image::Handles");
        info!("+ Image::Views");
    }

    pub unsafe fn create_framebuffers(&mut self, renderpass: &Renderpass) {
        let handle = renderpass.handle();
        self.images.iter_mut().for_each(|image| {
            let attachments = &[image.view];
            let info = vk::FramebufferCreateInfo::builder()
                .render_pass(handle)
                .attachments(attachments)
                .width(self.extent.width)
                .height(self.extent.height)
                .layers(1);

            image.framebuffer = self.device.logical().create_framebuffer(&info, None).unwrap();
        });
        info!("+ Image::Framebuffers")
    }

    /// Destroys views and framebuffers
    unsafe fn free_images(&mut self) {
        self.images
            .iter_mut()
            .for_each(|img| {
                self.device.logical().destroy_framebuffer(img.framebuffer, None);
                self.device.logical().destroy_image_view(img.view, None);
                // VkImage handles don't need cleaning up. Owned by VkSwapchain
            });
        self.images.clear();
        info!("~ Image::Framebuffers");
        info!("~ Image::Views");
        info!("~ Image::Handles")
    }
}

impl Index<usize> for Swapchain {
    type Output = Image;

    // Index must be in range of [0, PoolSize-1]
    fn index(&self, index: usize) -> &Self::Output  {
        &self.images[index]
    }
}

#[derive(Clone, Debug, Default)]
pub struct Image {
    handle: vk::Image,
    view: vk::ImageView,
    framebuffer: vk::Framebuffer,
}

impl Image {
    pub fn framebuffer(&self) -> vk::Framebuffer {
        self.framebuffer
    }
}