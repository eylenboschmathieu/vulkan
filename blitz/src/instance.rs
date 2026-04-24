#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{
    collections::HashSet,
    ffi::CStr,
    os::raw::c_void
};
use anyhow::{anyhow, Result};
use log::*;
use winit::window::Window;
use vulkanalia::{
    Version,
    Instance as vk_instance,
    Entry,
    prelude::v1_0::*,
    vk::{
        SurfaceKHR as vk_surface,
        PhysicalDevice,
        KhrSurfaceExtensionInstanceCommands,
        DebugUtilsMessengerEXT,
        ExtDebugUtilsExtensionInstanceCommands
    },
    window as vk_window
};

use crate::*;

pub const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);
pub const VALIDATION_LAYER: vk::ExtensionName = vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");
pub const DEVICE_EXTENSIONS: &[vk::ExtensionName] = &[vk::KHR_SWAPCHAIN_EXTENSION.name];

extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void) -> vk::Bool32 {
        let data = unsafe { *data };
        let message = unsafe { CStr::from_ptr(data.message) }.to_string_lossy();

        if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
            error!("({:?}) {}", type_, message);
        } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
            warn!("({:?}) {}", type_, message)
        } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::INFO {
            debug!("({:?}) {}", type_, message);
        } else {
            trace!("({:?}) {}", type_, message);
        }

        vk::FALSE
    }

/// Contains a vulkan instance, and optionally a DebugUtilsMessengerEXT
#[derive(Debug)]
pub struct Instance {
    handle: vk_instance,
    surface: vk_surface,
    messenger: Option<DebugUtilsMessengerEXT>,
}

impl Instance {
    /// The Vulkan handles and associated properties used by our Vulkan app.
    pub unsafe fn new(window: &Window, entry: &Entry) -> Result<Self> {
        // Application Info

        let app_info = vk::ApplicationInfo::builder()
            .application_name(b"Vulkan\0")
            .application_version(vk::make_version(1, 0, 0))
            .engine_name(b"No Engine\0")
            .engine_version(vk::make_version(1, 0, 0))
            .api_version(vk::make_version(1, 0, 0));

        // Validation Layers

        let available_layers = entry.enumerate_instance_layer_properties()?
            .iter()
            .map(|l| l.layer_name)
            .collect::<HashSet<_>>();

        if VALIDATION_ENABLED && !available_layers.contains(&VALIDATION_LAYER) {
            return Err(anyhow!("Validation layer requested but not supported."));
        }

        let layers = if VALIDATION_ENABLED { vec![VALIDATION_LAYER.as_ptr()] } else { Vec::new() };

        // Extensions

        let mut extensions = vk_window::get_required_instance_extensions(window)
            .iter()
            .map(|e| e.as_ptr())
            .collect::<Vec<_>>();

        if VALIDATION_ENABLED {
            extensions.push(vk::EXT_DEBUG_UTILS_EXTENSION.name.as_ptr());
        }

        let flags = if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
            info!("Enabling extensions for macOS portability");
            extensions.push(vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION.name.as_ptr());
            extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name.as_ptr());
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::empty()
        };

        // Instance creation debugging

        let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL |
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION |
                vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
            )
            .user_callback(Some(debug_callback));

        // Create

        let mut info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions)
            .flags(flags);

        if VALIDATION_ENABLED {
            info = info.push_next(&mut debug_info);
        }

        let instance = entry.create_instance(&info, None)?;
        info!("+ Handle");

        // Debug
        let messenger = if VALIDATION_ENABLED {
            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL |
                    vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION |
                    vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE)
                .user_callback(Some(debug_callback));
            info!("+ DebugMessenger");
            Some(instance.create_debug_utils_messenger_ext(&debug_info, None)?)
        } else {
            None
        };

        // Surface

        let surface = vk_window::create_surface(&instance, &window, &window)?;
        info!("+ Surface");

        Ok(Instance { handle: instance, messenger, surface })
    }

    pub unsafe fn destroy(&self) {
        self.handle.destroy_surface_khr(self.surface, None);
        info!("~ Surface");

        if let Some(messenger) = self.messenger {
            self.handle.destroy_debug_utils_messenger_ext(messenger, None);
            info!("~ DebugMessenger")
        };

        self.handle.destroy_instance(None);
        info!("~ Handle");
    }

    pub fn handle(&self) -> &vk_instance {
        &self.handle
    }

    pub fn surface(&self) -> vk_surface {
        return self.surface
    }

    pub unsafe fn pick_physical_device(&self) -> Result<(PhysicalDevice, QueueFamilyIndices)> {
        for physical_device in self.handle.enumerate_physical_devices()? {
            let properties = self.handle.get_physical_device_properties(physical_device);

            match self.check_physical_device(physical_device) {
                Ok(indices) => {
                    return Ok((physical_device, indices))
                },
                Err(err) => warn!("Skipping physical device('{}'): {}", properties.device_name, err)
            }
        }
        info!("pick_physical_device.end");
        
        Err(anyhow!("Failed to find suitable physical device."))
    }

    unsafe fn check_physical_device(&self, physical_device: PhysicalDevice) -> Result<QueueFamilyIndices> {
        let properties = self.handle.get_physical_device_properties(physical_device);
        if properties.device_type != vk::PhysicalDeviceType::DISCRETE_GPU {
            return Err(anyhow!(SuitabilityError("Only discrete GPUs are supported.")));
        }

        let features = self.handle.get_physical_device_features(physical_device);
        if features.geometry_shader != vk::TRUE {
            return Err(anyhow!(SuitabilityError("Missing geometry shader support.")));
        }

        self.check_physical_device_extensions(physical_device)?;
        let queue_family_indices = QueueFamilyIndices::get(self, physical_device)?;
        let swapchain_support = SwapchainSupport::get(self, physical_device)?;
        if swapchain_support.formats.is_empty() || swapchain_support.present_modes.is_empty() {
            return Err(anyhow!(SuitabilityError("Insufficient swapchain support.")));
        }

        Ok(queue_family_indices)
    }

    unsafe fn check_physical_device_extensions(&self, physical_device: PhysicalDevice) -> Result<()> {
        let extensions = self.handle.enumerate_device_extension_properties(physical_device, None)?
            .iter()
            .map(|e| e.extension_name)
            .collect::<HashSet<_>>();

        if DEVICE_EXTENSIONS.iter().all(|e| {
            extensions.contains(e)
        }) {
            Ok(())
        } else {
            Err(anyhow!(SuitabilityError("Missing required device extensions.")))
        }
    }

}

#[derive(Clone, Debug)]
pub struct QueueFamilyIndices {
    graphics: u32,
    transfer: u32,
    present: u32,
}

impl QueueFamilyIndices {
    pub unsafe fn get(instance: &Instance, physical_device: PhysicalDevice) -> Result<Self> {
        let properties = instance.handle().get_physical_device_queue_family_properties(physical_device);

        let graphics = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        let transfer = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::TRANSFER) && !p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        let mut present = None;
        for (index, properties) in properties.iter().enumerate() {
            if instance.handle().get_physical_device_surface_support_khr(physical_device, index as u32, instance.surface())? {
                present = Some(index as u32);
                break;
            }
        }

        if let (Some(graphics), Some(transfer), Some(present)) = (graphics, transfer, present) {
            Ok(Self { graphics, transfer, present })
        } else {
            Err(anyhow!(SuitabilityError("Missing required queue families.")))
        }
    }

    pub fn graphics(&self) -> u32 {
        self.graphics
    }

    pub fn transfer(&self) -> u32 {
        self.transfer
    }

    pub fn present(&self) -> u32 {
        self.present
    }
}

#[derive(Clone, Debug)]
pub struct SwapchainSupport {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}

impl SwapchainSupport {
    pub unsafe fn get(instance: &Instance, physical_device: PhysicalDevice) -> Result<Self> {
        Ok(Self {
            capabilities: instance.handle.get_physical_device_surface_capabilities_khr(physical_device, instance.surface)?,
            formats: instance.handle.get_physical_device_surface_formats_khr(physical_device, instance.surface)?,
            present_modes: instance.handle.get_physical_device_surface_present_modes_khr(physical_device, instance.surface)?,
        })
    }

    pub fn capabilities(&self) -> vk::SurfaceCapabilitiesKHR {
        return self.capabilities;
    }

    pub fn formats(&self) -> &Vec<vk::SurfaceFormatKHR> {
        return &self.formats
    }

    pub fn present_modes(&self) -> &Vec<vk::PresentModeKHR> {
        return &self.present_modes
    }

    pub unsafe fn get_surface_format(&self) -> vk::SurfaceFormatKHR {
        self.formats
            .iter()
            .cloned()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_SRGB && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            }).unwrap_or_else(|| self.formats[0])
    }

    pub unsafe fn get_present_mode(&self) -> vk::PresentModeKHR {
        self.present_modes
            .iter()
            .cloned()
            .find(|m| *m == vk::PresentModeKHR::MAILBOX)
            .unwrap_or(vk::PresentModeKHR::FIFO)
    }

    pub unsafe fn get_extent(&self, window: &Window) -> vk::Extent2D {
        if self.capabilities.current_extent.width != u32::MAX {
            self.capabilities.current_extent
        } else {
            vk::Extent2D::builder()
                .width(window.inner_size().width.clamp(
                    self.capabilities.min_image_extent.width,
                    self.capabilities.max_image_extent.width
                ))
                .height(window.inner_size().height.clamp(
                    self.capabilities.min_image_extent.height,
                    self.capabilities.max_image_extent.height
                ))
                .build()
        }
    }
}