pub type Pos2 = cgmath::Vector2<f32>;
pub type UV   = cgmath::Vector2<f32>;
pub type Rgba = cgmath::Vector4<f32>;

/// A single vertex of the UI's quad mesh: position, atlas UV, and tint color.
/// Renderer-agnostic — the host converts this into whatever vertex layout its
/// pipeline expects.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos:   Pos2,
    pub uv:    UV,
    pub color: Rgba,
}

impl Vertex {
    pub const fn new(pos: Pos2, uv: UV, color: Rgba) -> Self {
        Self { pos, uv, color }
    }
}

/// Opaque handle to a texture (e.g. a font atlas) registered by the host.
/// The UI never interprets this value — it only tags draw data with it so
/// the host knows which of its textures to bind.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct TextureId(pub u64);

/// Which texture a node samples from, and the UV rectangle within it.
/// Defaults to a degenerate rect on [`TextureId(0)`] — node-creation helpers
/// on [`crate::Ui`] point this at the shared UI atlas's white texel so solid-
/// color quads render correctly out of the box. Nodes that want to display an
/// icon or other image set this to the relevant region of one of the host's
/// textures instead.
#[derive(Clone, Copy, Default)]
pub struct Texture {
    pub id:     TextureId,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}
