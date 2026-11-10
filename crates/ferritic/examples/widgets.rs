//! A small widget gallery: a card with a title, two checkboxes and two
//! buttons (one disabled), rendered offscreen for the snapshot golden.

use ferritic::prelude::*;
use ferritic::ui;
use ferritic::WindowOptions;

#[derive(Clone, Debug)]
enum Msg {
    DarkMode,
    ShowGrid,
    VolumeChanged,
    Apply,
}

struct Gallery {
    ui: WidgetsUi,
}

ui! {
    msg = Msg;
    struct WidgetsUi {
        Center(padding: 24.0) {
            card: Panel(
                padding: 20.0,
                spacing: 14.0,
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
                title: Label(text: "Settings"),
                dark: Checkbox<Msg>(text: "Dark mode", on_change: Msg::DarkMode),
                grid: Checkbox<Msg>(text: "Show grid", checked: true, on_change: Msg::ShowGrid),
                volume_label: Label(text: "Volume"),
                volume: Slider<Msg>(value: 0.35, on_change: Msg::VolumeChanged),
                buttons: Row(spacing: 10.0) {
                    apply: Button<Msg>(text: "Apply", on_click: Msg::Apply),
                    disabled: Button<Msg>(text: "Disabled", enabled: false),
                }
            }
        }
    }
}

impl Gallery {
    fn new(theme: &Theme) -> Self {
        let mut ui = WidgetsUi::new(theme);
        ui.title.style.size = 19.0;
        ui.title.style.color = theme.text;
        Self { ui }
    }
}

impl App for Gallery {
    type Msg = Msg;
    type Ui = WidgetsUi;

    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        match msg {
            Msg::DarkMode | Msg::ShowGrid | Msg::VolumeChanged | Msg::Apply => {}
        }
    }

    fn view(&self) -> &WidgetsUi {
        &self.ui
    }

    fn view_mut(&mut self) -> &mut WidgetsUi {
        &mut self.ui
    }
}

fn main() -> Result<(), String> {
    if std::env::args().any(|argument| argument == "--snapshot") {
        return snapshot();
    }
    let options = WindowOptions {
        title: "FerriticUI: Widgets".to_string(),
        width: 360,
        height: 300,
        theme: Theme::light(),
    };
    ferritic::run(options, Gallery::new)
}

fn snapshot() -> Result<(), String> {
    let theme = Theme::light();
    let mut app = Gallery::new(&theme);
    let mut text = ferritic::SystemText::new()?;
    let scale = 2.0;
    text.set_scale(scale);
    let size = Size::new(360.0, 300.0);

    {
        let ui = app.view_mut();
        ui.apply_theme(&theme);
        let _ = ui.measure(&mut text, Constraints::loose(size));
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
    app.view_mut().paint(&mut text, &mut scene);

    let atlas = text.atlas_snapshot();
    let mut gpu = ferritic::render::Gpu::new_offscreen()?;
    let snapshot =
        ferritic::render::render_offscreen(&mut gpu, size, scale, &scene, atlas.as_ref())?;
    let path = std::env::temp_dir().join("ferritic-widgets.png");
    write_png(&path, &snapshot)?;
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
