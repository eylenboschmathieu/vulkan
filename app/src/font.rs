#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{collections::HashMap, fs, rc::Rc};
use anyhow::Result;

use blitz::{Blitz, Container, TextureId};
use fontdue::{Font, FontSettings};

#[derive(Debug)]
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

#[derive(Debug)]
pub struct FontManager {
    pub ui_atlas:    Rc<FontAtlas>,
    pub debug_atlas: Rc<FontAtlas>,
}

impl FontManager {
    pub unsafe fn new(blitz: &mut Blitz) -> Result<Self> {
        let mut ui_atlas    = None;
        let mut debug_atlas = None;

        blitz.upload(|container| {
            ui_atlas    = Some(Rc::new(FontAtlas::new(container, "app/font/consolas.ttf", 24.0)?));
            debug_atlas = Some(Rc::new(FontAtlas::new(container, "app/font/consolas.ttf", 16.0)?));
            Ok(())
        })?;

        Ok(Self {
            ui_atlas:    ui_atlas.unwrap(),
            debug_atlas: debug_atlas.unwrap(),
        })
    }
}

#[derive(Debug)]
pub struct FontAtlas {
    pub texture_id: TextureId,
    pub glyphs: HashMap<char, GlyphInfo>,
    pub white_uv: [f32; 2],
    pub line_height: f32,
    pub font_name: Option<String>,
    pub font_size: f32,
}

impl FontAtlas {
    pub unsafe fn new(container: &mut Container, path: &str, font_size: f32) -> Result<Self> {
        let font_data = fs::read(path)?;
        let font = Font::from_bytes(font_data.as_slice(), FontSettings::default()).unwrap();

        let characters = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=[]{}|;':\",./<>? ";

        let (data, width, height, glyphs, white_uv) = FontAtlas::build_font(&font, characters, font_size);

        let texture_id = container.alloc_font_atlas(data, width, height)?;

        Ok(Self {
            font_size,
            font_name: font.name().map(|s| s.to_string()),
            texture_id,
            glyphs,
            white_uv,
            line_height: height as f32,
        })
    }

    pub fn build_font(font: &Font, characters: &str, font_size: f32) -> (Vec<u8>, u32, u32, HashMap<char, GlyphInfo>, [f32; 2]) {
        let rasterized: Vec<(char, fontdue::Metrics, Vec<u8>)> = characters
            .chars()
            .map(|c| {
                let (metrics, bitmap) = font.rasterize(c, font_size);
                (c, metrics, bitmap)
            }).collect();

        let atlas_height = rasterized.iter()
            .map(|(_, metrics, _)| metrics.height as u32)
            .max()
            .unwrap_or(0);

        // Column 0 is reserved for a full-height white strip used by UI quads.
        let padding = 1u32;
        let atlas_width = 1 + rasterized.iter()
            .map(|(_, metrics, _)| metrics.width as u32 + padding)
            .sum::<u32>();

        let mut atlas_data = vec![0u8; (atlas_width * atlas_height) as usize];

        for row in 0..atlas_height {
            atlas_data[(row * atlas_width) as usize] = 0xFF;
        }

        let mut cursor_x = 1u32;
        let mut glyphs = HashMap::new();

        for (c, metrics, bitmap) in &rasterized {
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let atlas_idx = row as u32 * atlas_width + cursor_x + col as u32;
                    atlas_data[atlas_idx as usize] = bitmap[row * metrics.width + col];
                }
            }

            let hw = 0.5 / atlas_width as f32;
            let hh = 0.5 / atlas_height as f32;
            glyphs.insert(*c, GlyphInfo {
                uv_min: [cursor_x as f32 / atlas_width as f32 + hw, hh],
                uv_max: [
                    (cursor_x + metrics.width as u32) as f32 / atlas_width as f32 - hw,
                    metrics.height as f32 / atlas_height as f32 - hh,
                ],
                width: metrics.width as u32,
                height: metrics.height as u32,
                advance: metrics.advance_width,
                bearing_x: metrics.xmin as f32,
                bearing_y: metrics.ymin as f32,
            });

            cursor_x += metrics.width as u32 + padding;
        }

        let white_uv = [0.5 / atlas_width as f32, 0.5 / atlas_height as f32];

        (atlas_data, atlas_width, atlas_height, glyphs, white_uv)
    }
}
