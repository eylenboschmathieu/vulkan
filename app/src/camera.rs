use std::collections::HashSet;

use cgmath::{vec3, Deg, InnerSpace, Matrix4, Point3, Vector3};
use winit::keyboard::KeyCode;
use blitz::CameraUbo;

const MOVE_SPEED: f32 = 6.0;
const MOUSE_SENSITIVITY: f32 = 0.2; // degrees per pixel

#[derive(Debug)]
pub struct FpCamera {
    pub eye:   Point3<f32>,
    pub yaw:   f32, // degrees, horizontal
    pub pitch: f32, // degrees, vertical, clamped to ±89
}

impl FpCamera {
    pub fn new(eye: Point3<f32>, yaw: f32, pitch: f32) -> Self {
        Self { eye, yaw, pitch }
    }

    pub fn forward(&self) -> Vector3<f32> {
        let (sy, cy) = Deg(self.yaw).0.to_radians().sin_cos();
        let (sp, cp) = Deg(self.pitch).0.to_radians().sin_cos();
        vec3(cy * cp, sp, sy * cp).normalize()
    }

    pub fn right(&self) -> Vector3<f32> {
        let (sy, cy) = Deg(self.yaw).0.to_radians().sin_cos();
        vec3(-sy, 0.0, cy).normalize()
    }

    pub fn input(&mut self, keys: &HashSet<KeyCode>, dt: f32) {
        let fwd   = self.forward();
        let right = self.right();
        let up    = vec3(0.0_f32, 1.0, 0.0);

        if keys.contains(&KeyCode::ArrowUp)    { self.eye += fwd   * MOVE_SPEED * dt; }
        if keys.contains(&KeyCode::ArrowDown)  { self.eye -= fwd   * MOVE_SPEED * dt; }
        if keys.contains(&KeyCode::ArrowLeft)  { self.eye -= right * MOVE_SPEED * dt; }
        if keys.contains(&KeyCode::ArrowRight) { self.eye += right * MOVE_SPEED * dt; }
        if keys.contains(&KeyCode::Numpad1)    { self.eye += up    * MOVE_SPEED * dt; }
        if keys.contains(&KeyCode::Numpad2)    { self.eye -= up    * MOVE_SPEED * dt; }
    }

    pub fn mouse_move(&mut self, dx: f32, dy: f32) {
        self.yaw   += dx * MOUSE_SENSITIVITY;
        self.pitch  = (self.pitch - dy * MOUSE_SENSITIVITY).clamp(-89.0, 89.0);
    }

    pub fn ubo(&self, aspect: f32) -> CameraUbo {
        let target = self.eye + self.forward();

        let view = Matrix4::look_at_rh(self.eye, target, vec3(0.0, 1.0, 0.0));

        let fix = Matrix4::new(
            1.0, 0.0,       0.0, 0.0,
            0.0, -1.0,      0.0, 0.0,
            0.0,  0.0, 1.0/2.0, 0.0,
            0.0,  0.0, 1.0/2.0, 1.0,
        );
        let proj = fix * cgmath::perspective(Deg(90.0), aspect, 0.1, 100.0);

        CameraUbo { model: Matrix4::from_scale(1.0), view, proj }
    }
}
