#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod app;
mod input;
mod camera;
mod ui;
mod world;
mod chunk;
mod block;

use std::{collections::HashSet, time::{Duration, Instant}};

use anyhow::Result;
use log::*;
use winit::{
    dpi::LogicalSize,
    event::{DeviceEvent, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
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

    event_loop.run(move |event, elwt| {
        match event {
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                app.mouse_cursor_update(delta);
            },
            Event::AboutToWait => {
                let now = Instant::now();
                if now.duration_since(tick).as_millis() >= TICK_RATE {
                    let dt = now.duration_since(tick).as_secs_f32();
                    unsafe {
                        app.handle_input(&window, dt);
                        app.update_camera(dt);
                        app.update(&window);
                    }
                    tick = now;
                    window.request_redraw();
                }
                elwt.set_control_flow(ControlFlow::WaitUntil(tick + Duration::from_millis(TICK_RATE as u64)));
            },
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::MouseInput { device_id, state, button } => {
                    app.mouse_button_update(button, state);
                },
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        if event.state.is_pressed() && code == KeyCode::Escape {
                            let _ = window.set_cursor_grab(CursorGrabMode::None);
                            window.set_cursor_visible(true);
                            elwt.exit();
                            unsafe { app.destroy() }
                            return;
                        }
                        app.keyboard_update(code, event.state);
                    }
                },
                WindowEvent::RedrawRequested if !elwt.exiting() && !minimized => unsafe {
                    app.render(&window).expect("Failed to render.");
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
