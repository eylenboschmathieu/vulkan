#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::time::Instant;
use log::*;
use anyhow::Result;
use winit::window::Window;
use blitz::Blitz;

// Our Vulkan app.
#[derive(Debug)]
pub struct App{
    blitz: Blitz,
    delta: Instant,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        blitz.upload()?;
        blitz.record()?;

        info!("+ App");
        Ok(Self { blitz, delta: Instant::now() })
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        self.blitz.render(window, self.delta).expect("Rendering failed");
        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.blitz.destroy();
        info!("~ App");
    }
}