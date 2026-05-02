#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use cgmath::{vec2, vec3};
use anyhow::Result;
use std::time::Instant;
use log::*;
use winit::window::Window;
use blitz::*;

pub const VERTICES: [blitz::Vertex_3D_Color_Texture; 8] = [
    Vertex_3D_Color_Texture::new(vec3(-0.5, -0.5, 0.0), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, -0.5, 0.0), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, 0.5, 0.0), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, 0.5, 0.0), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, -0.5, -0.5), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, -0.5, -0.5), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, 0.5, -0.5), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, 0.5, -0.5), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
];

pub const VERTICES2: [blitz::Vertex_3D_Color_Texture; 8] = [
    Vertex_3D_Color_Texture::new(vec3(-0.5, -0.5, -1.0), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, -0.5, -1.0), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(0.5, 0.5, -1.0), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-0.5, 0.5, -1.0), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-2.0, -2.0, -1.5), vec3(1.0, 0.0, 0.0), vec2(1.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(2.0, -2.0, -1.5), vec3(0.0, 1.0, 0.0), vec2(0.0, 0.0)),
    Vertex_3D_Color_Texture::new(vec3(2.0, 2.0, -1.5), vec3(0.0, 0.0, 1.0), vec2(0.0, 1.0)),
    Vertex_3D_Color_Texture::new(vec3(-2.0, 2.0, -1.5), vec3(1.0, 1.0, 1.0), vec2(1.0, 1.0)),
];

pub const INDICES: &[u16] = &[
    0, 1, 2, 2, 3, 0,
    4, 5, 6, 6, 7, 4,
];

#[derive(Debug)]
struct TestObject {
    texture: TextureId,
    mesh: Mesh,
    material: MaterialId,
}

impl TestObject {
    pub unsafe fn new(container: &mut Container<Loading>, material: MaterialId) -> Result<Self> {
        let mesh = container.load_mesh(
            &VERTICES,
            &INDICES,
        )?;

        let texture = container.load_texture("/home/krozu/Documents/Code/Rust/vulkan/app/img/image.png")?;

        info!("+ TestObject");
        Ok(Self { texture, mesh, material })
    }

    pub fn resolve_upload(&mut self, container: &Container<Resolved>) {
        self.mesh = container.resolve_mesh(self.mesh.vertices);
        self.texture = container.resolve_texture(self.texture);
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) -> Result<()> {
        blitz.draw(self.mesh, self.material);
        Ok(())
    }
}

#[derive(Debug)]
struct TestObject2 {
    mesh: Mesh,
    material: MaterialId,
}
impl TestObject2 {
    pub unsafe fn new(container: &mut Container<Loading>, material: MaterialId) -> Result<Self> {
        let mesh = container.load_mesh(
            &VERTICES2,
            &INDICES,
        )?;

        info!("+ TestObject");
        Ok(Self { mesh, material })
    }

    pub fn resolve_upload(&mut self, container: &Container<Resolved>) {
        self.mesh = container.resolve_mesh(self.mesh.vertices);
    }

    pub unsafe fn draw(&self, blitz: &mut Blitz) -> Result<()> {
        blitz.draw(self.mesh, self.material);
        Ok(())
    }
}

// Our Vulkan app.
#[derive(Debug)]
pub struct App{
    blitz: blitz::Blitz,
    delta: Instant,
    
    material: MaterialId,
    o: TestObject,
    o2: TestObject2,
    uniform_buffers: Vec<UniformBufferId>,
}

impl App {
    /// Create our Vulkan app.
    pub unsafe fn new(window: &Window) -> Result<Self> {
        let mut blitz = blitz::init(window)?;

        let material = blitz.new_material(MaterialDef {
            vertex_shader: include_bytes!("../shaders/texture.vert.spv"),
            fragment_shader: include_bytes!("../shaders/texture.frag.spv"),
            vertex_format: VertexFormat::Vertex3D_Color_Texture,
            textures: 1,
            uniforms: 1,
        })?;

        let mut container = blitz.new_container();

        // Pass container to a bunch of new objects
        let mut o = TestObject::new(&mut container, material)?;
        let mut o2 = TestObject2::new(&mut container, material)?;

        // Process the container when all upload data is collected
        let container = blitz.process_container(container)?;
        
        // Resolve buffer ids for all created objects
        o.resolve_upload(&container);
        o2.resolve_upload(&container);

        let uniforms = blitz.new_uniform_buffers(); // Return FRAMES_IN_FLIGHT uniform buffers

        blitz.update_descriptor_sets(o.texture);

        info!("+ App");
        Ok(Self { blitz, delta: Instant::now(), o, o2, material, uniform_buffers: uniforms })
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        // Tell blitz to start a render
        if self.blitz.start_render(window)? {

            self.o.draw(&mut self.blitz)?; // Rerecord command buffers, essentially
            self.o2.draw(&mut self.blitz)?;
            
            self.blitz.update_uniform_buffers(&self.uniform_buffers, self.delta)?;

            // Tell blitz to end the render
            self.blitz.end_render(window)?;
        }

        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.blitz.destroy();
        info!("~ App");
    }
}