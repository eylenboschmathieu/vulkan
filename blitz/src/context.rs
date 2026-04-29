#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use log::*;
use anyhow::{anyhow, Result};
use vulkanalia::{
    Entry, loader::{LIBRARY, LibloadingLoader},
};
use winit::window::Window;

use crate::{
    commands::CommandManager,
    device::Device,
    instance::Instance,
    queues::QueueManager,
    resources::resource_manager::ResourceManager,
};

#[derive(Debug)]
pub struct Context {
    entry: Entry,
    pub instance: Instance,
    pub device: Device,
    pub queue_manager: QueueManager,
    pub command_manager: CommandManager,
    pub resource_manager: ResourceManager,
}

impl Context {
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let loader = LibloadingLoader::new(LIBRARY)?;
        let entry = Entry::new(loader).map_err(|b| anyhow!("{b}"))?;

        let instance = Instance::new(window, &entry)?;
        let device = Device::new(&entry, window, &instance)?;
        let queue_manager = QueueManager::new(&device)?;
        let command_manager = CommandManager::new(&instance, &device)?;
        let resource_manager = ResourceManager::new(&device)?;

        Ok(Self { entry, instance, device, queue_manager, command_manager, resource_manager })
    }

    pub unsafe fn destroy(&mut self) {
        self.command_manager.destroy(&self.device);
        self.resource_manager.destroy(&self.device);
        self.device.destroy();
        self.instance.destroy();
        info!("~ Context");
    }
}