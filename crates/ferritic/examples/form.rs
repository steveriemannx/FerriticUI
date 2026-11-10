//! A small form with two text inputs: a focused input with a caret and an
//! input with a selection, rendered offscreen for the snapshots.

use ferritic::prelude::*;
use ferritic::ui;
use ferritic::WindowOptions;

#[derive(Clone, Debug)]
enum Msg {
    NameChanged,
    EmailChanged,
    Submit,
}

struct Form {
    ui: FormUi,
}

ui! {
    msg = Msg;
    struct FormUi {
        Center(padding: 24.0) {
            card: Panel(
                padding: 20.0,
                spacing: 12.0,
                align: Align::Start,
                background: Color::WHITE,
                radius: 14.0,
                border_width: 1.0,
                border_color: Color::rgba8(0xd8, 0xd8, 0xe2, 0xff),
                shadow_color: Color::rgba(0.0, 0.0, 0.0, 0.12),
                shadow_blur: 18.0,
                shadow_offset_x: 0.0,
                shadow_offset_y: 5.0,
            ) {
                title: Label(text: "Sign in"),
                name: TextInput<Msg>(placeholder: "Name", on_change: Msg::NameChanged),
                email: TextInput<Msg>(placeholder: "Email", on_change: Msg::EmailChanged),
                submit: Button<Msg>(text: "Continue", on_click: Msg::Submit),
            }
        }
    }
}

impl Form {
    fn new(theme: &Theme) -> Self {
        let mut ui = FormUi::new(theme);
        ui.title.style.size = 19.0;
        Self { ui }
    }
}

impl App for Form {
    type Msg = Msg;
    type Ui = FormUi;

    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        match msg {
            Msg::NameChanged => {
                let _ = &self.ui.name.text;
            }
            Msg::EmailChanged => {
                let _ = &self.ui.email.text;
            }
            Msg::Submit => {}
        }
    }

    fn view(&self) -> &FormUi {
        &self.ui
    }

    fn view_mut(&mut self) -> &mut FormUi {
        &mut self.ui
    }
}

fn main() -> Result<(), String> {
    if std::env::args().any(|argument| argument == "--snapshot") {
        return snapshot();
    }
    let options = WindowOptions {
        title: "FerriticUI: Form".to_string(),
        width: 380,
        height: 280,
        theme: Theme::light(),
    };
    ferritic::run(options, Form::new)
}

const SNAPSHOT_SIZE: Size = Size {
    width: 380.0,
    height: 280.0,
};
const SNAPSHOT_SCALE: f32 = 2.0;

fn build_scene(app: &mut Form, text: &mut dyn TextEngine, theme: &Theme, size: Size) -> Scene {
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
    let mut app = Form::new(&theme);
    let mut text = ferritic::SystemText::new()?;
    text.set_scale(SNAPSHOT_SCALE);
    let mut gpu = ferritic::render::Gpu::new_offscreen()?;

    // Focused input with a caret at the end.
    {
        let ui = app.view_mut();
        ui.name.text = "Ada Lovelace".to_string();
        ui.name.cursor = ui.name.text.len();
        ui.name.anchor = ui.name.cursor;
        ui.name.focused = true;
    }
    let scene = build_scene(&mut app, &mut text, &theme, SNAPSHOT_SIZE);
    let atlas = text.atlas_snapshot();
    let focused = ferritic::render::render_offscreen(
        &mut gpu,
        SNAPSHOT_SIZE,
        SNAPSHOT_SCALE,
        &scene,
        atlas.as_ref(),
    )?;
    let path = std::env::temp_dir().join("ferritic-form.png");
    write_png(&path, &focused)?;
    println!("wrote {}", path.display());

    // A selection in the second input, with the first input unfocused.
    {
        let ui = app.view_mut();
        ui.name.focused = false;
        ui.email.text = "ada@example.com".to_string();
        ui.email.cursor = 3;
        ui.email.anchor = 8;
        ui.email.focused = true;
    }
    let scene = build_scene(&mut app, &mut text, &theme, SNAPSHOT_SIZE);
    let atlas = text.atlas_snapshot();
    let selection = ferritic::render::render_offscreen(
        &mut gpu,
        SNAPSHOT_SIZE,
        SNAPSHOT_SCALE,
        &scene,
        atlas.as_ref(),
    )?;
    let path = std::env::temp_dir().join("ferritic-form-selection.png");
    write_png(&path, &selection)?;
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
