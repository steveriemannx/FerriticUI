//! The Orbital platform layer for Redox: an `orbclient` window, the
//! `ferritic-soft` CPU renderer and an event-driven loop.
//!
//! Orbital has no GPU API, so unlike [`ferritic_winit`] this backend never
//! touches blade. The scene is rasterized into an RGBA8 buffer and copied into
//! the orbclient window buffer, which is then synced to the display server.
//!
//! The event mapping mirrors the winit layer: pointer moves/buttons/scroll,
//! keys with shift/ctrl state, text input, focus and clipboard.

#![forbid(unsafe_code)]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::time::{Duration, Instant};

use ferritic_core::{
    App, Constraints, Ctx, Cursor, Event, EventCtx, Key, Point, PointerButton, Prim, Rect, Scene,
    Size, StaticUi, TextEngine, WindowOptions,
};
use ferritic_soft::SoftRenderer;
use ferritic_text::SystemText;
use orbclient::{EventOption, KeyEvent, MediaKind, Renderer, Window, WindowFlag, WindowFlags};

/// Runs an application in an Orbital window until it closes.
pub fn run<A: App>(app: A, options: WindowOptions) -> Result<(), String> {
    let exit_after = std::env::var("FERRITIC_EXIT_AFTER_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis);

    // Async windows poll instead of blocking, so the smoke-test timer can
    // interrupt an idle event loop.
    let mut flags = vec![WindowFlag::Resizable];
    if exit_after.is_some() {
        flags.push(WindowFlag::Async);
    }

    let (display_width, display_height) =
        orbclient::get_display_size().unwrap_or((options.width, options.height));
    let x = display_width.saturating_sub(options.width) as i32 / 2;
    let y = display_height.saturating_sub(options.height) as i32 / 2;
    let window = Window::new_flags(
        x,
        y,
        options.width,
        options.height,
        &options.title,
        WindowFlags::new(&flags),
    )
    .ok_or_else(|| "unable to open an Orbital window".to_string())?;

    let mut text = SystemText::new()?;
    text.set_scale(1.0);

    let mut platform = Platform {
        app,
        theme: options.theme,
        window,
        text,
        renderer: SoftRenderer::new(),
        scene: Scene::default(),
        event_cx: EventCtx::default(),
        size: Size::ZERO,
        scale: 1.0,
        dirty: true,
        shift: false,
        ctrl: false,
        buttons: (false, false, false),
        should_exit: false,
        exit_after: exit_after.map(|delay| Instant::now() + delay),
    };
    platform.app.view_mut().apply_theme(&platform.theme);
    platform.render();

    let polling = exit_after.is_some();
    while !platform.should_exit {
        if let Some(deadline) = platform.exit_after {
            if Instant::now() >= deadline {
                break;
            }
        }
        let events: Vec<orbclient::Event> = platform.window.events().collect();
        if events.is_empty() {
            if polling {
                std::thread::sleep(Duration::from_millis(8));
            } else {
                // A blocking window that reports no events has lost its
                // connection to the display server.
                break;
            }
        }
        for raw in events {
            platform.handle(raw);
            if platform.should_exit {
                break;
            }
        }
        if platform.dirty && !platform.should_exit {
            platform.render();
        }
    }
    Ok(())
}

struct Platform<A: App> {
    app: A,
    theme: ferritic_core::Theme,
    window: Window,
    text: SystemText,
    renderer: SoftRenderer,
    scene: Scene,
    event_cx: EventCtx,
    size: Size,
    scale: f32,
    dirty: bool,
    shift: bool,
    ctrl: bool,
    /// Last known left/middle/right button state; orbclient reports the whole
    /// state per event, so we diff it to emit press/release events.
    buttons: (bool, bool, bool),
    should_exit: bool,
    exit_after: Option<Instant>,
}

impl<A: App> Platform<A> {
    fn handle(&mut self, raw: orbclient::Event) {
        if std::env::var_os("FERRITIC_DEBUG_EVENTS").is_some() {
            eprintln!("ferritic: event {:?}", raw.to_option());
        }
        match raw.to_option() {
            EventOption::Quit(_) => self.should_exit = true,
            EventOption::Focus(focus) => {
                let event = if focus.focused {
                    Event::FocusGained
                } else {
                    Event::FocusLost
                };
                self.dispatch(event);
            }
            EventOption::Mouse(mouse) => {
                let point = Point::new(mouse.x as f32 / self.scale, mouse.y as f32 / self.scale);
                self.event_cx.pointer = point;
                self.dispatch(Event::PointerMoved(point));
            }
            EventOption::Button(button) => {
                let next = (button.left, button.middle, button.right);
                let previous = self.buttons;
                self.buttons = next;
                let point = self.event_cx.pointer;
                let buttons = [
                    (previous.0, next.0, PointerButton::Primary),
                    (previous.1, next.1, PointerButton::Middle),
                    (previous.2, next.2, PointerButton::Secondary),
                ];
                for (was, is, pointer) in buttons {
                    if was == is {
                        continue;
                    }
                    self.dispatch(if is {
                        Event::PointerDown(point, pointer)
                    } else {
                        Event::PointerUp(point, pointer)
                    });
                }
            }
            EventOption::Scroll(scroll) => self.dispatch(Event::Wheel(scroll.y as f32)),
            EventOption::Key(key) => self.handle_key(key),
            EventOption::TextInput(input) => {
                if !input.character.is_control() {
                    self.dispatch_key(Key::Character(input.character), true);
                }
            }
            EventOption::Resize(_) => self.dirty = true,
            EventOption::Scale(scale) => {
                if scale.scale > 0 && scale.baseline > 0 {
                    self.scale = scale.scale as f32 / scale.baseline as f32;
                    self.text.set_scale(self.scale);
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        let pressed = key.pressed;
        match key.scancode {
            orbclient::K_LEFT_SHIFT | orbclient::K_RIGHT_SHIFT => {
                self.shift = pressed;
            }
            orbclient::K_LEFT_CTRL | orbclient::K_RIGHT_CTRL => {
                self.ctrl = pressed;
            }
            _ => self.dispatch_key(translate_key(key.scancode), pressed),
        }
    }

    fn dispatch_key(&mut self, key: Key, pressed: bool) {
        if pressed && self.ctrl {
            self.event_cx.clipboard_get = self.window.clipboard().map(|(_, text)| text);
        }
        self.dispatch(Event::Key {
            key,
            pressed,
            repeat: false,
            shift: self.shift,
            ctrl: self.ctrl,
        });
        if let Some(text) = self.event_cx.clipboard_set.take() {
            let _ = self.window.set_clipboard(&text, MediaKind::Text);
        }
    }

    fn dispatch(&mut self, event: Event) {
        self.event_cx.shift = self.shift;
        self.event_cx.ctrl = self.ctrl;
        let message = self
            .app
            .view_mut()
            .event(&mut self.event_cx, &event, &mut self.text);

        let mut dirty = self.event_cx.needs_redraw;
        let mut cursor = self.event_cx.cursor;
        if let Some(message) = message {
            let mut ctx = Ctx::default();
            self.app.update(message, &mut ctx);
            dirty |= ctx.needs_redraw;
            if ctx.close_requested {
                self.should_exit = true;
            }
            if ctx.cursor != Cursor::Default {
                cursor = ctx.cursor;
            }
        }
        // orbclient only exposes cursor visibility, not per-widget shapes.
        let _ = cursor;

        self.event_cx.needs_redraw = false;
        if dirty {
            self.dirty = true;
        }
    }

    fn render(&mut self) {
        let width = self.window.width();
        let height = self.window.height();
        if width == 0 || height == 0 {
            return;
        }
        self.size = Size::new(width as f32 / self.scale, height as f32 / self.scale);
        let size = self.size;

        let mut scene = std::mem::take(&mut self.scene);
        scene.clear();
        scene.prims.push(Prim::Rect {
            rect: Rect::from_size(size),
            color: self.theme.background,
            radius: 0.0,
            border_width: 0.0,
            border_color: ferritic_core::Color::TRANSPARENT,
            clip: None,
        });
        {
            let text = &mut self.text;
            let ui = self.app.view_mut();
            let _ = ui.measure(&mut *text, Constraints::loose(size));
            ui.arrange(Rect::from_size(size));
            ui.paint(&mut *text, &mut scene);
        }

        let atlas = self.text.atlas_update();
        self.renderer.resize(width, height, self.scale);
        let pixels = self.renderer.render(&scene, atlas.as_ref());
        let data = self.window.data_mut();
        for (pixel, color) in pixels.chunks_exact(4).zip(data.iter_mut()) {
            color.data = ((pixel[3] as u32) << 24)
                | ((pixel[0] as u32) << 16)
                | ((pixel[1] as u32) << 8)
                | (pixel[2] as u32);
        }
        self.window.sync();

        self.scene = scene;
        self.dirty = false;
    }
}

/// Maps an orbclient (XT set 1) scancode to the toolkit's key.
fn translate_key(scancode: u8) -> Key {
    match scancode {
        orbclient::K_TAB => Key::Tab,
        orbclient::K_ENTER => Key::Enter,
        orbclient::K_ESC => Key::Escape,
        orbclient::K_BKSP => Key::Backspace,
        orbclient::K_DEL => Key::Delete,
        orbclient::K_SPACE => Key::Space,
        orbclient::K_LEFT => Key::ArrowLeft,
        orbclient::K_RIGHT => Key::ArrowRight,
        orbclient::K_UP => Key::ArrowUp,
        orbclient::K_DOWN => Key::ArrowDown,
        orbclient::K_HOME => Key::Home,
        orbclient::K_END => Key::End,
        orbclient::K_PGUP => Key::PageUp,
        orbclient::K_PGDN => Key::PageDown,
        _ => Key::Other,
    }
}
