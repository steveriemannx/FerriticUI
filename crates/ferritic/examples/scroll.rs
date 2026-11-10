//! A scrolling list: a card with a `Scroll` viewport full of button rows.
//! The snapshot renders the top of the list and a scrolled state (with the
//! scrollbar thumb moved).

use ferritic::prelude::*;
use ferritic::ui;
use ferritic::WindowOptions;

#[derive(Clone, Debug)]
enum Msg {
    Row(usize),
}

struct List {
    ui: ScrollUi,
}

ui! {
    msg = Msg;
    struct ScrollUi {
        Center(padding: 20.0) {
            card: Panel(
                padding: 12.0,
                background: Color::WHITE,
                radius: 14.0,
                border_width: 1.0,
                border_color: Color::rgba8(0xd8, 0xd8, 0xe2, 0xff),
                shadow_color: Color::rgba(0.0, 0.0, 0.0, 0.12),
                shadow_blur: 18.0,
                shadow_offset_x: 0.0,
                shadow_offset_y: 5.0,
            ) {
                list: Scroll(padding: 4.0, spacing: 2.0) {
                    r0: Button<Msg>(text_align: Align::Start, text: "Row 1", on_click: Msg::Row(0)),
                    r1: Button<Msg>(text_align: Align::Start, text: "Row 2", on_click: Msg::Row(1)),
                    r2: Button<Msg>(text_align: Align::Start, text: "Row 3", on_click: Msg::Row(2)),
                    r3: Button<Msg>(text_align: Align::Start, text: "Row 4", on_click: Msg::Row(3)),
                    r4: Button<Msg>(text_align: Align::Start, text: "Row 5", on_click: Msg::Row(4)),
                    r5: Button<Msg>(text_align: Align::Start, text: "Row 6", on_click: Msg::Row(5)),
                    r6: Button<Msg>(text_align: Align::Start, text: "Row 7", on_click: Msg::Row(6)),
                    r7: Button<Msg>(text_align: Align::Start, text: "Row 8", on_click: Msg::Row(7)),
                    r8: Button<Msg>(text_align: Align::Start, text: "Row 9", on_click: Msg::Row(8)),
                    r9: Button<Msg>(text_align: Align::Start, text: "Row 10", on_click: Msg::Row(9)),
                    r10: Button<Msg>(text_align: Align::Start, text: "Row 11", on_click: Msg::Row(10)),
                    r11: Button<Msg>(text_align: Align::Start, text: "Row 12", on_click: Msg::Row(11)),
                }
            }
        }
    }
}

impl List {
    fn new(theme: &Theme) -> Self {
        Self {
            ui: ScrollUi::new(theme),
        }
    }
}

impl App for List {
    type Msg = Msg;
    type Ui = ScrollUi;

    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        match msg {
            Msg::Row(index) => {
                let _ = index;
            }
        }
    }

    fn view(&self) -> &ScrollUi {
        &self.ui
    }

    fn view_mut(&mut self) -> &mut ScrollUi {
        &mut self.ui
    }
}

fn main() -> Result<(), String> {
    if std::env::args().any(|argument| argument == "--snapshot") {
        return snapshot();
    }
    let options = WindowOptions {
        title: "FerriticUI: Scroll".to_string(),
        width: 360,
        height: 300,
        theme: Theme::light(),
    };
    ferritic::run(options, List::new)
}

const SNAPSHOT_SIZE: Size = Size {
    width: 360.0,
    height: 300.0,
};
const SNAPSHOT_SCALE: f32 = 2.0;

fn build_scene(app: &mut List, text: &mut dyn TextEngine, theme: &Theme, size: Size) -> Scene {
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

fn snapshot() -> Result<(), String> {
    let theme = Theme::light();
    let mut app = List::new(&theme);
    let mut text = ferritic::SystemText::new()?;
    text.set_scale(SNAPSHOT_SCALE);
    let mut gpu = ferritic::render::Gpu::new_offscreen()?;

    let scene = build_scene(&mut app, &mut text, &theme, SNAPSHOT_SIZE);
    let atlas = text.atlas_snapshot();
    let top = ferritic::render::render_offscreen(
        &mut gpu,
        SNAPSHOT_SIZE,
        SNAPSHOT_SCALE,
        &scene,
        atlas.as_ref(),
    )?;
    let path = std::env::temp_dir().join("ferritic-scroll.png");
    write_png(&path, &top)?;
    println!("wrote {}", path.display());

    app.ui.list.offset = 90.0;
    let scene = build_scene(&mut app, &mut text, &theme, SNAPSHOT_SIZE);
    let atlas = text.atlas_snapshot();
    let scrolled = ferritic::render::render_offscreen(
        &mut gpu,
        SNAPSHOT_SIZE,
        SNAPSHOT_SCALE,
        &scene,
        atlas.as_ref(),
    )?;
    let path = std::env::temp_dir().join("ferritic-scroll-scrolled.png");
    write_png(&path, &scrolled)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn write_png(
    path: &std::path::Path,
    snapshot: &ferritic::render::Snapshot,
) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, snapshot.width, snapshot.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(&snapshot.pixels)
        .map_err(|error| error.to_string())?;
    Ok(())
}
