#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod app;
mod world;
mod chunk;
mod block;

use std::time::Instant;

use anyhow::Result;
use log::*;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::{Window, WindowBuilder}
};

use app::App;

const TICK_RATE: u128 = 1000 / 60;  // In miliseconds

fn main() -> Result<()> {
    pretty_env_logger::init();

    // Window

    let event_loop: EventLoop<()> = EventLoop::new()?;
    let window: Window = WindowBuilder::new()
        .with_title("Vulkan Tutorial (Rust)")
        .with_inner_size(LogicalSize::new(1024, 768))
        .build(&event_loop)?;

    // App

    let mut app: App = unsafe { App::new(&window)? };

    let mut minimized: bool = false;
    let mut tick = Instant::now();

    event_loop.run(move |event, elwt| {
        match event {
            // Request a redraw when all events were processed.
            Event::AboutToWait => window.request_redraw(),
            Event::WindowEvent { event, .. } => match event {
                // Render a frame if our Vulkan app is not being destroyed.
                WindowEvent::RedrawRequested if !elwt.exiting() && !minimized => unsafe {
                    // Only render once a second
                    let now = Instant::now();
                    if now.duration_since(tick).as_millis() > TICK_RATE {
                        app.render(&window).expect("Failed to render.");
                        tick = now;
                    }
                },
                WindowEvent::Resized(size) => {
                    info!("WindowEvent::Resized");
                    if size.width == 0 || size.height == 0 {
                        minimized = true;
                    } else {
                        minimized = false;
                    }
                },
                // Destroy our Vulkan app.
                WindowEvent::CloseRequested => {
                    elwt.exit();
                    unsafe { app.destroy(); }
                }
                _ => {}
            }
            _ => {}
        }
    })?;

    Ok(())
}