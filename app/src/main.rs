#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod app;
mod camera;
mod sun;
mod world;
mod chunk;
mod block;

use std::{collections::HashSet, time::Instant};

use anyhow::Result;
use log::*;
use winit::{
    dpi::LogicalSize,
    event::{DeviceEvent, Event, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowBuilder},
};

use app::App;

const TICK_RATE: u128 = 1000 / 60;  // In milliseconds

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

    window.set_cursor_grab(CursorGrabMode::Locked)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
        .expect("Failed to grab cursor");
    window.set_cursor_visible(false);

    let mut minimized: bool = false;
    let mut tick = Instant::now();
    let mut keys: HashSet<KeyCode> = HashSet::new();

    event_loop.run(move |event, elwt| {
        match event {
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                app.mouse_move(delta.0 as f32, delta.1 as f32);
            },
            Event::AboutToWait => window.request_redraw(),
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        if code == KeyCode::Escape && event.state.is_pressed() {
                            let _ = window.set_cursor_grab(CursorGrabMode::None);
                            window.set_cursor_visible(true);
                            elwt.exit();
                            unsafe { app.destroy() }
                            return;
                        }

                        if event.state.is_pressed() {
                            keys.insert(code);
                        } else {
                            keys.remove(&code);
                        }
                    }
                },
                WindowEvent::RedrawRequested if !elwt.exiting() && !minimized => unsafe {
                    let now = Instant::now();
                    if now.duration_since(tick).as_millis() > TICK_RATE {
                        let dt = now.duration_since(tick).as_secs_f32();
                        app.input(&keys, dt);
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
                WindowEvent::CloseRequested => {
                    let _ = window.set_cursor_grab(CursorGrabMode::None);
                    window.set_cursor_visible(true);
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
