//! The terminal view: ransid emulation, a cell grid and FerriticUI painting.

use ferritic_core::{
    Color, Constraints, Event, EventCtx, Key, Point, Prim, Rect, Scene, Size, StaticUi, TextEngine,
    TextStyle, Theme,
};

/// Messages from the view to the application.
#[derive(Clone, Debug)]
pub enum Msg {
    /// Bytes to write to the PTY master.
    Input(Vec<u8>),
}

#[derive(Clone, Copy)]
struct Cell {
    c: char,
    fg: Color,
    bg: Color,
    bold: bool,
    underlined: bool,
    strikethrough: bool,
}

impl Cell {
    fn blank(bg: Color, fg: Color) -> Self {
        Self {
            c: '\0',
            fg,
            bg,
            bold: false,
            underlined: false,
            strikethrough: false,
        }
    }
}

/// The full-window terminal widget.
pub struct TerminalUi {
    console: ransid::Console,
    grid: Vec<Cell>,
    alt_grid: Vec<Cell>,
    alternate: bool,
    font_size: f32,
    cell_w: f32,
    cell_h: f32,
    rect: Rect,
    pending_input: Vec<u8>,
}

impl TerminalUi {
    pub fn new(cols: usize, rows: usize, font_size: f32, cell_w: f32, cell_h: f32) -> Self {
        let console = ransid::Console::new(cols, rows);
        let fg = cvt(&console.state.foreground);
        let bg = cvt(&console.state.background);
        let cell = Cell::blank(bg, fg);
        Self {
            console,
            grid: vec![cell; cols.saturating_mul(rows)],
            alt_grid: vec![cell; cols.saturating_mul(rows)],
            alternate: false,
            font_size,
            cell_w,
            cell_h,
            rect: Rect::ZERO,
            pending_input: Vec::new(),
        }
    }

    /// Feeds PTY output through the terminal emulator.
    pub fn write(&mut self, bytes: &[u8]) {
        let Self {
            console,
            grid,
            alt_grid,
            alternate,
            pending_input,
            ..
        } = self;
        let cols = console.state.w;
        let rows = console.state.h;
        let clear_bg = cvt(&console.state.background);
        let clear_fg = cvt(&console.state.foreground);
        console.write(bytes, |event| match event {
            ransid::Event::Char {
                x,
                y,
                c,
                bold,
                underlined,
                strikethrough,
                color,
                ..
            } => {
                if x < cols && y < rows {
                    if let Some(cell) = grid.get_mut(y * cols + x) {
                        cell.c = c;
                        cell.fg = cvt(&color);
                        cell.bold = bold;
                        cell.underlined = underlined;
                        cell.strikethrough = strikethrough;
                    }
                }
            }
            ransid::Event::Rect { x, y, w, h, color } => {
                let bg = cvt(&color);
                for row in y..(y + h).min(rows) {
                    for col in x..(x + w).min(cols) {
                        if let Some(cell) = grid.get_mut(row * cols + col) {
                            cell.c = '\0';
                            cell.bg = bg;
                        }
                    }
                }
            }
            ransid::Event::Input { data } => pending_input.extend_from_slice(data),
            ransid::Event::ScreenBuffer {
                alternate: next,
                clear: clear_screen,
            } => {
                if *alternate != next {
                    std::mem::swap(grid, alt_grid);
                }
                if clear_screen {
                    for cell in grid.iter_mut() {
                        *cell = Cell::blank(clear_bg, clear_fg);
                    }
                }
                *alternate = next;
            }
            ransid::Event::Move {
                from_x,
                from_y,
                to_x,
                to_y,
                w,
                h,
            } => {
                let width = w.min(cols);
                let height = h.min(rows);
                for raw in 0..height {
                    let row = if from_y > to_y { raw } else { height - raw - 1 };
                    let from_row = from_y + row;
                    let to_row = to_y + row;
                    if from_row >= rows || to_row >= rows {
                        continue;
                    }
                    let from_col = from_x.min(cols);
                    let to_col = to_x.min(cols);
                    let len = width
                        .min(cols.saturating_sub(from_col))
                        .min(cols.saturating_sub(to_col));
                    if len == 0 {
                        continue;
                    }
                    let source = from_row * cols + from_col;
                    let target = to_row * cols + to_col;
                    grid.copy_within(source..source + len, target);
                }
            }
            ransid::Event::Resize { .. } => {
                // The window size is driven by arrange()/sync_size(), not by
                // escape sequences.
            }
            ransid::Event::Title { .. } => {}
        });
    }

    /// Terminal responses (e.g. cursor-position reports) that must be written
    /// back to the PTY.
    pub fn take_input(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending_input)
    }

    /// Reconciles the emulator grid with the window's arranged size.
    pub fn sync_size(&mut self) -> Option<(u16, u16)> {
        if self.cell_w <= 0.0 || self.cell_h <= 0.0 {
            return None;
        }
        if self.rect.width < self.cell_w || self.rect.height < self.cell_h {
            return None;
        }
        let cols = (self.rect.width / self.cell_w).floor().max(1.0) as usize;
        let rows = (self.rect.height / self.cell_h).floor().max(1.0) as usize;
        if cols == self.console.state.w && rows == self.console.state.h {
            return None;
        }
        self.resize(cols, rows);
        Some((cols as u16, rows as u16))
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        let old_cols = self.console.state.w;
        let old_rows = self.console.state.h;
        let clear_bg = cvt(&self.console.state.background);
        let clear_fg = cvt(&self.console.state.foreground);
        self.console.resize(cols, rows);
        self.grid = resize_grid(
            std::mem::take(&mut self.grid),
            old_cols,
            old_rows,
            cols,
            rows,
            clear_bg,
            clear_fg,
        );
        self.alt_grid = resize_grid(
            std::mem::take(&mut self.alt_grid),
            old_cols,
            old_rows,
            cols,
            rows,
            clear_bg,
            clear_fg,
        );
    }

    fn paint_grid(&mut self, text: &mut dyn TextEngine, scene: &mut Scene) {
        let rect = self.rect;
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let cols = self.console.state.w;
        let rows = self.console.state.h;
        if cols == 0 || rows == 0 || cols * rows > self.grid.len() {
            return;
        }
        let default_bg = cvt(&self.console.state.background);
        scene.prims.push(Prim::Rect {
            rect,
            color: default_bg,
            radius: 0.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            clip: None,
        });

        let cursor = (self.console.state.x, self.console.state.y);
        let cursor_visible = self.console.state.cursor;
        let mut quads = Vec::new();
        for y in 0..rows {
            for x in 0..cols {
                let cell = self.grid[y * cols + x];
                let mut fg = cell.fg;
                let mut bg = cell.bg;
                let is_cursor = cursor_visible && cursor == (x, y);
                if is_cursor {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let cell_rect = Rect::new(
                    rect.x + x as f32 * self.cell_w,
                    rect.y + y as f32 * self.cell_h,
                    self.cell_w,
                    self.cell_h,
                );
                if is_cursor || bg != default_bg {
                    scene.prims.push(Prim::Rect {
                        rect: cell_rect,
                        color: bg,
                        radius: 0.0,
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        clip: None,
                    });
                }
                if cell.c != '\0' && cell.c != ' ' && !cell.c.is_control() {
                    let mut encoding = [0u8; 4];
                    let glyph = cell.c.encode_utf8(&mut encoding);
                    let style = TextStyle::new(self.font_size, fg);
                    quads.clear();
                    text.layout(glyph, &style, Point::new(cell_rect.x, cell_rect.y), &mut quads);
                    if cell.bold {
                        text.layout(
                            glyph,
                            &style,
                            Point::new(cell_rect.x + 1.0, cell_rect.y),
                            &mut quads,
                        );
                    }
                    for quad in &quads {
                        scene.prims.push(Prim::Glyph {
                            rect: quad.rect,
                            uv: quad.uv,
                            color: quad.color,
                            clip: None,
                        });
                    }
                }
                if cell.underlined {
                    scene.prims.push(Prim::Rect {
                        rect: Rect::new(cell_rect.x, cell_rect.bottom() - 1.5, cell_rect.width, 1.0),
                        color: fg,
                        radius: 0.0,
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        clip: None,
                    });
                }
                if cell.strikethrough {
                    scene.prims.push(Prim::Rect {
                        rect: Rect::new(
                            cell_rect.x,
                            cell_rect.y + cell_rect.height * 0.5,
                            cell_rect.width,
                            1.0,
                        ),
                        color: fg,
                        radius: 0.0,
                        border_width: 0.0,
                        border_color: Color::TRANSPARENT,
                        clip: None,
                    });
                }
            }
        }
    }
}

impl StaticUi for TerminalUi {
    type Msg = Msg;

    fn apply_theme(&mut self, _theme: &Theme) {}

    fn measure(&mut self, _text: &mut dyn TextEngine, constraints: Constraints) -> Size {
        let width = if constraints.max.width.is_finite() && constraints.max.width > 0.0 {
            constraints.max.width
        } else {
            self.cell_w * 80.0
        };
        let height = if constraints.max.height.is_finite() && constraints.max.height > 0.0 {
            constraints.max.height
        } else {
            self.cell_h * 24.0
        };
        Size::new(width, height)
    }

    fn arrange(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn paint(&mut self, text: &mut dyn TextEngine, scene: &mut Scene) {
        self.paint_grid(text, scene);
    }

    fn event(
        &mut self,
        cx: &mut EventCtx,
        event: &Event,
        _text: &mut dyn TextEngine,
    ) -> Option<Self::Msg> {
        match event {
            Event::Key {
                key,
                pressed: true,
                ctrl,
                ..
            } => {
                if *ctrl && matches!(key, Key::Character('v') | Key::Character('V')) {
                    if let Some(clipboard) = cx.clipboard_get.take() {
                        return Some(Msg::Input(clipboard.into_bytes()));
                    }
                    return None;
                }
                key_bytes(*key, *ctrl).map(Msg::Input)
            }
            _ => None,
        }
    }

    fn focus_next(
        &mut self,
        _cx: &mut EventCtx,
        _forward: bool,
        _text: &mut dyn TextEngine,
    ) {
    }
}

fn cvt(color: &ransid::Color) -> Color {
    let rgb = color.as_rgb();
    Color::rgba8(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
        255,
    )
}

fn resize_grid(
    old: Vec<Cell>,
    old_cols: usize,
    old_rows: usize,
    cols: usize,
    rows: usize,
    clear_bg: Color,
    clear_fg: Color,
) -> Vec<Cell> {
    let mut new = vec![Cell::blank(clear_bg, clear_fg); cols.saturating_mul(rows)];
    let copy_rows = old_rows.min(rows);
    let copy_cols = old_cols.min(cols);
    for y in 0..copy_rows {
        for x in 0..copy_cols {
            new[y * cols + x] = old[y * old_cols + x];
        }
    }
    new
}

fn key_bytes(key: Key, ctrl: bool) -> Option<Vec<u8>> {
    let bytes = match key {
        Key::Enter => b"\r".to_vec(),
        Key::Backspace => b"\x7f".to_vec(),
        Key::Tab => b"\t".to_vec(),
        Key::Escape => b"\x1b".to_vec(),
        // Space arrives through TextInput as Key::Character(' '); emitting it
        // here as well would type two spaces per key press.
        Key::Space => return None,
        Key::ArrowUp => b"\x1b[A".to_vec(),
        Key::ArrowDown => b"\x1b[B".to_vec(),
        Key::ArrowRight => b"\x1b[C".to_vec(),
        Key::ArrowLeft => b"\x1b[D".to_vec(),
        Key::Home => b"\x1b[H".to_vec(),
        Key::End => b"\x1b[F".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::Character(c) => {
            if ctrl {
                if c == ' ' {
                    vec![0]
                } else {
                    let lower = c.to_ascii_lowercase();
                    if lower.is_ascii_alphabetic() {
                        vec![(lower as u8) - b'a' + 1]
                    } else {
                        return None;
                    }
                }
            } else {
                let mut encoding = [0u8; 4];
                c.encode_utf8(&mut encoding).as_bytes().to_vec()
            }
        }
        Key::Other => return None,
    };
    Some(bytes)
}
