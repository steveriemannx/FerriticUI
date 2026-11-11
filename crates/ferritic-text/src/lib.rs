//! A v0 text engine: one system font, no shaping, an A8 glyph atlas.
//!
//! This intentionally keeps the text stack small so the static UI core can be
//! validated first. Shaping/fallback (parley + harfrust) replaces the inside of
//! this crate later without changing the [`TextEngine`] API.

#![forbid(unsafe_code)]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::collections::HashMap;

use ferritic_core::{AtlasUpdate, GlyphQuad, Point, Rect, Size, TextEngine, TextStyle};

const ATLAS_WIDTH: u32 = 1024;
const ATLAS_HEIGHT: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GlyphKey {
    glyph: u16,
    size_bits: u32,
}

#[derive(Clone, Copy, Debug)]
struct GlyphSlot {
    uv: [f32; 4],
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

#[derive(Debug)]
struct Atlas {
    pixels: Vec<u8>,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    dirty: bool,
    version: u64,
}

impl Atlas {
    fn new() -> Self {
        Self {
            pixels: vec![0; (ATLAS_WIDTH * ATLAS_HEIGHT) as usize],
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            dirty: false,
            version: 0,
        }
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width > ATLAS_WIDTH || height > ATLAS_HEIGHT {
            return None;
        }
        if self.cursor_x + width > ATLAS_WIDTH {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + 1;
            self.row_height = 0;
        }
        if self.cursor_y + height > ATLAS_HEIGHT {
            return None;
        }
        let position = (self.cursor_x, self.cursor_y);
        self.cursor_x += width + 1;
        self.row_height = self.row_height.max(height);
        Some(position)
    }

    fn put(&mut self, x: u32, y: u32, width: u32, height: u32, data: &[u8]) {
        for row in 0..height {
            let source = (row * width) as usize;
            let destination = ((y + row) * ATLAS_WIDTH + x) as usize;
            let length = width as usize;
            if source + length <= data.len() && destination + length <= self.pixels.len() {
                self.pixels[destination..destination + length]
                    .copy_from_slice(&data[source..source + length]);
            }
        }
    }
}

/// The system-font text engine.
pub struct SystemText {
    font_data: Vec<u8>,
    face_index: usize,
    scaler: swash::scale::ScaleContext,
    glyphs: HashMap<GlyphKey, GlyphSlot>,
    metrics: HashMap<u32, (f32, f32)>, // size bits -> (ascent, line height)
    atlas: Atlas,
    font_name: String,
    scale: f32,
}

/// Picks the font family: preferred UI families first, then a generic
/// sans-serif, then the first face that maps basic Latin glyphs. The last
/// step matters on hosts whose font database starts with symbol or script
/// faces that have digits but no letters.
fn choose_face(database: &fontdb::Database) -> Option<(fontdb::ID, String)> {
    const PREFERRED: &[&str] = &[
        "Helvetica",
        "Arial",
        "SF Pro Text",
        "Menlo",
        "San Francisco",
        "DejaVu Sans",
        "Liberation Sans",
        "Noto Sans",
        "FreeSans",
        "Ubuntu",
        "Cantarell",
    ];
    for family in PREFERRED {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..Default::default()
        };
        if let Some(id) = database.query(&query) {
            return Some((id, face_family(database, id, family)));
        }
    }
    for family in [
        fontdb::Family::SansSerif,
        fontdb::Family::Monospace,
        fontdb::Family::Serif,
    ] {
        let query = fontdb::Query {
            families: &[family],
            ..Default::default()
        };
        if let Some(id) = database.query(&query) {
            return Some((id, face_family(database, id, "generic")));
        }
    }
    for face in database.faces() {
        let Some((data, index)) =
            database.with_face_data(face.id, |data, index| (data.to_vec(), index as usize))
        else {
            continue;
        };
        let Some(font) = swash::FontRef::from_index(&data, index) else {
            continue;
        };
        let charmap = font.charmap();
        if ['R', 'e', 's', 't', '0']
            .iter()
            .all(|character| charmap.map(*character) != 0)
        {
            return Some((face.id, face_family(database, face.id, "unknown")));
        }
    }
    None
}

fn face_family(database: &fontdb::Database, id: fontdb::ID, fallback: &str) -> String {
    database
        .face(id)
        .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
        .unwrap_or_else(|| fallback.to_string())
}

impl SystemText {
    pub fn new() -> Result<Self, String> {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();

        let (id, font_name) =
            choose_face(&database).ok_or_else(|| "no system font found".to_string())?;
        let (font_data, face_index) = database
            .with_face_data(id, |data, index| (data.to_vec(), index as usize))
            .ok_or_else(|| "system font has no readable data".to_string())?;

        Ok(Self {
            font_data,
            face_index,
            scaler: swash::scale::ScaleContext::new(),
            glyphs: HashMap::new(),
            metrics: HashMap::new(),
            atlas: Atlas::new(),
            font_name,
            scale: 1.0,
        })
    }

    pub fn font_name(&self) -> &str {
        &self.font_name
    }

    fn font(&self) -> Option<swash::FontRef<'_>> {
        swash::FontRef::from_index(&self.font_data, self.face_index)
    }


    fn size_metrics(&mut self, size: f32) -> (f32, f32) {
        let key = size.to_bits();
        if let Some(found) = self.metrics.get(&key) {
            return *found;
        }
        let (ascent, height) = match self.font() {
            Some(font) => {
                let metrics = font.metrics(&[]).scale(size);
                (metrics.ascent, metrics.ascent + metrics.descent)
            }
            None => (size, size * 1.2),
        };
        self.metrics.insert(key, (ascent, height));
        (ascent, height)
    }

    fn glyph_slot(&mut self, glyph: u16, size: f32) -> Option<GlyphSlot> {
        let key = GlyphKey { glyph, size_bits: size.to_bits() };
        if let Some(slot) = self.glyphs.get(&key) {
            return Some(*slot);
        }

        let index = self.face_index;
        let SystemText {
            font_data,
            scaler,
            glyphs,
            atlas,
            ..
        } = self;
        let font = swash::FontRef::from_index(font_data, index)?;
        let mut scaler_instance = scaler.builder(font).size(size).hint(true).build();
        let image = scaler_render(&mut scaler_instance, glyph)?;
        let placement = image.placement;
        let width = placement.width.max(1);
        let height = placement.height.max(1);
        let (x, y) = atlas.allocate(width, height)?;
        atlas.put(x, y, width, height, &image.data);
        atlas.dirty = true;
        atlas.version += 1;

        let slot = GlyphSlot {
            // Half-texel inset: with a linear sampler, sampling exactly on
            // the texture edge would average in the neighbouring glyph.
            uv: [
                (x as f32 + 0.5) / ATLAS_WIDTH as f32,
                (y as f32 + 0.5) / ATLAS_HEIGHT as f32,
                (x + width) as f32 / ATLAS_WIDTH as f32 - 0.5 / ATLAS_WIDTH as f32,
                (y + height) as f32 / ATLAS_HEIGHT as f32 - 0.5 / ATLAS_HEIGHT as f32,
            ],
            left: placement.left,
            top: placement.top,
            width,
            height,
        };
        glyphs.insert(key, slot);
        Some(slot)
    }

    fn for_each_glyph(&mut self, text: &str, style: &TextStyle) -> Vec<(u16, f32)> {
        let size = (style.size.max(1.0)) * self.scale;
        let Some(font) = self.font() else {
            return Vec::new();
        };
        let metrics = font.glyph_metrics(&[]).scale(size);
        let charmap = font.charmap();
        let mut result = Vec::with_capacity(text.len());
        for character in text.chars() {
            let glyph = charmap.map(character);
            if glyph == 0 {
                continue;
            }
            let advance = metrics.advance_width(glyph);
            result.push((glyph, advance));
        }
        result
    }
}

fn scaler_render(
    scaler: &mut swash::scale::Scaler<'_>,
    glyph: u16,
) -> Option<swash::scale::image::Image> {
    let sources = [swash::scale::Source::Outline];
    swash::scale::Render::new(&sources).render(scaler, glyph)
}

impl TextEngine for SystemText {
    fn measure(&mut self, text: &str, style: &TextStyle) -> Size {
        let scale = self.scale.max(0.01);
        let physical = style.size * scale;
        let (_, line_height) = self.size_metrics(physical);
        let runs = self.for_each_glyph(text, style);
        let width = runs.iter().map(|(_, advance)| *advance).sum::<f32>() / scale;
        Size::new(width, line_height / scale)
    }

    fn layout(&mut self, text: &str, style: &TextStyle, origin: Point, out: &mut Vec<GlyphQuad>) {
        let scale = self.scale.max(0.01);
        let physical = style.size * scale;
        let (ascent, _) = self.size_metrics(physical);
        let runs = self.for_each_glyph(text, style);
        let baseline = origin.y + ascent / scale;
        let mut pen_x = origin.x;
        for (glyph, advance) in runs {
            if let Some(slot) = self.glyph_slot(glyph, physical) {
                let x = pen_x + slot.left as f32 / scale;
                // swash's `Placement` is relative to the glyph origin on the
                // baseline with the y axis pointing down: `top` is how far the
                // bitmap's top sits above the baseline (positive up), so the
                // bitmap top is `baseline - top`.
                let y = baseline - slot.top as f32 / scale;
                out.push(GlyphQuad {
                    rect: Rect::new(
                        x,
                        y,
                        slot.width as f32 / scale,
                        slot.height as f32 / scale,
                    ),
                    uv: slot.uv,
                    color: style.color,
                });
            }
            pen_x += advance / scale;
        }
    }

    fn atlas_update(&mut self) -> Option<AtlasUpdate> {
        if !self.atlas.dirty {
            return None;
        }
        self.atlas.dirty = false;
        Some(AtlasUpdate {
            width: ATLAS_WIDTH,
            height: ATLAS_HEIGHT,
            pixels: self.atlas.pixels.clone(),
            version: self.atlas.version,
        })
    }

    fn line_height(&mut self, size: f32) -> f32 {
        let scale = self.scale.max(0.01);
        self.size_metrics(size * scale).1 / scale
    }

    fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.01);
    }

    fn atlas_snapshot(&mut self) -> Option<AtlasUpdate> {
        Some(AtlasUpdate {
            width: ATLAS_WIDTH,
            height: ATLAS_HEIGHT,
            pixels: self.atlas.pixels.clone(),
            version: self.atlas.version,
        })
    }
}
