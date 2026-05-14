#![allow(dead_code, unsafe_op_in_unsafe_fn)]

use cgmath::{vec3, vec4, InnerSpace, Matrix4, Vector4};
use anyhow::Result;
use blitz::{Blitz, Container, Mesh, Vertex_3D_Color};

use crate::camera::FpCamera;

#[derive(Debug)]
pub struct Sun {
    angle: f32,
    mesh:  Mesh,
}

impl Sun {
    pub fn new() -> Self {
        Self { angle: 0.3, mesh: Mesh::default() }
    }

    pub unsafe fn alloc(&mut self, container: &mut Container) -> Result<()> {
        let gold = vec3(1.0, 0.84, 0.0);
        self.mesh = container.alloc_mesh(
            &[
                Vertex_3D_Color::new(vec3(-0.5, -0.5, 0.0), gold),
                Vertex_3D_Color::new(vec3( 0.5, -0.5, 0.0), gold),
                Vertex_3D_Color::new(vec3( 0.5,  0.5, 0.0), gold),
                Vertex_3D_Color::new(vec3(-0.5,  0.5, 0.0), gold),
            ],
            &[0u16, 1, 2, 2, 3, 0],
        );
        Ok(())
    }

    pub fn update(&mut self, dt: f32) {
        self.angle = (self.angle + dt * std::f32::consts::TAU / 30.0) % std::f32::consts::TAU;
    }

    pub fn sun_dir(&self) -> Vector4<f32> {
        vec4(0.0, self.angle.sin(), self.angle.cos(), 0.0)
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz, camera: &FpCamera) {
        let sun_dir_v = vec3(0.0, self.angle.sin(), self.angle.cos());
        let fwd       = camera.forward();
        let cam_right = camera.right();
        let look      = -fwd;
        let up        = cam_right.cross(look).normalize();
        let eye       = vec3(camera.eye.x, camera.eye.y, camera.eye.z);
        let sun_pos   = eye + sun_dir_v * 60.0;
        let model = Matrix4::from_cols(
            (-cam_right * 8.0).extend(0.0),
            (up         * 8.0).extend(0.0),
            look.extend(0.0),
            sun_pos.extend(1.0),
        );
        blitz.draw_dynamic_color(self.mesh, model);
    }
}
