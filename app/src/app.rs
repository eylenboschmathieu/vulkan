#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use log::*;
use anyhow::Result;
use winit::window::Window;
use blitz::Blitz;

// Our Vulkan app.
#[derive(Debug)]
pub struct App{
    blitz: Blitz,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let blitz = blitz::init(window)?;

        blitz.upload()?;
        blitz.record()?;

        info!("+ App");
        Ok(Self { blitz })
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        self.blitz.render(window).expect("Rendering failed");
        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.blitz.destroy();
        info!("~ App");
    }
}