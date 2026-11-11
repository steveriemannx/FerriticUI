//! A terminal for Redox: ransid emulation, a PTY shell and FerriticUI
//! rendering on the Orbital backend.
//!
//! ```bash
//! ferritic-term [--shell PATH] [--cols N] [--rows N] [--font-size PX]
//! ```

mod app;
mod session;
mod terminal;

use std::time::Duration;

use ferritic_core::{Color, TextEngine, TextStyle, Theme, WindowOptions};
use ferritic_text::SystemText;

fn main() -> Result<(), String> {
    let mut cols = 80usize;
    let mut rows = 24usize;
    let mut font_size = 16.0f32;
    let mut shell = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--shell" => shell = args.next(),
            "--cols" => {
                cols = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(cols)
            }
            "--rows" => {
                rows = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(rows)
            }
            "--font-size" => {
                font_size = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(font_size)
            }
            "--help" | "-h" => {
                println!(
                    "usage: ferritic-term [--shell PATH] [--cols N] [--rows N] [--font-size PX]"
                );
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let shell = shell
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(default_shell);

    let mut text = SystemText::new_monospace()?;
    text.set_scale(1.0);
    let style = TextStyle::new(font_size, Color::WHITE);
    let cell_w = text.measure("M", &style).width.max(1.0);
    let cell_h = text.line_height(font_size).max(1.0);
    let width = (cell_w * cols as f32).ceil().max(1.0) as u32;
    let height = (cell_h * rows as f32).ceil().max(1.0) as u32;

    let session = session::Session::spawn(&shell, cols as u16, rows as u16)
        .map_err(|error| format!("failed to start {shell}: {error}"))?;
    let ui = terminal::TerminalUi::new(cols, rows, font_size, cell_w, cell_h);
    let app = app::TermApp::new(session, ui);
    let options = WindowOptions {
        title: "Terminal".to_string(),
        width,
        height,
        theme: Theme::dark(),
    };

    ferritic_orbital::run_polling_with_text(app, options, Duration::from_millis(8), text)
}

fn default_shell() -> String {
    if cfg!(target_os = "redox") && std::path::Path::new("/usr/bin/ion").exists() {
        "/usr/bin/ion".to_string()
    } else {
        "sh".to_string()
    }
}
