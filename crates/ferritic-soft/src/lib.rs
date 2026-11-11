//! Pure-Rust CPU rasterizer for FerriticUI scenes.
//!
//! This is the Redox/Orbital rendering path: the same [`Scene`] that
//! `ferritic-blade` renders on the GPU is rasterized into an RGBA8 buffer here,
//! which `ferritic-orbital` copies into an Orbital client window.
//!
//! The coverage rules mirror `ferritic-blade/src/shader.wgsl`:
//! * rounded rectangles use the same signed distance field and 1px analytic AA;
//! * borders are the ring between the outer shape and `d + border_width`;
//! * shadows fade with the shader's smoothstep over `blur`;
//! * glyphs are alpha masks sampled from the A8 atlas.
//!
//! Colors are straight-alpha sRGB and are blended source-over, matching
//! blade's `ALPHA_BLENDING` pipeline.
//!
//! ```
//! use ferritic_core::{Color, Prim, Rect, Scene};
//! use ferritic_soft::SoftRenderer;
//!
//! let mut renderer = SoftRenderer::new();
//! renderer.resize(4, 4, 1.0);
//! let mut scene = Scene::default();
//! scene.prims.push(Prim::Rect {
//!     rect: Rect::new(0.0, 0.0, 4.0, 4.0),
//!     color: Color::rgba8(255, 0, 0, 255),
//!     radius: 0.0,
//!     border_width: 0.0,
//!     border_color: Color::TRANSPARENT,
//!     clip: None,
//! });
//! let pixels = renderer.render(&scene, None);
//! assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
//! ```

#![forbid(unsafe_code)]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use ferritic_core::{AtlasUpdate, Prim, Rect, Scene};

/// A CPU renderer for FerriticUI's [`Scene`].
pub struct SoftRenderer {
    width: u32,
    height: u32,
    scale: f32,
    pixels: Vec<u8>,
    atlas: Vec<u8>,
    atlas_width: u32,
    atlas_height: u32,
    atlas_version: u64,
}

impl Default for SoftRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftRenderer {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            scale: 1.0,
            pixels: Vec::new(),
            atlas: Vec::new(),
            atlas_width: 0,
            atlas_height: 0,
            atlas_version: 0,
        }
    }

    /// Resizes the physical pixel buffer. `width`/`height` are physical
    /// pixels; `scale` converts logical scene coordinates to physical pixels.
    pub fn resize(&mut self, width: u32, height: u32, scale: f32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.pixels = vec![0; (width as usize) * (height as usize) * 4];
        }
        self.scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The most recently rendered RGBA8 pixels (straight alpha).
    pub fn rgba(&self) -> &[u8] {
        &self.pixels
    }

    /// Renders `scene` into the internal pixel buffer and returns it.
    pub fn render(&mut self, scene: &Scene, atlas: Option<&AtlasUpdate>) -> &[u8] {
        if let Some(update) = atlas {
            self.apply_atlas(update);
        }
        self.pixels.fill(0);
        if self.width == 0 || self.height == 0 {
            return &self.pixels;
        }
        let bounds = self.full_bounds();
        for prim in &scene.prims {
            match *prim {
                Prim::Rect {
                    rect,
                    color,
                    radius,
                    border_width,
                    border_color,
                    clip,
                } => self.draw_rect(
                    rect,
                    color,
                    radius,
                    border_width,
                    border_color,
                    bounds.intersect(self.clip_bounds(clip)),
                ),
                Prim::Shadow {
                    rect,
                    radius,
                    color,
                    blur,
                    offset,
                    clip,
                } => self.draw_shadow(
                    rect,
                    radius,
                    color,
                    blur,
                    offset,
                    bounds.intersect(self.clip_bounds(clip)),
                ),
                Prim::Glyph {
                    rect,
                    uv,
                    color,
                    clip,
                } => self.draw_glyph(rect, uv, color, bounds.intersect(self.clip_bounds(clip))),
            }
        }
        &self.pixels
    }

    fn full_bounds(&self) -> Bounds {
        Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: self.width as f32,
            y1: self.height as f32,
        }
    }

    fn clip_bounds(&self, clip: Option<Rect>) -> Bounds {
        match clip {
            Some(clip) => self.full_bounds().intersect(physical_rect(clip, self.scale)),
            None => self.full_bounds(),
        }
    }

    fn apply_atlas(&mut self, update: &AtlasUpdate) {
        if update.width == 0 || update.height == 0 {
            return;
        }
        let len = (update.width as usize) * (update.height as usize);
        if self.atlas_version != update.version
            || self.atlas.len() != len
            || self.atlas_width != update.width
            || self.atlas_height != update.height
        {
            self.atlas = vec![0; len];
            let count = update.pixels.len().min(len);
            self.atlas[..count].copy_from_slice(&update.pixels[..count]);
            self.atlas_width = update.width;
            self.atlas_height = update.height;
            self.atlas_version = update.version;
        }
    }

    fn draw_rect(
        &mut self,
        rect: Rect,
        color: ferritic_core::Color,
        radius: f32,
        border_width: f32,
        border_color: ferritic_core::Color,
        clip: Bounds,
    ) {
        let shape = physical_rect(rect, self.scale);
        if shape.x1 <= shape.x0 || shape.y1 <= shape.y0 {
            return;
        }
        let area = shape.expand(1.0).intersect(clip);
        let Some((x0, y0, x1, y1)) = self.pixel_range(area) else {
            return;
        };
        let radius = (radius * self.scale).max(0.0);
        let border_width = (border_width * self.scale).max(0.0);
        let fill = color.to_array();
        let border = border_color.to_array();
        let width = self.width;
        let pixels = &mut self.pixels;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let d = sd_rounded_box(px, py, shape, radius);
                let inside = (0.5 - d).clamp(0.0, 1.0);
                let (fill_coverage, border_coverage) = if border_width > 0.0 {
                    let inner = (0.5 - (d + border_width)).clamp(0.0, 1.0);
                    let border_coverage = (inside - inner).max(0.0);
                    ((inside - border_coverage).max(0.0), border_coverage)
                } else {
                    (inside, 0.0)
                };
                let alpha = fill[3] * fill_coverage + border[3] * border_coverage;
                if alpha <= 0.0 {
                    continue;
                }
                let rgb = [
                    (fill[0] * fill[3] * fill_coverage + border[0] * border[3] * border_coverage)
                        / alpha,
                    (fill[1] * fill[3] * fill_coverage + border[1] * border[3] * border_coverage)
                        / alpha,
                    (fill[2] * fill[3] * fill_coverage + border[2] * border[3] * border_coverage)
                        / alpha,
                ];
                blend_pixel(pixels, width, x, y, [rgb[0], rgb[1], rgb[2], alpha]);
            }
        }
    }

    fn draw_shadow(
        &mut self,
        rect: Rect,
        radius: f32,
        color: ferritic_core::Color,
        blur: f32,
        offset: (f32, f32),
        clip: Bounds,
    ) {
        let shape = physical_rect(
            Rect::new(
                rect.x + offset.0,
                rect.y + offset.1,
                rect.width,
                rect.height,
            ),
            self.scale,
        );
        if shape.x1 <= shape.x0 || shape.y1 <= shape.y0 {
            return;
        }
        let blur = (blur * self.scale).max(0.5);
        let area = shape.expand(blur + 1.0).intersect(clip);
        let Some((x0, y0, x1, y1)) = self.pixel_range(area) else {
            return;
        };
        let radius = (radius * self.scale).max(0.0);
        let fill = color.to_array();
        let width = self.width;
        let pixels = &mut self.pixels;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let d = sd_rounded_box(px, py, shape, radius);
                let t = (d / blur).clamp(0.0, 1.0);
                let falloff = 1.0 - t * t * (3.0 - 2.0 * t);
                let alpha = fill[3] * falloff;
                if alpha > 0.0 {
                    blend_pixel(pixels, width, x, y, [fill[0], fill[1], fill[2], alpha]);
                }
            }
        }
    }

    fn draw_glyph(
        &mut self,
        rect: Rect,
        uv: [f32; 4],
        color: ferritic_core::Color,
        clip: Bounds,
    ) {
        // Blade snaps glyph quads to the physical pixel grid before sampling.
        let left = (rect.x * self.scale).round();
        let top = (rect.y * self.scale).round();
        let right = (rect.right() * self.scale).round();
        let bottom = (rect.bottom() * self.scale).round();
        let shape = Bounds {
            x0: left,
            y0: top,
            x1: right,
            y1: bottom,
        };
        if shape.x1 <= shape.x0 || shape.y1 <= shape.y0 {
            return;
        }
        let area = shape.intersect(clip);
        let Some((x0, y0, x1, y1)) = self.pixel_range(area) else {
            return;
        };
        if self.atlas.is_empty() {
            return;
        }
        let fill = color.to_array();
        let width = self.width;
        let atlas = &self.atlas;
        let atlas_width = self.atlas_width;
        let atlas_height = self.atlas_height;
        let pixels = &mut self.pixels;
        let span_x = shape.x1 - shape.x0;
        let span_y = shape.y1 - shape.y0;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let fy = ((py - shape.y0) / span_y).clamp(0.0, 1.0);
            let v = uv[1] + (uv[3] - uv[1]) * fy;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let fx = ((px - shape.x0) / span_x).clamp(0.0, 1.0);
                let u = uv[0] + (uv[2] - uv[0]) * fx;
                let mask = sample_atlas(atlas, atlas_width, atlas_height, u, v);
                let alpha = fill[3] * mask;
                if alpha > 0.0 {
                    blend_pixel(pixels, width, x, y, [fill[0], fill[1], fill[2], alpha]);
                }
            }
        }
    }

    fn pixel_range(&self, bounds: Bounds) -> Option<(i32, i32, i32, i32)> {
        let x0 = bounds.x0.max(0.0).floor() as i32;
        let y0 = bounds.y0.max(0.0).floor() as i32;
        let x1 = bounds
            .x1
            .min(self.width as f32)
            .ceil()
            .max(0.0) as i32;
        let y1 = bounds
            .y1
            .min(self.height as f32)
            .ceil()
            .max(0.0) as i32;
        if x0 >= x1 || y0 >= y1 {
            None
        } else {
            Some((x0, y0, x1, y1))
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Bounds {
    fn expand(self, amount: f32) -> Self {
        Self {
            x0: self.x0 - amount,
            y0: self.y0 - amount,
            x1: self.x1 + amount,
            y1: self.y1 + amount,
        }
    }

    fn intersect(self, other: Self) -> Self {
        Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }
}

fn physical_rect(rect: Rect, scale: f32) -> Bounds {
    Bounds {
        x0: rect.x * scale,
        y0: rect.y * scale,
        x1: rect.right() * scale,
        y1: rect.bottom() * scale,
    }
}

/// Signed distance to a rounded box, identical to the shader's formulation.
fn sd_rounded_box(px: f32, py: f32, shape: Bounds, radius: f32) -> f32 {
    let half_width = (shape.x1 - shape.x0) * 0.5;
    let half_height = (shape.y1 - shape.y0) * 0.5;
    if half_width <= 0.0 || half_height <= 0.0 {
        return f32::INFINITY;
    }
    let cx = shape.x0 + half_width;
    let cy = shape.y0 + half_height;
    let radius = radius.clamp(0.0, half_width.min(half_height));
    let qx = (px - cx).abs() - half_width + radius;
    let qy = (py - cy).abs() - half_height + radius;
    qx.max(qy).min(0.0) + qx.max(0.0).hypot(qy.max(0.0)) - radius
}

fn sample_atlas(atlas: &[u8], width: u32, height: u32, u: f32, v: f32) -> f32 {
    if width == 0 || height == 0 || atlas.is_empty() {
        return 0.0;
    }
    let x = (u.clamp(0.0, 1.0) * width as f32 - 0.5).clamp(0.0, (width - 1) as f32);
    let y = (v.clamp(0.0, 1.0) * height as f32 - 0.5).clamp(0.0, (height - 1) as f32);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let texel = |x: u32, y: u32| -> f32 {
        let index = (y as usize) * (width as usize) + x as usize;
        atlas.get(index).copied().unwrap_or(0) as f32
    };
    let top = texel(x0, y0) * (1.0 - tx) + texel(x1, y0) * tx;
    let bottom = texel(x0, y1) * (1.0 - tx) + texel(x1, y1) * tx;
    (top * (1.0 - ty) + bottom * ty) / 255.0
}

/// Blends one source pixel with blade's `ALPHA_BLENDING` factors: color
/// `Cs * Sa + Cd * (1 - Sa)`, alpha `Sa + Ad * (1 - Sa)`. The source color is
/// straight alpha, so the division by the source alpha happens on the GPU
/// before blending, not here.
fn blend_pixel(pixels: &mut [u8], width: u32, x: i32, y: i32, source: [f32; 4]) {
    if x < 0 || y < 0 {
        return;
    }
    let index = ((y as u32 * width + x as u32) * 4) as usize;
    let Some(destination) = pixels.get_mut(index..index + 4) else {
        return;
    };
    let source_alpha = source[3].clamp(0.0, 1.0);
    if source_alpha <= 0.0 {
        return;
    }
    let inverse = 1.0 - source_alpha;
    let destination_alpha = destination[3] as f32 / 255.0;
    for channel in 0..3 {
        let destination_channel = destination[channel] as f32 / 255.0;
        let out = source[channel] * source_alpha + destination_channel * inverse;
        destination[channel] = (out * 255.0).clamp(0.0, 255.0).round() as u8;
    }
    let out_alpha = source_alpha + destination_alpha * inverse;
    destination[3] = (out_alpha * 255.0).clamp(0.0, 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferritic_core::Color;

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * width + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    }

    fn rect(rect: Rect, color: Color) -> Prim {
        Prim::Rect {
            rect,
            color,
            radius: 0.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            clip: None,
        }
    }

    #[test]
    fn opaque_rects_composite_in_paint_order() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(8, 8, 1.0);
        let mut scene = Scene::default();
        scene.prims.push(rect(
            Rect::new(0.0, 0.0, 8.0, 8.0),
            Color::rgba8(255, 0, 0, 255),
        ));
        scene.prims.push(rect(
            Rect::new(2.0, 2.0, 4.0, 4.0),
            Color::rgba8(0, 0, 255, 255),
        ));
        let pixels = renderer.render(&scene, None);
        assert_eq!(pixel(pixels, 8, 1, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(pixels, 8, 3, 3), [0, 0, 255, 255]);
    }

    #[test]
    fn translucent_source_uses_gpu_blend_factors() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(1, 1, 1.0);
        let mut scene = Scene::default();
        scene.prims.push(rect(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::rgba8(255, 0, 0, 128),
        ));
        let pixels = renderer.render(&scene, None);
        // Color is Cs * Sa on a transparent destination, matching the GPU
        // blend state (the source is straight alpha).
        assert_eq!(pixel(pixels, 1, 0, 0), [128, 0, 0, 128]);
    }

    #[test]
    fn clip_limits_the_draw() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(8, 8, 1.0);
        let mut scene = Scene::default();
        scene.prims.push(rect(
            Rect::new(0.0, 0.0, 8.0, 8.0),
            Color::rgba8(255, 255, 255, 255),
        ));
        scene.prims.push(Prim::Rect {
            rect: Rect::new(0.0, 0.0, 8.0, 8.0),
            color: Color::rgba8(0, 0, 0, 255),
            radius: 0.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            clip: Some(Rect::new(0.0, 0.0, 4.0, 8.0)),
        });
        let pixels = renderer.render(&scene, None);
        assert_eq!(pixel(pixels, 8, 1, 1), [0, 0, 0, 255]);
        assert_eq!(pixel(pixels, 8, 6, 1), [255, 255, 255, 255]);
    }

    #[test]
    fn rounded_corner_is_antialiased() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(16, 16, 1.0);
        let mut scene = Scene::default();
        scene.prims.push(Prim::Rect {
            rect: Rect::new(0.0, 0.0, 16.0, 16.0),
            color: Color::rgba8(255, 255, 255, 255),
            radius: 8.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            clip: None,
        });
        let pixels = renderer.render(&scene, None);
        assert_eq!(pixel(pixels, 16, 8, 8)[3], 255);
        let corner = pixel(pixels, 16, 0, 0)[3];
        assert!(corner < 255, "corner alpha {corner} should be antialiased");
    }

    #[test]
    fn glyph_mask_samples_the_atlas() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(4, 4, 1.0);
        let atlas = AtlasUpdate {
            width: 2,
            height: 2,
            pixels: vec![0, 0, 255, 255],
            version: 1,
        };
        let mut scene = Scene::default();
        scene.prims.push(Prim::Glyph {
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            uv: [0.0, 0.0, 1.0, 1.0],
            color: Color::rgba8(0, 0, 0, 255),
            clip: None,
        });
        let pixels = renderer.render(&scene, Some(&atlas));
        assert_eq!(pixel(pixels, 4, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(pixels, 4, 3, 3), [0, 0, 0, 255]);
    }

    #[test]
    fn shadow_fades_outside_the_shape() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(32, 32, 1.0);
        let mut scene = Scene::default();
        scene.prims.push(Prim::Shadow {
            rect: Rect::new(8.0, 8.0, 16.0, 16.0),
            radius: 4.0,
            color: Color::rgba8(0, 0, 0, 255),
            blur: 6.0,
            offset: (0.0, 0.0),
            clip: None,
        });
        let pixels = renderer.render(&scene, None);
        let center = pixel(pixels, 32, 16, 16)[3];
        let far = pixel(pixels, 32, 1, 1)[3];
        assert_eq!(center, 255);
        assert!(far < center, "shadow should fade toward the edge");
    }

    #[test]
    fn border_ring_uses_the_border_color() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(16, 16, 1.0);
        let mut scene = Scene::default();
        scene.prims.push(Prim::Rect {
            rect: Rect::new(0.0, 0.0, 16.0, 16.0),
            color: Color::rgba8(0, 0, 255, 255),
            radius: 0.0,
            border_width: 2.0,
            border_color: Color::rgba8(255, 0, 0, 255),
            clip: None,
        });
        let pixels = renderer.render(&scene, None);
        assert_eq!(pixel(pixels, 16, 1, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(pixels, 16, 8, 8), [0, 0, 255, 255]);
    }

    #[test]
    fn scale_maps_logical_coordinates_to_physical_pixels() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(8, 8, 2.0);
        let mut scene = Scene::default();
        scene.prims.push(rect(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Color::rgba8(255, 255, 255, 255),
        ));
        let pixels = renderer.render(&scene, None);
        assert_eq!(pixel(pixels, 8, 7, 7), [255, 255, 255, 255]);
    }

    #[test]
    fn a_new_atlas_version_replaces_the_mask() {
        let mut renderer = SoftRenderer::new();
        renderer.resize(4, 4, 1.0);
        let mut scene = Scene::default();
        scene.prims.push(Prim::Glyph {
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            uv: [0.0, 0.0, 1.0, 1.0],
            color: Color::rgba8(0, 0, 0, 255),
            clip: None,
        });
        let empty = AtlasUpdate {
            width: 2,
            height: 2,
            pixels: vec![0; 4],
            version: 1,
        };
        let pixels = renderer.render(&scene, Some(&empty));
        assert_eq!(pixel(pixels, 4, 3, 3), [0, 0, 0, 0]);
        let full = AtlasUpdate {
            width: 2,
            height: 2,
            pixels: vec![255; 4],
            version: 2,
        };
        let pixels = renderer.render(&scene, Some(&full));
        assert_eq!(pixel(pixels, 4, 3, 3), [0, 0, 0, 255]);
    }
}
