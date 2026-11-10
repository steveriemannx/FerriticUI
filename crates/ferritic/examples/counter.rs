//! The counter example written with `ui!`: the flat static tree is generated
//! from the DSL (struct fields, node enum, child/focus consts, one `match` per
//! pass). The snapshot at the end renders the same UI offscreen.

use ferritic::prelude::*;
use ferritic::ui;
use ferritic::WindowOptions;

#[derive(Clone, Debug)]
enum Msg {
    Increment,
    Reset,
}

struct Counter {
    count: i32,
    ui: CounterUi,
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
    if std::env::args().any(|argument| argument == "--snapshot") {
        return snapshot();
    }
    let options = WindowOptions {
        title: "FerriticUI: Counter".to_string(),
        width: 360,
        height: 220,
        theme: Theme::light(),
    };
    ferritic::run(options, Counter::new)
}

const SNAPSHOT_SIZE: Size = Size {
    width: 360.0,
    height: 220.0,
};
const SNAPSHOT_SCALE: f32 = 2.0;

fn build_scene(
    app: &mut Counter,
    text: &mut dyn TextEngine,
    theme: &Theme,
    size: Size,
) -> Scene {
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
    let mut app = Counter::new(&theme);
    let mut text = ferritic::SystemText::new()?;
    text.set_scale(SNAPSHOT_SCALE);
    let size = SNAPSHOT_SIZE;

    let mut gpu = ferritic::render::Gpu::new_offscreen()?;

    let scene = build_scene(&mut app, &mut text, &theme, size);
    let atlas = text.atlas_snapshot();
    let snapshot =
        ferritic::render::render_offscreen(&mut gpu, size, SNAPSHOT_SCALE, &scene, atlas.as_ref())?;
    let path = std::env::temp_dir().join("ferritic-counter.png");
    write_png(&path, &snapshot)?;
    println!("wrote {}", path.display());

    // A second frame with interaction states: the plus button hovered and the
    // reset button disabled.
    {
        let ui = app.view_mut();
        ui.inc.hovered = true;
        ui.reset.enabled = false;
    }
    let scene = build_scene(&mut app, &mut text, &theme, size);
    let atlas = text.atlas_snapshot();
    let states =
        ferritic::render::render_offscreen(&mut gpu, size, SNAPSHOT_SCALE, &scene, atlas.as_ref())?;
    let path = std::env::temp_dir().join("ferritic-counter-states.png");
    write_png(&path, &states)?;
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
