//! The winit platform layer: one window, a blade swap chain, and an
//! event-driven loop that only renders when something changed.

#![forbid(unsafe_code)]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use ferritic_blade::{Gpu, WindowSurface};
use ferritic_core::{
    App, Constraints, Ctx, Cursor, Event, EventCtx, Key, PointerButton, Prim, Rect, Scene, Size,
    StaticUi, TextEngine, WindowOptions,
};
use ferritic_text::SystemText;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::window::{CursorIcon, Window};

/// Runs an application until the window closes.
pub fn run<A: App>(app: A, options: WindowOptions) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut platform = Platform {
        app,
        options,
        window: None,
        gpu: None,
        surface: None,
        text: None,
        event_cx: EventCtx::default(),
        scene: Scene::default(),
        size: Size::ZERO,
        scale: 1.0,
        dirty: true,
        shift: false,
        ctrl: false,
        should_exit: false,
        error: None,
        frames: 0,
        exit_after: None,
    };

    event_loop.run_app(&mut platform).map_err(|error| error.to_string())?;

    match platform.error.take() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct Platform<A: App> {
    app: A,
    options: WindowOptions,
    window: Option<Window>,
    gpu: Option<Gpu>,
    surface: Option<WindowSurface>,
    text: Option<SystemText>,
    event_cx: EventCtx,
    scene: Scene,
    size: Size,
    scale: f32,
    dirty: bool,
    shift: bool,
    ctrl: bool,
    should_exit: bool,
    error: Option<String>,
    frames: u64,
    exit_after: Option<std::time::Instant>,
}

impl<A: App> ApplicationHandler for Platform<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(error) = self.initialize(event_loop) {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                self.update_size();
                self.resize_surface();
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor as f32;
                if let Some(text) = self.text.as_mut() {
                    text.set_scale(self.scale);
                }
                self.update_size();
                self.resize_surface();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.render_frame();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let point = ferritic_core::Point::new(
                    position.x as f32 / self.scale,
                    position.y as f32 / self.scale,
                );
                self.event_cx.pointer = point;
                self.dispatch(event_loop, Event::PointerMoved(point));
            }
            WindowEvent::CursorLeft { .. } => {
                self.dispatch(event_loop, Event::PointerLeft);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    _ => return,
                };
                let point = self.event_cx.pointer;
                let event = match state {
                    ElementState::Pressed => Event::PointerDown(point, button),
                    ElementState::Released => Event::PointerUp(point, button),
                };
                self.dispatch(event_loop, event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                self.dispatch(event_loop, Event::Wheel(delta));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = translate_key(&event);
                let shift = event.state == ElementState::Pressed && self.shift;
                let ctrl = event.state == ElementState::Pressed && self.ctrl;
                if ctrl {
                    self.event_cx.clipboard_get = read_clipboard();
                }
                self.dispatch(
                    event_loop,
                    Event::Key {
                        key,
                        pressed: event.state == ElementState::Pressed,
                        repeat: event.repeat,
                        shift,
                        ctrl,
                    },
                );
                if let Some(text) = self.event_cx.clipboard_set.take() {
                    write_clipboard(&text);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift = modifiers.state().shift_key();
                self.ctrl = modifiers.state().control_key();
            }
            WindowEvent::Ime(ime) => {
                let event = match ime {
                    winit::event::Ime::Commit(text) => Event::ImeCommit(text),
                    winit::event::Ime::Preedit(text, _) => Event::ImePreedit(text),
                    winit::event::Ime::Disabled => Event::ImeDisabled,
                    winit::event::Ime::Enabled => return,
                };
                self.dispatch(event_loop, event);
            }
            WindowEvent::Focused(false) => {
                self.dispatch(event_loop, Event::FocusLost);
            }
            _ => {
                let _ = window;
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.should_exit {
            event_loop.exit();
            return;
        }
        if let Some(deadline) = self.exit_after {
            if std::time::Instant::now() >= deadline {
                eprintln!("ferritic: smoke exit after {} frames", self.frames);
                event_loop.exit();
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        if self.dirty {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

impl<A: App> Platform<A> {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let attributes = Window::default_attributes()
            .with_title(&self.options.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.options.width as f64,
                self.options.height as f64,
            ));
        let window = event_loop
            .create_window(attributes)
            .map_err(|error| error.to_string())?;
        self.scale = window.scale_factor() as f32;
        self.size = Size::new(self.options.width as f32, self.options.height as f32);

        let gpu = Gpu::new()?;
        let surface = WindowSurface::new(&gpu, &window, self.size, self.scale)?;
        let mut text = SystemText::new()?;
        text.set_scale(self.scale);
        eprintln!(
            "ferritic: surface format {:?}, scale {}, font {}",
            surface.format(),
            self.scale,
            text.font_name()
        );

        self.window = Some(window);
        self.gpu = Some(gpu);
        self.surface = Some(surface);
        self.text = Some(text);
        self.app.view_mut().apply_theme(&self.options.theme);
        if let Ok(milliseconds) = std::env::var("FERRITIC_EXIT_AFTER_MS") {
            if let Ok(milliseconds) = milliseconds.parse::<u64>() {
                self.exit_after = Some(
                    std::time::Instant::now() + std::time::Duration::from_millis(milliseconds),
                );
            }
        }
        self.dirty = true;
        self.window.as_ref().unwrap().request_redraw();
        Ok(())
    }

    fn update_size(&mut self) {
        if let Some(window) = &self.window {
            let size = window.inner_size();
            self.size = Size::new(
                size.width as f32 / self.scale,
                size.height as f32 / self.scale,
            );
        }
    }

    fn resize_surface(&mut self) {
        if let (Some(gpu), Some(surface)) = (self.gpu.as_ref(), self.surface.as_mut()) {
            if !self.size.is_empty() {
                surface.resize(gpu, self.size, self.scale);
            }
        }
    }

    fn request_redraw(&mut self) {
        self.dirty = true;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn dispatch(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        let Some(text) = self.text.as_mut() else {
            return;
        };
        let message = self.app.view_mut().event(&mut self.event_cx, &event, text);

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

        if let Some(window) = &self.window {
            window.set_ime_allowed(self.event_cx.ime_allowed);
            if let Some(cursor) = self.event_cx.ime_cursor {
                window.set_ime_cursor_area(
                    winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
                        cursor.x as f64,
                        cursor.y as f64,
                    )),
                    winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
                        cursor.width as f64,
                        cursor.height as f64,
                    )),
                );
            }
            let icon = match cursor {
                Cursor::PointingHand => CursorIcon::Pointer,
                Cursor::Text => CursorIcon::Text,
                Cursor::Default => CursorIcon::Default,
            };
            window.set_cursor(icon);
        }

        if dirty {
            self.request_redraw();
        }
        if self.should_exit {
            event_loop.exit();
        }
    }

    fn render_frame(&mut self) {
        if self.size.is_empty() {
            return;
        }
        let (Some(gpu), Some(surface), Some(text)) = (
            self.gpu.as_mut(),
            self.surface.as_mut(),
            self.text.as_mut(),
        ) else {
            return;
        };

        let size = self.size;
        {
            let ui = self.app.view_mut();
            let _ = ui.measure(text, Constraints::loose(size));
            ui.arrange(Rect::from_size(size));
        }

        let mut scene = std::mem::take(&mut self.scene);
        scene.clear();
        scene.prims.push(Prim::Rect {
            rect: Rect::from_size(size),
            color: self.options.theme.background,
            radius: 0.0,
            border_width: 0.0,
            border_color: ferritic_core::Color::TRANSPARENT,
            clip: None,
        });
        self.app.view_mut().paint(text, &mut scene);

        let atlas = text.atlas_update();
        if let Err(error) = surface.render(gpu, &scene, atlas.as_ref()) {
            log::error!("render failed: {error}");
        }

        self.scene = scene;
        self.dirty = false;
        self.frames += 1;
    }
}

fn translate_key(event: &winit::event::KeyEvent) -> Key {
    match &event.logical_key {
        WinitKey::Named(NamedKey::Tab) => Key::Tab,
        WinitKey::Named(NamedKey::Enter) => Key::Enter,
        WinitKey::Named(NamedKey::Escape) => Key::Escape,
        WinitKey::Named(NamedKey::Backspace) => Key::Backspace,
        WinitKey::Named(NamedKey::Delete) => Key::Delete,
        WinitKey::Named(NamedKey::Space) => Key::Space,
        WinitKey::Named(NamedKey::ArrowLeft) => Key::ArrowLeft,
        WinitKey::Named(NamedKey::ArrowRight) => Key::ArrowRight,
        WinitKey::Named(NamedKey::ArrowUp) => Key::ArrowUp,
        WinitKey::Named(NamedKey::ArrowDown) => Key::ArrowDown,
        WinitKey::Named(NamedKey::Home) => Key::Home,
        WinitKey::Named(NamedKey::End) => Key::End,
        WinitKey::Named(NamedKey::PageUp) => Key::PageUp,
        WinitKey::Named(NamedKey::PageDown) => Key::PageDown,
        WinitKey::Character(text) => text
            .chars()
            .next()
            .map(Key::Character)
            .unwrap_or(Key::Other),
        WinitKey::Named(NamedKey::Shift) => Key::Other,
        _ => Key::Other,
    }
}


#[cfg(target_os = "macos")]
fn read_clipboard() -> Option<String> {
    let output = std::process::Command::new("pbpaste").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(not(target_os = "macos"))]
fn read_clipboard() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn write_clipboard(text: &str) {
    use std::io::Write;
    if let Ok(mut child) = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

#[cfg(not(target_os = "macos"))]
fn write_clipboard(_text: &str) {}
