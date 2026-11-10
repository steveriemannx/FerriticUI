//! A virtualized list with 1000 rows: only the visible rows are measured and
//! painted. The snapshots show the top of the list and a scrolled state with a
//! selected and a hovered row.

use ferritic::prelude::*;
use ferritic::ui;
use ferritic::WindowOptions;

#[derive(Clone, Debug)]
enum Msg {
    Selected,
}

struct Inbox {
    ui: ListUi,
}

ui! {
    msg = Msg;
    struct ListUi {
        Center(padding: 20.0) {
            card: Panel(
                padding: 12.0,
                spacing: 8.0,
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
                header: Label(text: "Inbox"),
                list: List<Msg>(row_height: 32.0, padding: 14.0, on_select: Msg::Selected),
            }
        }
    }
}

impl Inbox {
    fn new(theme: &Theme) -> Self {
        let mut ui = ListUi::new(theme);
        ui.header.style.size = 19.0;
        ui.list.set_items(
            (0..1000)
                .map(|index| format!("Message #{index:04}"))
                .collect(),
        );
        ui.list.selected = Some(2);
        Self { ui }
    }
}

impl App for Inbox {
    type Msg = Msg;
    type Ui = ListUi;

    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        match msg {
            Msg::Selected => {
                let _ = self.ui.list.selected;
            }
        }
    }

    fn view(&self) -> &ListUi {
        &self.ui
    }

    fn view_mut(&mut self) -> &mut ListUi {
        &mut self.ui
    }
}

fn main() -> Result<(), String> {
    if std::env::args().any(|argument| argument == "--snapshot") {
        return snapshot();
    }
    let options = WindowOptions {
        title: "FerriticUI: List".to_string(),
        width: 380,
        height: 340,
        theme: Theme::light(),
    };
    ferritic::run(options, Inbox::new)
}

const SNAPSHOT_SIZE: Size = Size {
    width: 380.0,
    height: 340.0,
};
const SNAPSHOT_SCALE: f32 = 2.0;

fn build_scene(app: &mut Inbox, text: &mut dyn TextEngine, theme: &Theme, size: Size) -> Scene {
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
    let mut app = Inbox::new(&theme);
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
    let path = std::env::temp_dir().join("ferritic-list.png");
    write_png(&path, &top)?;
    println!("wrote {}", path.display());

    {
        let ui = app.view_mut();
        ui.list.offset = 320.0;
        ui.list.selected = Some(12);
        ui.list.hovered = Some(13);
    }
    let scene = build_scene(&mut app, &mut text, &theme, SNAPSHOT_SIZE);
    let atlas = text.atlas_snapshot();
    let scrolled = ferritic::render::render_offscreen(
        &mut gpu,
        SNAPSHOT_SIZE,
        SNAPSHOT_SCALE,
        &scene,
        atlas.as_ref(),
    )?;
    let path = std::env::temp_dir().join("ferritic-list-scrolled.png");
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
