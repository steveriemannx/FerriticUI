//! The `counter` app on the Redox stack: `ferritic-orbital` for the window and
//! `ferritic-soft` for rendering.
//!
//! ```bash
//! # An Orbital window (an SDL2 window when built for a non-Redox host).
//! ferritic-demo
//!
//! # No display: write a PPM snapshot to the temporary directory.
//! ferritic-demo --snapshot
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

struct Counter {
    count: i32,
    ui: CounterUi,
}

impl Counter {
    fn new(theme: &Theme) -> Self {
        Self {
            count: 0,
            ui: CounterUi::new(theme),
        }
    }
}

impl App for Counter {
    type Msg = Msg;
    type Ui = CounterUi;

    fn update(&mut self, msg: Msg, cx: &mut Ctx) {
        let _ = cx;
        match msg {
            Msg::Increment => self.count += 1,
            Msg::Reset => self.count = 0,
        }
        self.ui.value.set_text(self.count.to_string());
    }

    fn view(&self) -> &CounterUi {
        &self.ui
    }

    fn view_mut(&mut self) -> &mut CounterUi {
        &mut self.ui
    }
}

fn main() -> Result<(), String> {
    let theme = Theme::light();
    if std::env::args().any(|argument| argument == "--snapshot") {
        return snapshot(&theme);
    }
    let options = WindowOptions {
        title: "FerriticUI: Counter".to_string(),
        width: 360,
        height: 220,
        theme,
    };
    ferritic_orbital::run(Counter::new(&options.theme), options)
}

const SNAPSHOT_SIZE: Size = Size {
    width: 360.0,
    height: 220.0,
};
const SNAPSHOT_SCALE: f32 = 2.0;

fn build_scene(app: &mut Counter, text: &mut dyn TextEngine, theme: &Theme, size: Size) -> Scene {
    {
        let ui = app.view_mut();
        ui.apply_theme(theme);
        let _ = ui.measure(text, Constraints::loose(size));
        ui.arrange(Rect::from_size(size));
    }
    let mut scene = Scene::default();
    scene.prims.push(Prim::Rect {
        rect: Rect::from_size(size),
        color: theme.background,
        radius: 0.0,
        border_width: 0.0,
        border_color: Color::TRANSPARENT,
        clip: None,
    });
    app.view_mut().paint(text, &mut scene);
    scene
}

fn snapshot(theme: &Theme) -> Result<(), String> {
    let mut app = Counter::new(theme);
    let mut text = SystemText::new()?;
    text.set_scale(SNAPSHOT_SCALE);

    let scene = build_scene(&mut app, &mut text, theme, SNAPSHOT_SIZE);
    let atlas = text.atlas_snapshot();

    let width = (SNAPSHOT_SIZE.width * SNAPSHOT_SCALE) as u32;
    let height = (SNAPSHOT_SIZE.height * SNAPSHOT_SCALE) as u32;
    let mut renderer = SoftRenderer::new();
    renderer.resize(width, height, SNAPSHOT_SCALE);
    let pixels = renderer.render(&scene, atlas.as_ref());

    let path = std::env::temp_dir().join("ferritic-demo-counter.ppm");
    write_ppm(&path, width, height, pixels)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn write_ppm(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let header = format!("P6\n{width} {height}\n255\n");
    let mut out = Vec::with_capacity(header.len() + rgba.len() / 4 * 3);
    out.extend_from_slice(header.as_bytes());
    for pixel in rgba.chunks_exact(4) {
        out.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(path, out).map_err(|error| error.to_string())
}
