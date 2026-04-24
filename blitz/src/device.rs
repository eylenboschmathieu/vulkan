#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::collections::HashSet;
use log::*;
use anyhow::Result;
use winit::window::Window;
use vulkanalia::{
    Device as LogicalDevice,
    Entry,
    prelude::v1_0::*,
    vk::{PhysicalDevice}
};

use crate::{
    VALIDATION_ENABLED,
    instance::{
        DEVICE_EXTENSIONS, Instance, PORTABILITY_MACOS_VERSION, QueueFamilyIndices, VALIDATION_LAYER
    },
};

#[derive(Clone, Debug)]
pub struct Device {
    physical_device: PhysicalDevice,
    logical_device: LogicalDevice,
    queue_family_indices: QueueFamilyIndices,
}

impl Device {
    pub unsafe fn new(entry: &Entry, window: &Window, instance: &Instance) -> Result<Self> {
        let (physical_device, queue_family_indices) = instance.pick_physical_device()?;
        let logical_device = Device::create_logical_device(&entry, instance, physical_device, &queue_family_indices)?;

        Ok(Self {
            physical_device,
            logical_device,
            queue_family_indices,
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
        
        let layers = if VALIDATION_ENABLED {
            vec![VALIDATION_LAYER.as_ptr()]
        } else {
            vec![]
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

        let features = vk::PhysicalDeviceFeatures::builder();

        // Create

        let info: vk::DeviceCreateInfoBuilder<'_> = vk::DeviceCreateInfo::builder()
            .queue_create_infos(&queue_infos)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions)
            .enabled_features(&features);

        let logical_device = instance.handle().create_device(physical_device, &info, None)?;
        info!("+ Handle");

        Ok(logical_device)
    }
}
