#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::collections::HashSet;
use log::*;
use anyhow::Result;
use winit::window::Window;
use vulkanalia::{
    Device as LogicalDevice,
    Entry,
    prelude::v1_0::*,
    vk::{PhysicalDevice, PhysicalDeviceMemoryProperties}
};

use crate::{
    instance::{
        VALIDATION_ENABLED,
        DEVICE_EXTENSIONS,
        Instance,
        PORTABILITY_MACOS_VERSION,
        QueueFamilyIndices,
        SwapchainSupport,
        VALIDATION_LAYERS,
    },
};

#[derive(Clone, Debug)]
pub struct Device {
    physical_device: PhysicalDevice,
    logical_device: LogicalDevice,
    queue_family_indices: QueueFamilyIndices,
    memory_properties: PhysicalDeviceMemoryProperties,
    swapchain_support: SwapchainSupport,
}

impl Device {
    pub unsafe fn new(entry: &Entry, window: &Window, instance: &Instance) -> Result<Self> {
        let (physical_device, queue_family_indices) = instance.pick_physical_device()?;
        let logical_device = Device::create_logical_device(&entry, instance, physical_device, &queue_family_indices)?;
        let memory_properties = instance.handle().get_physical_device_memory_properties(physical_device);
        let swapchain_support = SwapchainSupport::get(instance, physical_device)?;

        Ok(Self {
            physical_device,
            logical_device,
            queue_family_indices,
            memory_properties,
            swapchain_support,
        })
    }

    pub unsafe fn destroy(&self) {
        self.logical_device.destroy_device(None);
        info!("~ Handle")
    }

    pub fn physical(&self) -> PhysicalDevice {
        self.physical_device
    }

    pub fn logical(&self) -> &LogicalDevice {
        &self.logical_device
    }

    pub fn queue_family_indices(&self) -> QueueFamilyIndices {
        self.queue_family_indices.clone()
    }

    pub fn memory_properties(&self) -> PhysicalDeviceMemoryProperties {
        self.memory_properties
    }

    pub fn swapchain_support(&self) -> SwapchainSupport {
        self.swapchain_support.clone()
    }

    pub unsafe fn refresh_swapchain_support(&mut self, instance: &Instance) -> Result<()> {
        self.swapchain_support = SwapchainSupport::get(instance, self.physical_device)?;
        Ok(())
    }

    unsafe fn create_logical_device(entry: &Entry, instance: &Instance, physical_device: PhysicalDevice, indices: &QueueFamilyIndices) -> Result<LogicalDevice> {
        
        // Queues

        let mut unique_indices = HashSet::new();
        unique_indices.insert(indices.graphics());
        unique_indices.insert(indices.transfer());
        unique_indices.insert(indices.present());

        let queue_priorities = &[1.0];
        let queue_infos = unique_indices.iter().map(|i| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*i)
                .queue_priorities(queue_priorities)
        }).collect::<Vec<_>>();

        // Layers
        
        let layers: Vec<*const i8> = if VALIDATION_ENABLED {
            VALIDATION_LAYERS
                .iter()
                .map(|layer| layer.as_ptr())
                .collect()
        } else {
            Vec::new()
        };

        // Extensions

        let mut extensions = DEVICE_EXTENSIONS
            .iter()
            .map(|n| n.as_ptr())
            .collect::<Vec<_>>();

        // Required by Vulkan SDK since 1.3.216.
        if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
            extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
        }

        // Features

        let mut vulkan13_features = vk::PhysicalDeviceVulkan13Features::builder()
            .synchronization2(true);
        let core_features = vk::PhysicalDeviceFeatures::builder()
            .sampler_anisotropy(true);
        let mut features = vk::PhysicalDeviceFeatures2::builder()
            .features(core_features)
            .push_next(&mut vulkan13_features);

        // Create

        let info= vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_infos)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions)
            .push_next(&mut features);

        let logical_device = instance.handle().create_device(physical_device, &info, None)?;
        info!("+ Handle");

        Ok(logical_device)
    }
}
