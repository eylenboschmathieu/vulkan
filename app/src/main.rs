#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

mod app;
mod input;
mod camera;
mod ui;
mod world;
mod chunk;
mod block;

use std::{time::{Duration, Instant}};

use anyhow::Result;
use log::*;
use winit::{
    dpi::LogicalSize,
    event::{DeviceEvent, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    window::{CursorGrabMode, Window, WindowBuilder},
};

use app::{App, AppEvent};

const TICK_RATE: u128 = 1000 / 60;  // In milliseconds

fn main() -> Result<()> {
    pretty_env_logger::init();

    // Window

    let event_loop: EventLoop<()> = EventLoop::new()?;
    let window: Window = WindowBuilder::new()
        .with_title("Playground")
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
                app.mouse_motion((delta.0 as f32, delta.1 as f32));
            },
            Event::AboutToWait => {
                let now = Instant::now();
                if now.duration_since(tick).as_millis() >= TICK_RATE {
                    let dt = now.duration_since(tick).as_secs_f32();
                    unsafe {
                        if let Some(AppEvent::Exit) = app.handle_input(&window) {
                            let _ = window.set_cursor_grab(CursorGrabMode::None);
                            window.set_cursor_visible(true);
                            elwt.exit();
                            return;
                        }
                        app.update(&window, dt);
                    }
                    tick = now;
                    window.request_redraw();
                }
                elwt.set_control_flow(ControlFlow::WaitUntil(tick + Duration::from_millis(TICK_RATE as u64)));
            },
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CursorMoved { position, .. } => {
                    app.cursor_moved(position.x as f32, position.y as f32);
                },
                WindowEvent::MouseInput { device_id, state, button } => {
                    app.button_update(button, state);
                },
                WindowEvent::KeyboardInput { event, .. } => {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        app.button_update(code, event.state);
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
                }
                _ => {}
            }
            _ => {}
        }
    })?;

    Ok(())
}
