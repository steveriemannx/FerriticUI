//! FerriticUI: a static retained-mode GUI library on top of `blade-graphics`.
//!
//! A UI is built once into a flat, statically typed struct (see the `ui!` macro
//! in a later milestone), never rebuilt and never diffed; values change only
//! through explicit setters, and frames are produced only when something
//! changed.

#![forbid(unsafe_code)]

pub use ferritic_core::{App, Theme, WindowOptions};
pub use ferritic_macros::ui;

pub mod core {
    pub use ferritic_core::*;
}

pub mod render {
    pub use ferritic_blade::{render_offscreen, Gpu, Snapshot, WindowSurface};
}

pub use ferritic_text::SystemText;

pub mod prelude {
    pub use ferritic_core::prelude::*;
}

/// Builds an application from the theme and runs it in a window until it
/// closes.
pub fn run<A: App>(
    options: WindowOptions,
    build: impl FnOnce(&Theme) -> A,
) -> Result<(), String> {
    let app = build(&options.theme);
    ferritic_winit::run(app, options)
}
