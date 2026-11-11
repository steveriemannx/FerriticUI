//! Renders the counter UI with the CPU rasterizer and writes a PPM snapshot.
//!
//! This is the visual check for `ferritic-soft`: it builds the same scene as
//! `ferritic/examples/counter.rs --snapshot` and rasterizes it without a GPU.
//! Compare the result against `docs/images/counter-snapshot.png` with
//! `scripts/ssim.py` (PIL reads PPM directly).
//!
//! ```bash
//! cargo run -p ferritic-soft --example counter_snapshot
//! ```

use ferritic_core::prelude::*;
use ferritic_macros::ui;
use ferritic_soft::SoftRenderer;
use ferritic_text::SystemText;

#[derive(Clone, Debug)]
enum Msg {
    Increment,
    Reset,
}

ui! {
    msg = Msg;
    struct CounterUi {
        Center(padding: 24.0) {
            card: Panel(
                padding: 20.0,
                spacing: 12.0,
                align: Align::Center,
                background: Color::WHITE,
                radius: 14.0,
                border_width: 1.0,
                border_color: Color::rgba8(0xd8, 0xd8, 0xe2, 0xff),
                shadow_color: Color::rgba(0.0, 0.0, 0.0, 0.12),
                shadow_blur: 18.0,
                shadow_offset_x: 0.0,
                shadow_offset_y: 5.0,
            ) {
                value: Label(text: "0"),
                buttons: Row(spacing: 10.0) {
                    inc: Button<Msg>(text: "+", on_click: Msg::Increment),
                    reset: Button<Msg>(text: "Reset", on_click: Msg::Reset),
                }
            }
        }
    }
}

const SNAPSHOT_SIZE: Size = Size {
    width: 360.0,
    height: 220.0,
};
const SNAPSHOT_SCALE: f32 = 2.0;

fn build_scene(
    ui: &mut CounterUi,
    text: &mut dyn TextEngine,
    theme: &Theme,
    size: Size,
) -> Scene {
    ui.apply_theme(theme);
    let _ = ui.measure(text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));
    let mut scene = Scene::default();
    scene.prims.push(Prim::Rect {
        rect: Rect::from_size(size),
        color: theme.background,
        radius: 0.0,
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        clip: None,
    });
    ui.paint(text, &mut scene);
    scene
}

fn main() -> Result<(), String> {
    let theme = Theme::light();
    let mut ui = CounterUi::new(&theme);
    let mut text = SystemText::new()?;
    text.set_scale(SNAPSHOT_SCALE);
    println!("font: {}", text.font_name());

    let scene = build_scene(&mut ui, &mut text, &theme, SNAPSHOT_SIZE);
    let atlas = text.atlas_snapshot();

    let physical_width = (SNAPSHOT_SIZE.width * SNAPSHOT_SCALE) as u32;
    let physical_height = (SNAPSHOT_SIZE.height * SNAPSHOT_SCALE) as u32;
    let mut renderer = SoftRenderer::new();
    renderer.resize(physical_width, physical_height, SNAPSHOT_SCALE);
    let pixels = renderer.render(&scene, atlas.as_ref());

    let path = std::env::temp_dir().join("ferritic-counter-soft.ppm");
    write_ppm(&path, physical_width, physical_height, pixels)?;
    println!(
        "wrote {} ({} prims, {} glyphs)",
        path.display(),
        scene.prims.len(),
        count_glyphs(&scene)
    );
    Ok(())
}

/// Counts the glyph primitives in `scene`, for a quick sanity print.
fn count_glyphs(scene: &Scene) -> usize {
    scene
        .prims
        .iter()
        .filter(|prim| matches!(prim, Prim::Glyph { .. }))
        .count()
}

fn write_ppm(
    path: &std::path::Path,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<(), String> {
    let header = format!("P6\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + rgba.len() / 4 * 3);
    out.extend_from_slice(header.as_bytes());
    for pixel in rgba.chunks_exact(4) {
        out.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(path, out).map_err(|error| error.to_string())
}
