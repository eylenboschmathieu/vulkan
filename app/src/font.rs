#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{collections::HashMap, fs, rc::Rc};
use anyhow::Result;

use blitz::{Blitz, Container};
use fontdue::{Font, FontSettings};

pub use ui::{FontAtlas, GlyphInfo};

pub struct FontManager {
    pub ui_atlas: Rc<FontAtlas>,
}

impl FontManager {
    pub unsafe fn new(blitz: &mut Blitz) -> Result<Self> {
        let mut ui_atlas = None;

        blitz.upload(|container| {
            ui_atlas = Some(Rc::new(build_atlas(container, "app/font/consolas.ttf", 24.0)?));
            Ok(())
        })?;

        Ok(Self {
            ui_atlas: ui_atlas.unwrap(),
        })
    }
}

pub unsafe fn build_atlas(container: &mut Container, path: &str, font_size: f32) -> Result<FontAtlas> {
    let font_data = fs::read(path)?;
    let font = Font::from_bytes(font_data.as_slice(), FontSettings::default()).unwrap();

    let characters = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=[]{}|;':\",./<>? ";

    let (data, width, height, glyphs, cap_height) = build_font(&font, characters, font_size);

    let texture_id = container.alloc_font_atlas(data, width, height)?;

    Ok(FontAtlas {
        texture_id: ui::TextureId(texture_id as u64),
        glyphs,
        line_height: height as f32,
        cap_height,
    })
}

fn build_font(font: &Font, characters: &str, font_size: f32) -> (Vec<u8>, u32, u32, HashMap<char, GlyphInfo>, f32) {
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

    let padding = 1u32;
    let atlas_width = rasterized.iter()
        .map(|(_, metrics, _)| metrics.width as u32 + padding)
        .sum::<u32>();

    let mut atlas_data = vec![0u8; (atlas_width * atlas_height) as usize];

    let mut cursor_x = 0u32;
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

    let cap_height = rasterized.iter()
        .filter(|(c, _, _)| c.is_uppercase())
        .map(|(_, m, _)| (m.ymin + m.height as i32).max(0) as f32)
        .fold(0.0f32, f32::max);

    (atlas_data, atlas_width, atlas_height, glyphs, cap_height)
}
