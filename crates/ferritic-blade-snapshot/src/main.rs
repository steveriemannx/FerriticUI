//! Headless `ferritic-blade` render test.
//!
//! This is the smallest end-to-end check that blade works on a platform
//! without a Vulkan window system: it initializes the HAL with
//! `presentation = false`, renders a known scene into an offscreen texture,
//! reads the pixels back and prints a checksum plus a few sampled pixels.
//!
//! It runs on the host (with any Vulkan ICD) and on Redox (Mesa/lavapipe):
//!
//! ```bash
//! cargo run -p ferritic-blade-snapshot --offline
//! ```
//!
//! Expected samples: the corners are blue (32, 64, 128) and the centre is
//! red (255, 0, 0).

use ferritic_core::{Color, Prim, Rect, Scene, Size};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

const BACKGROUND_RGBA: [u8; 4] = [32, 64, 128, 255];
const CENTRE_RGBA: [u8; 4] = [255, 0, 0, 255];

fn build_scene(size: Size) -> Scene {
    let mut scene = Scene::default();
    scene.prims.push(Prim::Rect {
        rect: Rect::from_size(size),
        color: Color::rgba8(32, 64, 128, 255),
        radius: 0.0,
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        clip: None,
    });
    scene.prims.push(Prim::Rect {
        rect: Rect::new(16.0, 16.0, 32.0, 32.0),
        color: Color::rgba8(255, 0, 0, 255),
        radius: 0.0,
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        clip: None,
    });
    scene
}

fn checksum(pixels: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in pixels {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn sample(snapshot: &ferritic_blade::Snapshot, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * snapshot.width + x) * 4) as usize;
    let pixel = &snapshot.pixels[offset..offset + 4];
    [pixel[0], pixel[1], pixel[2], pixel[3]]
}

fn main() {
    if let Err(error) = run() {
        eprintln!("BLADE-SNAPSHOT-FAIL: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    println!("ferritic-blade-snapshot: initializing blade (Vulkan, offscreen)");
    let mut gpu = ferritic_blade::Gpu::new_offscreen()?;
    println!("ferritic-blade-snapshot: device ready");

    let size = Size::new(WIDTH as f32, HEIGHT as f32);
    let scene = build_scene(size);
    let snapshot = ferritic_blade::render_offscreen(&mut gpu, size, 1.0, &scene, None)?;

    let corner = sample(&snapshot, 0, 0);
    let centre = sample(&snapshot, WIDTH / 2, HEIGHT / 2);
    let hash = checksum(&snapshot.pixels);

    println!(
        "ferritic-blade-snapshot: rendered {}x{} rgba8, checksum={hash:016x}",
        snapshot.width, snapshot.height
    );
    println!("ferritic-blade-snapshot: corner={corner:?} centre={centre:?}");

    if corner != BACKGROUND_RGBA || centre != CENTRE_RGBA {
        return Err(format!(
            "unexpected pixels: corner={corner:?} (want {BACKGROUND_RGBA:?}), \
             centre={centre:?} (want {CENTRE_RGBA:?})"
        ));
    }

    println!("BLADE-SNAPSHOT-OK");
    Ok(())
}
