use std::collections::HashMap;

use crate::types::TextureId;

/// Per-glyph layout and atlas-UV data needed to lay out and render text.
pub struct GlyphInfo {
    /// Top-left UV coordinate of this glyph in the font atlas (normalized 0..1).
    pub uv_min: [f32; 2],
    /// Bottom-right UV coordinate of this glyph in the font atlas (normalized 0..1).
    pub uv_max: [f32; 2],
    /// Width of the glyph bitmap in pixels.
    pub width: u32,
    /// Height of the glyph bitmap in pixels.
    pub height: u32,
    /// Horizontal distance to advance the cursor after drawing this glyph, in pixels.
    pub advance: f32,
    /// Horizontal offset from the cursor to the left edge of the glyph bitmap, in pixels.
    pub bearing_x: f32,
    /// Vertical offset from the baseline to the bottom edge of the glyph bitmap, in pixels.
    pub bearing_y: f32,
}

/// Font atlas data needed for text layout and rendering. The host is
/// responsible for rasterizing the font, uploading the atlas texture, and
/// registering it under `texture_id`.
pub struct FontAtlas {
    pub texture_id:  TextureId,
    pub glyphs:      HashMap<char, GlyphInfo>,
    pub line_height: f32,
    /// Height of uppercase letters — used for vertical centering.
    pub cap_height:  f32,
}
