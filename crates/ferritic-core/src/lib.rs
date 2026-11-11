//! The core of FerriticUI: geometry, layout, theme, scene and the static UI traits.
//!
//! The library is a **static retained-mode** GUI: a `Ui` is built once, is
//! never rebuilt and never diffed; values change only through explicit
//! setters. The layout of a `Ui` is a flat struct whose node graph is
//! generated at compile time (see `ferritic-macros`), so this crate only
//! contains the pieces that are shared between generated code and widgets.

#![forbid(unsafe_code)]

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

/// A straight-alpha sRGB color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

    pub fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// From 8-bit sRGB components.
    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// The components as straight-alpha sRGB in `0..=1`.
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Premultiplied components, as the renderer blends them.
    pub fn premultiplied(self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self { width: 0.0, height: 0.0 };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn max(self, other: Self) -> Self {
        Self::new(self.width.max(other.width), self.height.max(other.height))
    }

    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn from_size(size: Size) -> Self {
        Self::new(0.0, 0.0, size.width, size.height)
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.right() && p.y < self.bottom()
    }

    pub fn inset(self, amount: f32) -> Self {
        Self::new(
            self.x + amount,
            self.y + amount,
            (self.width - amount * 2.0).max(0.0),
            (self.height - amount * 2.0).max(0.0),
        )
    }
}

/// The constraints a parent gives a child while measuring.
#[derive(Clone, Copy, Debug)]
pub struct Constraints {
    pub min: Size,
    pub max: Size,
}

impl Default for Constraints {
    fn default() -> Self {
        Self {
            min: Size::ZERO,
            max: Size::new(f32::INFINITY, f32::INFINITY),
        }
    }
}

impl Constraints {
    pub fn loose(max: Size) -> Self {
        Self { min: Size::ZERO, max }
    }

    pub fn tight(size: Size) -> Self {
        Self { min: size, max: size }
    }

    pub fn constrain(&self, size: Size) -> Size {
        Size::new(
            size.width.clamp(self.min.width, self.max.width),
            size.height.clamp(self.min.height, self.max.height),
        )
    }
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub size: f32,
    pub color: Color,
}

impl TextStyle {
    pub const fn new(size: f32, color: Color) -> Self {
        Self { size, color }
    }
}

/// One glyph of a laid-out run, in logical pixels. `uv` is in atlas texture
/// coordinates (`0..1`).
#[derive(Clone, Copy, Debug)]
pub struct GlyphQuad {
    pub rect: Rect,
    pub uv: [f32; 4],
    pub color: Color,
}

/// A texture update of the glyph atlas. `pixels` is `width * height` alpha
/// (R8) samples.
#[derive(Clone, Debug)]
pub struct AtlasUpdate {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub version: u64,
}

/// Everything the widgets need from text: measurement, glyph layout and the
/// glyph atlas.
pub trait TextEngine {
    fn measure(&mut self, text: &str, style: &TextStyle) -> Size;

    fn layout(&mut self, text: &str, style: &TextStyle, origin: Point, out: &mut Vec<GlyphQuad>);

    /// The current atlas contents, if it changed since the last call.
    fn atlas_update(&mut self) -> Option<AtlasUpdate>;

    fn line_height(&mut self, size: f32) -> f32;

    /// The pixels-per-logical-pixel factor the engine rasterizes glyphs at.
    fn set_scale(&mut self, _scale: f32) {}

    /// The full current atlas, for a renderer that starts with a fresh
    /// texture (offscreen snapshots) instead of a long-lived one.
    fn atlas_snapshot(&mut self) -> Option<AtlasUpdate> {
        None
    }

    /// Horizontal offset of the caret before `byte_index`.
    fn caret_x(&mut self, text: &str, style: &TextStyle, byte_index: usize) -> f32 {
        let index = byte_index.min(text.len());
        let index = if text.is_char_boundary(index) {
            index
        } else {
            0
        };
        self.measure(&text[..index], style).width
    }
}

pub struct MeasureCx<'a> {
    pub text: &'a mut dyn TextEngine,
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct ButtonStyle {
    pub background: Color,
    pub hover: Color,
    pub pressed: Color,
    pub text: Color,
    pub padding_x: f32,
    pub padding_y: f32,
    pub radius: f32,
    pub text_size: f32,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub button: ButtonStyle,
    pub text_size: f32,
    pub padding: f32,
    pub spacing: f32,
    pub radius: f32,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            background: Color::rgba8(0xf5, 0xf5, 0xf7, 0xff),
            surface: Color::rgba8(0xff, 0xff, 0xff, 0xff),
            text: Color::rgba8(0x1c, 0x1c, 0x1e, 0xff),
            text_muted: Color::rgba8(0x60, 0x60, 0x68, 0xff),
            accent: Color::rgba8(0x0a, 0x64, 0xff, 0xff),
            button: ButtonStyle {
                background: Color::rgba8(0xe8, 0xe8, 0xec, 0xff),
                hover: Color::rgba8(0xdc, 0xdc, 0xe2, 0xff),
                pressed: Color::rgba8(0xcc, 0xcc, 0xd4, 0xff),
                text: Color::rgba8(0x1c, 0x1c, 0x1e, 0xff),
                padding_x: 14.0,
                padding_y: 8.0,
                radius: 6.0,
                text_size: 15.0,
            },
            text_size: 17.0,
            padding: 16.0,
            spacing: 10.0,
            radius: 8.0,
        }
    }

    pub fn dark() -> Self {
        Self {
            background: Color::rgba8(0x1e, 0x1e, 0x22, 0xff),
            surface: Color::rgba8(0x2a, 0x2a, 0x30, 0xff),
            text: Color::rgba8(0xf0, 0xf0, 0xf4, 0xff),
            text_muted: Color::rgba8(0xa0, 0xa0, 0xa8, 0xff),
            accent: Color::rgba8(0x4c, 0x9a, 0xff, 0xff),
            button: ButtonStyle {
                background: Color::rgba8(0x3a, 0x3a, 0x42, 0xff),
                hover: Color::rgba8(0x46, 0x46, 0x50, 0xff),
                pressed: Color::rgba8(0x54, 0x54, 0x60, 0xff),
                text: Color::rgba8(0xf0, 0xf0, 0xf4, 0xff),
                padding_x: 14.0,
                padding_y: 8.0,
                radius: 6.0,
                text_size: 15.0,
            },
            text_size: 17.0,
            padding: 16.0,
            spacing: 10.0,
            radius: 8.0,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

// ---------------------------------------------------------------------------
// Scene and painting
// ---------------------------------------------------------------------------

/// One drawable primitive in logical pixels, in paint order.
#[derive(Clone, Copy, Debug)]
pub enum Prim {
    /// A (optionally rounded) rectangle with an optional inside border.
    Rect {
        rect: Rect,
        color: Color,
        radius: f32,
        border_width: f32,
        border_color: Color,
        /// Axis-aligned clip (scissor) in logical pixels.
        clip: Option<Rect>,
    },
    Glyph {
        rect: Rect,
        uv: [f32; 4],
        color: Color,
        clip: Option<Rect>,
    },
    /// A soft shadow of a rounded rectangle: drawn behind a shape.
    Shadow {
        rect: Rect,
        radius: f32,
        color: Color,
        blur: f32,
        offset: (f32, f32),
        clip: Option<Rect>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub prims: Vec<Prim>,
}

impl Scene {
    pub fn clear(&mut self) {
        self.prims.clear();
    }
}

pub struct PaintCx<'a> {
    pub scene: &'a mut Scene,
    pub text: &'a mut dyn TextEngine,
    /// The current axis-aligned clip in logical pixels.
    pub clip: Option<Rect>,
}

impl PaintCx<'_> {
    /// Intersects the current clip with another one (and returns the old clip
    /// so the caller can restore it).
    pub fn set_clip(&mut self, clip: Option<Rect>) -> Option<Rect> {
        let old = self.clip;
        self.clip = match (old, clip) {
            (Some(a), Some(b)) => {
                let x = a.x.max(b.x);
                let y = a.y.max(b.y);
                let right = a.right().min(b.right());
                let bottom = a.bottom().min(b.bottom());
                if right <= x || bottom <= y {
                    Some(Rect::new(x, y, 0.0, 0.0))
                } else {
                    Some(Rect::new(x, y, right - x, bottom - y))
                }
            }
            (Some(a), None) => Some(a),
            (None, other) => other,
        };
        old
    }

    fn visible(&self, rect: Rect) -> bool {
        match self.clip {
            Some(clip) => {
                rect.x < clip.right()
                    && rect.right() > clip.x
                    && rect.y < clip.bottom()
                    && rect.bottom() > clip.y
            }
            None => true,
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if !self.visible(rect) {
            return;
        }
        self.scene.prims.push(Prim::Rect {
            rect,
            color,
            radius: 0.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            clip: self.clip,
        });
    }

    pub fn fill_rounded_rect(&mut self, rect: Rect, color: Color, radius: f32) {
        if !self.visible(rect) {
            return;
        }
        self.scene.prims.push(Prim::Rect {
            rect,
            color,
            radius,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            clip: self.clip,
        });
    }

    /// A rounded rectangle with an inside border of `border_width`.
    pub fn fill_rounded_rect_bordered(
        &mut self,
        rect: Rect,
        color: Color,
        radius: f32,
        border_width: f32,
        border_color: Color,
    ) {
        if !self.visible(rect) {
            return;
        }
        self.scene.prims.push(Prim::Rect {
            rect,
            color,
            radius,
            border_width,
            border_color,
            clip: self.clip,
        });
    }

    /// A soft shadow behind a rounded rectangle.
    pub fn fill_shadow(
        &mut self,
        rect: Rect,
        color: Color,
        radius: f32,
        blur: f32,
        offset: (f32, f32),
    ) {
        let bounds = Rect::new(
            rect.x + offset.0 - blur,
            rect.y + offset.1 - blur,
            rect.width + blur * 2.0,
            rect.height + blur * 2.0,
        );
        if !self.visible(bounds) {
            return;
        }
        self.scene.prims.push(Prim::Shadow {
            rect,
            radius,
            color,
            blur,
            offset,
            clip: self.clip,
        });
    }

    pub fn text(&mut self, text: &str, style: &TextStyle, origin: Point) {
        let mut quads = Vec::new();
        self.text.layout(text, style, origin, &mut quads);
        for quad in quads {
            if !self.visible(quad.rect) {
                continue;
            }
            self.scene.prims.push(Prim::Glyph {
                rect: quad.rect,
                uv: quad.uv,
                color: quad.color,
                clip: self.clip,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    Enter,
    Escape,
    Backspace,
    Delete,
    Space,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Character(char),
    Other,
}

#[derive(Clone, Debug)]
pub enum Event {
    PointerMoved(Point),
    PointerDown(Point, PointerButton),
    PointerUp(Point, PointerButton),
    PointerLeft,
    Wheel(f32),
    Key {
        key: Key,
        pressed: bool,
        repeat: bool,
        shift: bool,
        ctrl: bool,
    },
    /// The IME committed text.
    ImeCommit(String),
    /// The IME pre-edit (composition) string.
    ImePreedit(String),
    ImeDisabled,
    FocusGained,
    FocusLost,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Default,
    PointingHand,
    Text,
}

#[derive(Debug)]
pub struct EventCtx {
    pub hovered: Option<u32>,
    pub pressed: Option<u32>,
    pub focus: Option<u32>,
    pub cursor: Cursor,
    pub needs_redraw: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub pointer: Point,
    /// Set by a focused text input; the platform mirrors it to the window.
    pub ime_allowed: bool,
    /// Set by a focused text input; the platform positions the IME candidate
    /// window at this logical rectangle.
    pub ime_cursor: Option<Rect>,
    /// The clipboard contents the platform reads before dispatching a key.
    pub clipboard_get: Option<String>,
    /// Text the platform writes to the clipboard after dispatching a key.
    pub clipboard_set: Option<String>,
    /// Set by a widget that consumed a wheel event (a list or a scroll view).
    pub wheel_consumed: bool,
}

impl Default for EventCtx {
    fn default() -> Self {
        Self {
            hovered: None,
            pressed: None,
            focus: None,
            cursor: Cursor::Default,
            needs_redraw: false,
            shift: false,
            ctrl: false,
            pointer: Point::ZERO,
            ime_allowed: false,
            ime_cursor: None,
            clipboard_get: None,
            clipboard_set: None,
            wheel_consumed: false,
        }
    }
}

impl EventCtx {
    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

// ---------------------------------------------------------------------------
// Layout algorithms
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub padding: f32,
    pub spacing: f32,
    pub align: Align,
}

impl Default for Column {
    fn default() -> Self {
        Self {
            padding: 0.0,
            spacing: 0.0,
            align: Align::Start,
        }
    }
}

impl Column {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn padding(mut self, value: f32) -> Self {
        self.padding = value;
        self
    }

    pub fn spacing(mut self, value: f32) -> Self {
        self.spacing = value;
        self
    }

    pub fn align(mut self, value: Align) -> Self {
        self.align = value;
        self
    }

    pub fn measure(&self, constraints: Constraints, sizes: &[Size]) -> Size {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for size in sizes {
            width = width.max(size.width);
            height += size.height;
        }
        if sizes.len() > 1 {
            height += self.spacing * (sizes.len() - 1) as f32;
        }
        constraints.constrain(Size::new(
            width + self.padding * 2.0,
            height + self.padding * 2.0,
        ))
    }

    pub fn arrange(&self, rect: Rect, sizes: &[Size], out: &mut [Rect]) {
        let inner = rect.inset(self.padding);
        let mut y = inner.y;
        for (index, size) in sizes.iter().enumerate() {
            if index >= out.len() {
                break;
            }
            let width = size.width.min(inner.width);
            let (x, width) = match self.align {
                Align::Start => (inner.x, width),
                Align::Center => (inner.x + (inner.width - width) / 2.0, width),
                Align::End => (inner.x + inner.width - width, width),
                Align::Stretch => (inner.x, inner.width),
            };
            let height = size.height.min((inner.bottom() - y).max(0.0));
            out[index] = Rect::new(x, y, width, height);
            y += height + self.spacing;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub padding: f32,
    pub spacing: f32,
    pub align: Align,
}

impl Default for Row {
    fn default() -> Self {
        Self {
            padding: 0.0,
            spacing: 0.0,
            align: Align::Start,
        }
    }
}

impl Row {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn padding(mut self, value: f32) -> Self {
        self.padding = value;
        self
    }

    pub fn spacing(mut self, value: f32) -> Self {
        self.spacing = value;
        self
    }

    pub fn align(mut self, value: Align) -> Self {
        self.align = value;
        self
    }

    pub fn measure(&self, constraints: Constraints, sizes: &[Size]) -> Size {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        for size in sizes {
            width += size.width;
            height = height.max(size.height);
        }
        if sizes.len() > 1 {
            width += self.spacing * (sizes.len() - 1) as f32;
        }
        constraints.constrain(Size::new(
            width + self.padding * 2.0,
            height + self.padding * 2.0,
        ))
    }

    pub fn arrange(&self, rect: Rect, sizes: &[Size], out: &mut [Rect]) {
        let inner = rect.inset(self.padding);
        let mut x = inner.x;
        for (index, size) in sizes.iter().enumerate() {
            if index >= out.len() {
                break;
            }
            let height = size.height.min(inner.height);
            let (y, height) = match self.align {
                Align::Start => (inner.y, height),
                Align::Center => (inner.y + (inner.height - height) / 2.0, height),
                Align::End => (inner.y + inner.height - height, height),
                Align::Stretch => (inner.y, inner.height),
            };
            let width = size.width.min((inner.right() - x).max(0.0));
            out[index] = Rect::new(x, y, width, height);
            x += width + self.spacing;
        }
    }
}

// ---------------------------------------------------------------------------
// Widgets
// ---------------------------------------------------------------------------

/// Per-node layout state. A generated `Ui` stores one of these per node, in
/// the node order of its generated `Node` enum.
#[derive(Clone, Copy, Debug)]
pub struct NodeMeta {
    pub measured: Size,
    pub rect: Rect,
}

impl NodeMeta {
    pub const ZERO: Self = Self {
        measured: Size { width: 0.0, height: 0.0 },
        rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
    };
}

impl Default for NodeMeta {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Clone, Debug)]
pub struct Label {
    pub text: String,
    pub style: TextStyle,
}

impl Label {
    pub fn new(theme: &Theme) -> Self {
        Self {
            text: String::new(),
            style: TextStyle::new(theme.text_size, theme.text),
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        let size = cx.text.measure(&self.text, &self.style);
        constraints.constrain(size)
    }

    pub fn paint(&self, cx: &mut PaintCx<'_>, rect: Rect) {
        if self.text.is_empty() {
            return;
        }
        let line_height = cx.text.line_height(self.style.size);
        let origin = Point::new(rect.x, rect.y + (rect.height - line_height) / 2.0);
        cx.text(&self.text, &self.style, origin);
    }

    pub fn apply_theme(&mut self, theme: &Theme) {
        self.style.size = theme.text_size;
        self.style.color = theme.text;
    }
}

#[derive(Clone, Debug)]
pub struct Button<M> {
    pub text: String,
    pub on_click: Option<M>,
    pub enabled: bool,
    pub style: ButtonStyle,
    pub text_align: Align,
    pub hovered: bool,
    pub pressed: bool,
}

impl<M: Clone> Button<M> {
    pub fn new(theme: &Theme) -> Self {
        Self {
            text: String::new(),
            on_click: None,
            enabled: true,
            style: theme.button,
            text_align: Align::Center,
            hovered: false,
            pressed: false,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn on_click(mut self, msg: M) -> Self {
        self.on_click = Some(msg);
        self
    }

    pub fn text_style(&self) -> TextStyle {
        TextStyle::new(self.style.text_size, self.style.text)
    }

    pub fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        let text = cx.text.measure(&self.text, &self.text_style());
        let size = Size::new(
            text.width + self.style.padding_x * 2.0,
            text.height + self.style.padding_y * 2.0,
        );
        constraints.constrain(size)
    }

    pub fn paint(&self, cx: &mut PaintCx<'_>, rect: Rect) {
        let background = if !self.enabled {
            self.style.background.with_alpha(0.5)
        } else if self.pressed {
            self.style.pressed
        } else if self.hovered {
            self.style.hover
        } else {
            self.style.background
        };
        cx.fill_rounded_rect(rect, background, self.style.radius);

        let mut style = self.text_style();
        if !self.enabled {
            style.color = style.color.with_alpha(0.4);
        }
        let text_size = cx.text.measure(&self.text, &style);
        let x = match self.text_align {
            Align::Start => rect.x + self.style.padding_x,
            Align::End => rect.right() - self.style.padding_x - text_size.width,
            Align::Center | Align::Stretch => rect.x + (rect.width - text_size.width) / 2.0,
        };
        let y = rect.y + (rect.height - text_size.height) / 2.0;
        cx.text(&self.text, &style, Point::new(x, y));
    }

    pub fn hit(&self, point: Point, rect: Rect) -> bool {
        self.enabled && rect.contains(point)
    }

    pub fn event(&mut self, event: &Event, cx: &mut EventCtx, rect: Rect) -> Option<M> {
        match event {
            Event::PointerMoved(point) => {
                let hovered = rect.contains(*point);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_redraw();
                }
                if hovered {
                    cx.cursor = Cursor::PointingHand;
                }
                None
            }
            Event::PointerDown(point, PointerButton::Primary) => {
                if self.enabled && rect.contains(*point) {
                    self.pressed = true;
                    cx.request_redraw();
                }
                None
            }
            Event::PointerUp(point, PointerButton::Primary) => {
                let was_pressed = self.pressed;
                self.pressed = false;
                if was_pressed {
                    cx.request_redraw();
                }
                if was_pressed && self.enabled && rect.contains(*point) {
                    self.on_click.clone()
                } else {
                    None
                }
            }
            Event::PointerLeft => {
                if self.hovered || self.pressed {
                    self.hovered = false;
                    self.pressed = false;
                    cx.request_redraw();
                }
                None
            }
            Event::FocusLost => {
                self.pressed = false;
                None
            }
            _ => None,
        }
    }

    pub fn apply_theme(&mut self, theme: &Theme) {
        self.style = theme.button;
    }
}

/// A checkbox with a label. Clicking it toggles `checked` and emits
/// `on_change`.
#[derive(Clone, Debug)]
pub struct Checkbox<M> {
    pub text: String,
    pub checked: bool,
    pub enabled: bool,
    pub on_change: Option<M>,
    pub box_size: f32,
    pub radius: f32,
    pub border_color: Color,
    pub accent: Color,
    pub text_color: Color,
    pub text_size: f32,
    pub gap: f32,
    pub hovered: bool,
    pub pressed: bool,
}

impl<M: Clone> Checkbox<M> {
    pub fn new(theme: &Theme) -> Self {
        Self {
            text: String::new(),
            checked: false,
            enabled: true,
            on_change: None,
            box_size: 18.0,
            radius: 4.0,
            border_color: theme.text_muted,
            accent: theme.accent,
            text_color: theme.text,
            text_size: theme.button.text_size,
            gap: 8.0,
            hovered: false,
            pressed: false,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_change(mut self, msg: M) -> Self {
        self.on_change = Some(msg);
        self
    }

    pub fn text_style(&self) -> TextStyle {
        TextStyle::new(self.text_size, self.text_color)
    }

    pub fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        let text = cx.text.measure(&self.text, &self.text_style());
        let line_height = cx.text.line_height(self.text_size);
        let height = self.box_size.max(text.height).max(line_height);
        let width = self.box_size + self.gap + text.width;
        constraints.constrain(Size::new(width, height))
    }

    pub fn paint(&self, cx: &mut PaintCx<'_>, rect: Rect) {
        let top = rect.y + (rect.height - self.box_size) / 2.0;
        let box_rect = Rect::new(rect.x, top, self.box_size, self.box_size);
        let border = if self.hovered && self.enabled {
            self.accent
        } else {
            self.border_color
        };
        cx.fill_rounded_rect_bordered(
            box_rect,
            Color::WHITE,
            self.radius,
            1.5,
            border,
        );
        if self.checked {
            let inner = box_rect.inset(3.5);
            cx.fill_rounded_rect(inner, self.accent, (self.radius - 2.0).max(1.0));
        }

        let mut style = self.text_style();
        if !self.enabled {
            style.color = style.color.with_alpha(0.4);
        }
        let text_x = box_rect.right() + self.gap;
        let text_size = cx.text.measure(&self.text, &style);
        let text_y = rect.y + (rect.height - text_size.height) / 2.0;
        cx.text(&self.text, &style, Point::new(text_x, text_y));
    }

    pub fn hit(&self, point: Point, rect: Rect) -> bool {
        self.enabled && rect.contains(point)
    }

    pub fn event(&mut self, event: &Event, cx: &mut EventCtx, rect: Rect) -> Option<M> {
        match event {
            Event::PointerMoved(point) => {
                let hovered = rect.contains(*point);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_redraw();
                }
                if hovered {
                    cx.cursor = Cursor::PointingHand;
                }
                None
            }
            Event::PointerDown(point, PointerButton::Primary) => {
                if self.enabled && rect.contains(*point) {
                    self.pressed = true;
                    cx.request_redraw();
                }
                None
            }
            Event::PointerUp(point, PointerButton::Primary) => {
                let was_pressed = self.pressed;
                self.pressed = false;
                if was_pressed && self.enabled && rect.contains(*point) {
                    self.checked = !self.checked;
                    cx.request_redraw();
                    self.on_change.clone()
                } else {
                    if was_pressed {
                        cx.request_redraw();
                    }
                    None
                }
            }
            Event::PointerLeft => {
                if self.hovered || self.pressed {
                    self.hovered = false;
                    self.pressed = false;
                    cx.request_redraw();
                }
                None
            }
            _ => None,
        }
    }

    pub fn apply_theme(&mut self, theme: &Theme) {
        self.border_color = theme.text_muted;
        self.accent = theme.accent;
        self.text_color = theme.text;
        self.text_size = theme.button.text_size;
    }
}

impl<M: Clone> WidgetView for Checkbox<M> {
    fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        Checkbox::measure(self, cx, constraints)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        Checkbox::paint(self, cx, rect);
    }

    fn hit(&self, point: Point, rect: Rect) -> bool {
        Checkbox::hit(self, point, rect)
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn apply_theme(&mut self, theme: &Theme) {
        Checkbox::apply_theme(self, theme);
    }
}

impl<M: Clone> MessageSource<M> for Checkbox<M> {
    fn emit(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        _text: &mut dyn TextEngine,
    ) -> Option<M> {
        Checkbox::event(self, event, cx, rect)
    }
}

impl<M: Clone> Construct for Checkbox<M> {
    fn construct(theme: &Theme) -> Self {
        Self::new(theme)
    }
}

// ---------------------------------------------------------------------------
// The static UI and the application
// ---------------------------------------------------------------------------

/// The framework-facing surface of a generated static UI.
pub trait StaticUi {
    type Msg: Clone + 'static;

    fn apply_theme(&mut self, theme: &Theme);
    fn measure(&mut self, text: &mut dyn TextEngine, constraints: Constraints) -> Size;
    fn arrange(&mut self, rect: Rect);
    fn paint(&mut self, text: &mut dyn TextEngine, scene: &mut Scene);
    fn event(
        &mut self,
        cx: &mut EventCtx,
        ev: &Event,
        text: &mut dyn TextEngine,
    ) -> Option<Self::Msg>;
    fn focus_next(&mut self, cx: &mut EventCtx, forward: bool, text: &mut dyn TextEngine);
}

/// The application: business state plus the static view.
///
/// `update` is the only place where application state and the view are
/// touched together; there is no `view()` rebuild and no diffing.
pub trait App: Sized + 'static {
    type Msg: Clone + 'static;
    type Ui: StaticUi<Msg = Self::Msg>;

    fn update(&mut self, msg: Self::Msg, cx: &mut Ctx);
    fn view(&self) -> &Self::Ui;
    fn view_mut(&mut self) -> &mut Self::Ui;

    /// Periodic callback for apps that poll external sources (a PTY, a socket,
    /// an animation clock). Platform backends with a polling loop call this
    /// once per iteration; backends with a blocking event loop never do.
    fn tick(&mut self, _cx: &mut Ctx) {}
}

/// The handle an application gets in `update`: it can request a redraw or
/// close the window, but it has no access to the UI (the app owns it).
#[derive(Debug, Default)]
pub struct Ctx {
    pub needs_redraw: bool,
    pub close_requested: bool,
    pub cursor: Cursor,
}

impl Ctx {
    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub fn request_close(&mut self) {
        self.close_requested = true;
    }

    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.cursor = cursor;
    }
}

#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub theme: Theme,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "FerriticUI".to_string(),
            width: 480,
            height: 320,
            theme: Theme::light(),
        }
    }
}

/// A container with a background, a rounded outline and a border.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    pub padding: f32,
    pub spacing: f32,
    pub align: Align,
    pub background: Color,
    pub radius: f32,
    pub border_width: f32,
    pub border_color: Color,
    pub shadow_color: Color,
    pub shadow_blur: f32,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            padding: 0.0,
            spacing: 0.0,
            align: Align::Start,
            background: Color::TRANSPARENT,
            radius: 0.0,
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
            shadow_color: Color::TRANSPARENT,
            shadow_blur: 0.0,
            shadow_offset_x: 0.0,
            shadow_offset_y: 0.0,
        }
    }
}

impl Panel {
    fn layout(&self) -> Column {
        Column {
            padding: self.padding,
            spacing: self.spacing,
            align: self.align,
        }
    }
}

impl ContainerLayout for Panel {
    fn measure_children(&mut self, constraints: Constraints, sizes: &[Size]) -> Size {
        self.layout().measure(constraints, sizes)
    }

    fn arrange_children(&mut self, rect: Rect, sizes: &[Size], out: &mut [Rect]) {
        self.layout().arrange(rect, sizes, out);
    }

    fn paint_background(&self, cx: &mut PaintCx<'_>, rect: Rect) {
        if self.shadow_blur > 0.0 && self.shadow_color.a > 0.0 {
            cx.fill_shadow(
                rect,
                self.shadow_color,
                self.radius,
                self.shadow_blur,
                (self.shadow_offset_x, self.shadow_offset_y),
            );
        }
        if self.background.a > 0.0 {
            cx.fill_rounded_rect_bordered(
                rect,
                self.background,
                self.radius,
                self.border_width,
                self.border_color,
            );
        } else if self.border_width > 0.0 {
            cx.fill_rounded_rect_bordered(
                rect,
                Color::TRANSPARENT,
                self.radius,
                self.border_width,
                self.border_color,
            );
        }
    }
}

/// A container that centers its single child at its measured size.
#[derive(Clone, Copy, Debug, Default)]
pub struct Center {
    pub padding: f32,
}

impl ContainerLayout for Center {
    fn child_constraints(&self, constraints: Constraints) -> Constraints {
        Constraints {
            min: Size::ZERO,
            max: Size::new(
                (constraints.max.width - self.padding * 2.0).max(0.0),
                (constraints.max.height - self.padding * 2.0).max(0.0),
            ),
        }
    }

    fn measure_children(&mut self, constraints: Constraints, sizes: &[Size]) -> Size {
        let mut size = Size::ZERO;
        for child in sizes {
            size = size.max(*child);
        }
        constraints.constrain(Size::new(
            size.width + self.padding * 2.0,
            size.height + self.padding * 2.0,
        ))
    }

    fn arrange_children(&mut self, rect: Rect, sizes: &[Size], out: &mut [Rect]) {
        let inner = rect.inset(self.padding);
        for (index, size) in sizes.iter().enumerate() {
            if index >= out.len() {
                break;
            }
            out[index] = Rect::new(
                inner.x + (inner.width - size.width) / 2.0,
                inner.y + (inner.height - size.height) / 2.0,
                size.width,
                size.height,
            );
        }
    }
}

/// A vertical scrolling viewport. It fills the height it is given and clips
/// its children with a scissor.
#[derive(Clone, Copy, Debug)]
pub struct Scroll {
    pub padding: f32,
    pub spacing: f32,
    /// Vertical scroll offset in logical pixels.
    pub offset: f32,
    pub content_height: f32,
    pub viewport_height: f32,
    pub scrollbar_width: f32,
    pub track_color: Color,
    pub thumb_color: Color,
}

/// Logical pixels a wheel line scrolls.
const SCROLL_STEP: f32 = 28.0;

impl Default for Scroll {
    fn default() -> Self {
        Self {
            padding: 0.0,
            spacing: 0.0,
            offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            scrollbar_width: 6.0,
            track_color: Color::TRANSPARENT,
            thumb_color: Color::rgba(0.0, 0.0, 0.0, 0.25),
        }
    }
}

impl Scroll {
    pub fn max_offset(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    fn clamp_offset(&mut self) {
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }
}

impl ContainerLayout for Scroll {
    fn child_constraints(&self, constraints: Constraints) -> Constraints {
        Constraints {
            min: Size::ZERO,
            max: Size::new(
                (constraints.max.width - self.padding * 2.0).max(0.0),
                f32::INFINITY,
            ),
        }
    }

    fn measure_children(&mut self, constraints: Constraints, sizes: &[Size]) -> Size {
        let mut content_height = self.padding * 2.0;
        let mut content_width: f32 = 0.0;
        for size in sizes {
            content_height += size.height;
            content_width = content_width.max(size.width);
        }
        if sizes.len() > 1 {
            content_height += self.spacing * (sizes.len() - 1) as f32;
        }
        self.content_height = content_height;

        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            content_width + self.padding * 2.0
        };
        let height = if constraints.max.height.is_finite() {
            constraints.max.height
        } else {
            content_height
        };
        self.viewport_height = height;
        self.clamp_offset();
        constraints.constrain(Size::new(width, height))
    }

    fn arrange_children(&mut self, rect: Rect, sizes: &[Size], out: &mut [Rect]) {
        self.viewport_height = rect.height;
        self.clamp_offset();
        let inner_width = (rect.width - self.padding * 2.0).max(0.0);
        let mut y = rect.y + self.padding - self.offset;
        for (index, size) in sizes.iter().enumerate() {
            if index >= out.len() {
                break;
            }
            out[index] = Rect::new(rect.x + self.padding, y, inner_width, size.height);
            y += size.height + self.spacing;
        }
    }

    fn child_clip(&self, rect: Rect) -> Option<Rect> {
        Some(rect)
    }

    fn wheel(&mut self, delta: f32, cx: &mut EventCtx) -> bool {
        if self.max_offset() <= 0.0 {
            return false;
        }
        self.offset -= delta * SCROLL_STEP;
        self.clamp_offset();
        cx.request_redraw();
        true
    }

    fn paint_foreground(&self, cx: &mut PaintCx<'_>, rect: Rect) {
        let max_offset = self.max_offset();
        if max_offset <= 0.0 || self.content_height <= 0.0 {
            return;
        }
        let width = self.scrollbar_width;
        let track = Rect::new(
            rect.right() - width,
            rect.y + 2.0,
            width,
            (rect.height - 4.0).max(1.0),
        );
        let min_thumb = 24.0;
        let thumb_height =
            (rect.height * (self.viewport_height / self.content_height)).clamp(min_thumb, rect.height);
        let travel = (rect.height - thumb_height).max(0.0);
        let thumb_y = rect.y + (self.offset / max_offset) * travel;
        cx.fill_rounded_rect(track, self.track_color, width * 0.5);
        cx.fill_rounded_rect(
            Rect::new(track.x, thumb_y, width, thumb_height),
            self.thumb_color,
            width * 0.5,
        );
    }
}

/// A single-line text input.
///
/// The editing model is kept here: a byte cursor, a selection anchor, the
/// horizontal scroll offset and the IME pre-edit string. `on_change` is emitted
/// when the text changes; the application reads `text` and moves the cursor
/// explicitly.
#[derive(Clone, Debug)]
pub struct TextInput<M> {
    pub text: String,
    pub placeholder: String,
    pub on_change: Option<M>,
    pub enabled: bool,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll_x: f32,
    pub focused: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub preedit: String,
    pub background: Color,
    pub border: Color,
    pub focus_border: Color,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub selection_color: Color,
    pub font_size: f32,
    pub padding: f32,
    pub radius: f32,
}

impl<M: Clone> TextInput<M> {
    pub fn new(theme: &Theme) -> Self {
        Self {
            text: String::new(),
            placeholder: String::new(),
            on_change: None,
            enabled: true,
            cursor: 0,
            anchor: 0,
            scroll_x: 0.0,
            focused: false,
            hovered: false,
            pressed: false,
            preedit: String::new(),
            background: Color::WHITE,
            border: theme.text_muted.with_alpha(0.45),
            focus_border: theme.accent,
            text_color: theme.text,
            placeholder_color: theme.text_muted,
            selection_color: theme.accent.with_alpha(0.25),
            font_size: theme.button.text_size,
            padding: 8.0,
            radius: 6.0,
        }
    }

    pub fn text_style(&self) -> TextStyle {
        TextStyle::new(self.font_size, self.text_color)
    }

    fn selection(&self) -> Option<(usize, usize)> {
        if self.cursor == self.anchor {
            None
        } else if self.cursor < self.anchor {
            Some((self.cursor, self.anchor))
        } else {
            Some((self.anchor, self.cursor))
        }
    }

    fn clear_selection(&mut self) {
        self.anchor = self.cursor;
    }

    fn prev_boundary(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self) -> usize {
        self.text[self.cursor..]
            .chars()
            .next()
            .map(|character| self.cursor + character.len_utf8())
            .unwrap_or(self.text.len())
    }

    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            return false;
        };
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.anchor = start;
        true
    }

    fn insert(&mut self, value: &str) {
        self.delete_selection();
        self.text.insert_str(self.cursor, value);
        self.cursor += value.len();
        self.anchor = self.cursor;
    }

    fn replace_selection(&mut self, value: &str) {
        self.delete_selection();
        self.insert(value);
    }

    fn caret_x(&self, text: &mut dyn TextEngine) -> f32 {
        text.caret_x(&self.text, &self.text_style(), self.cursor)
    }

    fn inner_rect(&self, rect: Rect) -> Rect {
        rect.inset(self.padding)
    }

    fn cursor_from_point(&mut self, point: Point, rect: Rect, text: &mut dyn TextEngine) {
        let style = self.text_style();
        let local = point.x - rect.x - self.padding + self.scroll_x;
        let mut best = 0;
        let mut best_distance = local.abs();
        for (index, _) in self.text.char_indices() {
            let boundary = index + self.text[index..].chars().next().map_or(0, char::len_utf8);
            let width = text.caret_x(&self.text, &style, boundary);
            let distance = (width - local).abs();
            if distance < best_distance {
                best = boundary;
                best_distance = distance;
            }
        }
        self.cursor = best;
    }

    fn ensure_cursor_visible(&mut self, rect: Rect, text: &mut dyn TextEngine) {
        let inner = self.inner_rect(rect);
        let caret = self.caret_x(text);
        if caret - self.scroll_x > inner.width - 2.0 {
            self.scroll_x = caret - inner.width + 2.0;
        }
        if caret - self.scroll_x < 0.0 {
            self.scroll_x = caret;
        }
        self.scroll_x = self.scroll_x.max(0.0);
    }

    fn update_ime(&mut self, cx: &mut EventCtx, rect: Rect, text: &mut dyn TextEngine) {
        let inner = self.inner_rect(rect);
        let line_height = text.line_height(self.font_size);
        let caret = self.caret_x(text) - self.scroll_x;
        cx.ime_cursor = Some(Rect::new(
            inner.x + caret,
            rect.y + (rect.height - line_height) / 2.0,
            1.0,
            line_height,
        ));
    }

    fn changed(&self) -> Option<M> {
        self.on_change.clone()
    }

    fn handle_key(
        &mut self,
        key: Key,
        shift: bool,
        ctrl: bool,
        cx: &mut EventCtx,
        rect: Rect,
        text: &mut dyn TextEngine,
    ) -> Option<M> {
        let mut changed = false;

        if ctrl {
            match key {
                Key::Character('a') | Key::Character('A') => {
                    self.anchor = 0;
                    self.cursor = self.text.len();
                }
                Key::Character('c') | Key::Character('C') => {
                    if let Some((start, end)) = self.selection() {
                        cx.clipboard_set = Some(self.text[start..end].to_string());
                    }
                }
                Key::Character('x') | Key::Character('X') => {
                    if let Some((start, end)) = self.selection() {
                        cx.clipboard_set = Some(self.text[start..end].to_string());
                        changed = self.delete_selection();
                    }
                }
                Key::Character('v') | Key::Character('V') => {
                    if let Some(paste) = cx.clipboard_get.take() {
                        let paste = paste.replace(['\n', '\r'], " ");
                        self.replace_selection(&paste);
                        changed = true;
                    }
                }
                _ => {}
            }
            if changed {
                cx.request_redraw();
            }
            self.ensure_cursor_visible(rect, text);
            self.update_ime(cx, rect, text);
            return if changed { self.changed() } else { None };
        }

        match key {
            Key::Backspace => {
                if !self.delete_selection() {
                    if self.cursor > 0 {
                        let previous = self.prev_boundary();
                        self.text.replace_range(previous..self.cursor, "");
                        self.cursor = previous;
                        self.anchor = previous;
                        changed = true;
                    }
                } else {
                    changed = true;
                }
            }
            Key::Delete => {
                if !self.delete_selection() && self.cursor < self.text.len() {
                    let next = self.next_boundary();
                    self.text.replace_range(self.cursor..next, "");
                    changed = true;
                } else if self.selection().is_some() {
                    changed = true;
                }
            }
            Key::ArrowLeft => {
                if !shift {
                    if let Some((start, _)) = self.selection() {
                        self.cursor = start;
                    } else if self.cursor > 0 {
                        self.cursor = self.prev_boundary();
                    }
                    self.clear_selection();
                } else if self.cursor > 0 {
                    self.cursor = self.prev_boundary();
                }
            }
            Key::ArrowRight => {
                if !shift {
                    if let Some((_, end)) = self.selection() {
                        self.cursor = end;
                    } else if self.cursor < self.text.len() {
                        self.cursor = self.next_boundary();
                    }
                    self.clear_selection();
                } else if self.cursor < self.text.len() {
                    self.cursor = self.next_boundary();
                }
            }
            Key::Home => {
                self.cursor = 0;
                if !shift {
                    self.clear_selection();
                }
            }
            Key::End => {
                self.cursor = self.text.len();
                if !shift {
                    self.clear_selection();
                }
            }
            Key::Character(character) => {
                let mut buffer = [0u8; 4];
                let encoded = character.encode_utf8(&mut buffer);
                self.replace_selection(encoded);
                changed = true;
            }
            _ => {}
        }

        if changed {
            cx.request_redraw();
        }
        self.ensure_cursor_visible(rect, text);
        self.update_ime(cx, rect, text);
        if changed {
            self.changed()
        } else {
            None
        }
    }

    pub fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        let text = cx.text.measure(&self.text, &self.text_style());
        let line_height = cx.text.line_height(self.font_size);
        let intrinsic = Size::new(
            text.width.max(80.0) + self.padding * 2.0,
            line_height + self.padding * 2.0,
        );
        constraints.constrain(intrinsic)
    }

    pub fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        let border_width = if self.focused { 1.5 } else { 1.0 };
        let border_color = if self.focused {
            self.focus_border
        } else if self.hovered && self.enabled {
            self.text_color.with_alpha(0.5)
        } else {
            self.border
        };
        cx.fill_rounded_rect_bordered(
            rect,
            self.background,
            self.radius,
            border_width,
            border_color,
        );

        let inner = self.inner_rect(rect);
        let old_clip = cx.set_clip(Some(inner));
        let style = self.text_style();
        let line_height = cx.text.line_height(self.font_size);
        let text_y = rect.y + (rect.height - line_height) / 2.0;
        let text_x = inner.x - self.scroll_x;

        if self.text.is_empty() {
            if !self.focused && !self.placeholder.is_empty() {
                let placeholder_style = TextStyle::new(self.font_size, self.placeholder_color);
                cx.text(
                    &self.placeholder,
                    &placeholder_style,
                    Point::new(text_x, text_y),
                );
            }
        } else {
            if let Some((start, end)) = self.selection() {
                let x0 = text_x + cx.text.caret_x(&self.text, &style, start);
                let x1 = text_x + cx.text.caret_x(&self.text, &style, end);
                cx.fill_rect(
                    Rect::new(x0, text_y, (x1 - x0).max(1.0), line_height),
                    self.selection_color,
                );
            }
            cx.text(&self.text, &style, Point::new(text_x, text_y));
        }

        let caret_x = text_x + cx.text.caret_x(&self.text, &style, self.cursor);
        if !self.preedit.is_empty() {
            let preedit_width = cx.text.measure(&self.preedit, &style).width;
            cx.text(&self.preedit, &style, Point::new(caret_x, text_y));
            cx.fill_rect(
                Rect::new(caret_x, text_y + line_height - 1.0, preedit_width, 1.0),
                self.focus_border,
            );
        }
        if self.focused {
            cx.fill_rect(
                Rect::new(caret_x, text_y + 1.0, 1.5, line_height - 2.0),
                self.focus_border,
            );
        }

        cx.clip = old_clip;
    }

    pub fn hit(&self, point: Point, rect: Rect) -> bool {
        self.enabled && rect.contains(point)
    }

    pub fn event(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        text: &mut dyn TextEngine,
    ) -> Option<M> {
        match event {
            Event::PointerMoved(point) => {
                let hovered = rect.contains(*point);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_redraw();
                }
                if hovered {
                    cx.cursor = Cursor::Text;
                }
                if self.pressed {
                    self.cursor_from_point(*point, rect, text);
                    cx.request_redraw();
                    self.ensure_cursor_visible(rect, text);
                    self.update_ime(cx, rect, text);
                }
                None
            }
            Event::PointerDown(point, PointerButton::Primary) => {
                if self.enabled && rect.contains(*point) {
                    self.pressed = true;
                    let old_anchor = self.anchor;
                    self.cursor_from_point(*point, rect, text);
                    self.anchor = if cx.shift { old_anchor } else { self.cursor };
                    cx.request_redraw();
                    self.ensure_cursor_visible(rect, text);
                    self.update_ime(cx, rect, text);
                }
                None
            }
            Event::PointerUp(..) => {
                self.pressed = false;
                None
            }
            Event::PointerLeft => {
                if self.hovered || self.pressed {
                    self.hovered = false;
                    self.pressed = false;
                    cx.request_redraw();
                }
                None
            }
            Event::Key {
                key,
                pressed: true,
                shift,
                ctrl,
                ..
            } => self.handle_key(*key, *shift, *ctrl, cx, rect, text),
            Event::ImePreedit(value) => {
                if self.enabled {
                    self.preedit = value.clone();
                    self.update_ime(cx, rect, text);
                    cx.request_redraw();
                }
                None
            }
            Event::ImeCommit(value) => {
                if self.enabled {
                    self.preedit.clear();
                    self.replace_selection(value);
                    cx.request_redraw();
                    self.ensure_cursor_visible(rect, text);
                    self.update_ime(cx, rect, text);
                    return self.changed();
                }
                None
            }
            Event::ImeDisabled => {
                self.preedit.clear();
                cx.request_redraw();
                None
            }
            Event::FocusGained => {
                self.focused = true;
                self.anchor = self.cursor;
                cx.ime_allowed = true;
                cx.request_redraw();
                self.update_ime(cx, rect, text);
                None
            }
            Event::FocusLost => {
                self.focused = false;
                self.preedit.clear();
                cx.ime_allowed = false;
                cx.ime_cursor = None;
                cx.request_redraw();
                None
            }
            _ => None,
        }
    }

    pub fn apply_theme(&mut self, theme: &Theme) {
        self.border = theme.text_muted.with_alpha(0.45);
        self.focus_border = theme.accent;
        self.text_color = theme.text;
        self.placeholder_color = theme.text_muted;
        self.selection_color = theme.accent.with_alpha(0.25);
        self.font_size = theme.button.text_size;
    }
}

impl<M: Clone> WidgetView for TextInput<M> {
    fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        TextInput::measure(self, cx, constraints)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        TextInput::paint(self, cx, rect);
    }

    fn hit(&self, point: Point, rect: Rect) -> bool {
        TextInput::hit(self, point, rect)
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn apply_theme(&mut self, theme: &Theme) {
        TextInput::apply_theme(self, theme);
    }
}

impl<M: Clone> MessageSource<M> for TextInput<M> {
    fn emit(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        text: &mut dyn TextEngine,
    ) -> Option<M> {
        TextInput::event(self, event, cx, rect, text)
    }
}

impl<M: Clone> Construct for TextInput<M> {
    fn construct(theme: &Theme) -> Self {
        Self::new(theme)
    }
}

/// A vertically scrolling, virtualized list of text rows.
///
/// Only the visible rows are measured and painted; the data lives in `items`
/// and the selected row is `selected`. It is a leaf widget: the application
/// updates it with [`List::set_items`] and reads `selected`.
#[derive(Clone, Debug)]
pub struct List<M> {
    pub items: Vec<String>,
    pub selected: Option<usize>,
    pub on_select: Option<M>,
    pub enabled: bool,
    pub row_height: f32,
    pub offset: f32,
    pub content_height: f32,
    pub viewport_height: f32,
    pub hovered: Option<usize>,
    pub pressed: bool,
    pub background: Color,
    pub text_color: Color,
    pub selected_background: Color,
    pub hover_background: Color,
    pub font_size: f32,
    pub padding: f32,
    pub scrollbar_width: f32,
    pub track_color: Color,
    pub thumb_color: Color,
}

impl<M: Clone> List<M> {
    pub fn new(theme: &Theme) -> Self {
        Self {
            items: Vec::new(),
            selected: None,
            on_select: None,
            enabled: true,
            row_height: 32.0,
            offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            hovered: None,
            pressed: false,
            background: Color::WHITE,
            text_color: theme.text,
            selected_background: theme.accent.with_alpha(0.16),
            hover_background: theme.text_muted.with_alpha(0.10),
            font_size: theme.button.text_size,
            padding: 12.0,
            scrollbar_width: 6.0,
            track_color: Color::rgba(0.0, 0.0, 0.0, 0.05),
            thumb_color: theme.text_muted.with_alpha(0.5),
        }
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = items;
        self.content_height = self.items.len() as f32 * self.row_height;
        if let Some(selected) = self.selected {
            if selected >= self.items.len() {
                self.selected = None;
            }
        }
        self.clamp_offset();
    }

    fn max_offset(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    fn clamp_offset(&mut self) {
        self.offset = self.offset.clamp(0.0, self.max_offset());
    }

    fn visible_range(&self) -> (usize, usize) {
        if self.items.is_empty() || self.row_height <= 0.0 {
            return (0, 0);
        }
        let first = (self.offset / self.row_height).floor().max(0.0) as usize;
        let visible = (self.viewport_height / self.row_height).ceil() as usize + 1;
        let last = (first + visible).min(self.items.len());
        (first, last)
    }

    fn row_rect(&self, index: usize, rect: Rect) -> Rect {
        Rect::new(
            rect.x,
            rect.y + index as f32 * self.row_height - self.offset,
            rect.width,
            self.row_height,
        )
    }

    fn row_at(&self, point: Point, rect: Rect) -> Option<usize> {
        if !rect.contains(point) || self.items.is_empty() {
            return None;
        }
        let local_y = point.y - rect.y + self.offset;
        if local_y < 0.0 {
            return None;
        }
        let index = (local_y / self.row_height).floor() as usize;
        if index < self.items.len() {
            Some(index)
        } else {
            None
        }
    }

    fn text_style(&self) -> TextStyle {
        TextStyle::new(self.font_size, self.text_color)
    }

    fn scroll_to_selected(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        let top = selected as f32 * self.row_height;
        let bottom = top + self.row_height;
        if top < self.offset {
            self.offset = top;
        }
        if bottom > self.offset + self.viewport_height {
            self.offset = bottom - self.viewport_height;
        }
        self.clamp_offset();
    }

    pub fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        self.content_height = self.items.len() as f32 * self.row_height;
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            200.0
        };
        let height = if constraints.max.height.is_finite() {
            constraints.max.height
        } else {
            self.content_height
        };
        self.viewport_height = height;
        self.clamp_offset();
        let _ = cx;
        constraints.constrain(Size::new(width, height))
    }

    pub fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        cx.fill_rounded_rect(rect, self.background, 8.0);
        let viewport = Rect::new(
            rect.x,
            rect.y,
            (rect.width - self.scrollbar_width).max(0.0),
            rect.height,
        );
        let old_clip = cx.set_clip(Some(viewport));
        let style = self.text_style();
        let line_height = cx.text.line_height(self.font_size);
        let (first, last) = self.visible_range();
        for index in first..last {
            let Some(item) = self.items.get(index) else {
                continue;
            };
            let row = self.row_rect(index, rect);
            if self.selected == Some(index) {
                cx.fill_rect(row, self.selected_background);
            } else if self.hovered == Some(index) {
                cx.fill_rect(row, self.hover_background);
            }
            let text_y = row.y + (row.height - line_height) / 2.0;
            cx.text(
                item,
                &style,
                Point::new(row.x + self.padding, text_y),
            );
        }
        cx.clip = old_clip;

        // Scrollbar.
        let max_offset = self.max_offset();
        if max_offset > 0.0 {
            let width = self.scrollbar_width;
            let track = Rect::new(
                rect.right() - width,
                rect.y + 2.0,
                width,
                (rect.height - 4.0).max(1.0),
            );
            let thumb_height = (rect.height
                * (self.viewport_height / self.content_height))
                .clamp(24.0, rect.height);
            let travel = (rect.height - thumb_height).max(0.0);
            let thumb_y = rect.y + (self.offset / max_offset) * travel;
            cx.fill_rounded_rect(track, self.track_color, width * 0.5);
            cx.fill_rounded_rect(
                Rect::new(track.x, thumb_y, width, thumb_height),
                self.thumb_color,
                width * 0.5,
            );
        }
    }

    pub fn hit(&self, point: Point, rect: Rect) -> bool {
        self.enabled && rect.contains(point)
    }

    pub fn event(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        _text: &mut dyn TextEngine,
    ) -> Option<M> {
        match event {
            Event::PointerMoved(point) => {
                let hovered = self.row_at(*point, rect);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_redraw();
                }
                if hovered.is_some() {
                    cx.cursor = Cursor::PointingHand;
                }
                None
            }
            Event::PointerDown(point, PointerButton::Primary) => {
                if rect.contains(*point) {
                    self.pressed = true;
                }
                None
            }
            Event::PointerUp(point, PointerButton::Primary) => {
                let was_pressed = self.pressed;
                self.pressed = false;
                if was_pressed {
                    if let Some(index) = self.row_at(*point, rect) {
                        if self.selected != Some(index) {
                            self.selected = Some(index);
                            cx.request_redraw();
                            return self.on_select.clone();
                        }
                    }
                }
                None
            }
            Event::PointerLeft => {
                if self.hovered.take().is_some() {
                    cx.request_redraw();
                }
                self.pressed = false;
                None
            }
            Event::Wheel(delta) => {
                if self.max_offset() <= 0.0 {
                    return None;
                }
                self.offset -= delta * 28.0;
                self.clamp_offset();
                cx.wheel_consumed = true;
                cx.request_redraw();
                None
            }
            Event::Key {
                key,
                pressed: true,
                ..
            } => {
                let mut moved = false;
                match key {
                    Key::ArrowDown => {
                        let next = self.selected.map_or(0, |index| (index + 1).min(self.items.len().saturating_sub(1)));
                        if !self.items.is_empty() {
                            self.selected = Some(next);
                            moved = true;
                        }
                    }
                    Key::ArrowUp => {
                        if let Some(index) = self.selected {
                            self.selected = Some(index.saturating_sub(1));
                            moved = true;
                        }
                    }
                    Key::Home => {
                        if !self.items.is_empty() {
                            self.selected = Some(0);
                            moved = true;
                        }
                    }
                    Key::End => {
                        if !self.items.is_empty() {
                            self.selected = Some(self.items.len() - 1);
                            moved = true;
                        }
                    }
                    Key::PageDown => {
                        self.offset += self.viewport_height;
                        self.clamp_offset();
                        cx.request_redraw();
                    }
                    Key::PageUp => {
                        self.offset -= self.viewport_height;
                        self.clamp_offset();
                        cx.request_redraw();
                    }
                    _ => {}
                }
                if moved {
                    self.scroll_to_selected();
                    cx.request_redraw();
                    return self.on_select.clone();
                }
                None
            }
            Event::FocusLost => {
                self.pressed = false;
                None
            }
            _ => None,
        }
    }

    pub fn apply_theme(&mut self, theme: &Theme) {
        self.text_color = theme.text;
        self.selected_background = theme.accent.with_alpha(0.16);
        self.hover_background = theme.text_muted.with_alpha(0.10);
        self.font_size = theme.button.text_size;
        self.thumb_color = theme.text_muted.with_alpha(0.5);
    }
}

impl<M: Clone> WidgetView for List<M> {
    fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        List::measure(self, cx, constraints)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        List::paint(self, cx, rect);
    }

    fn hit(&self, point: Point, rect: Rect) -> bool {
        List::hit(self, point, rect)
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn apply_theme(&mut self, theme: &Theme) {
        List::apply_theme(self, theme);
    }
}

impl<M: Clone> MessageSource<M> for List<M> {
    fn emit(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        text: &mut dyn TextEngine,
    ) -> Option<M> {
        List::event(self, event, cx, rect, text)
    }
}

impl<M: Clone> Construct for List<M> {
    fn construct(theme: &Theme) -> Self {
        Self::new(theme)
    }
}

/// A horizontal slider for a `value` in `0..=1`. Dragging or the arrow keys
/// change the value and emit `on_change`; the application reads `value`.
#[derive(Clone, Debug)]
pub struct Slider<M> {
    pub value: f32,
    pub on_change: Option<M>,
    pub enabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub track_color: Color,
    pub fill_color: Color,
    pub accent: Color,
    pub track_height: f32,
    pub thumb_radius: f32,
}

impl<M: Clone> Slider<M> {
    pub fn new(theme: &Theme) -> Self {
        Self {
            value: 0.0,
            on_change: None,
            enabled: true,
            hovered: false,
            pressed: false,
            track_color: theme.text_muted.with_alpha(0.2),
            fill_color: theme.accent,
            accent: theme.accent,
            track_height: 5.0,
            thumb_radius: 9.0,
        }
    }

    fn track_rect(&self, rect: Rect) -> Rect {
        let track_width = (rect.width - self.thumb_radius * 2.0).max(0.0);
        Rect::new(
            rect.x + self.thumb_radius,
            rect.y + rect.height * 0.5 - self.track_height * 0.5,
            track_width,
            self.track_height,
        )
    }

    fn set_value(&mut self, value: f32) -> bool {
        let value = value.clamp(0.0, 1.0);
        if (value - self.value).abs() < f32::EPSILON {
            return false;
        }
        self.value = value;
        true
    }

    fn value_from_point(&self, point: Point, rect: Rect) -> f32 {
        let track = self.track_rect(rect);
        if track.width <= 0.0 {
            return self.value;
        }
        ((point.x - track.x) / track.width).clamp(0.0, 1.0)
    }

    pub fn measure(&mut self, _cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            180.0
        };
        constraints.constrain(Size::new(width, self.thumb_radius * 2.0 + 6.0))
    }

    pub fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        let track = self.track_rect(rect);
        let center_y = rect.y + rect.height * 0.5;
        cx.fill_rounded_rect(track, self.track_color, self.track_height * 0.5);
        let progress = self.value.clamp(0.0, 1.0);
        let filled = Rect::new(track.x, track.y, track.width * progress, track.height);
        cx.fill_rounded_rect(filled, self.fill_color, self.track_height * 0.5);

        let thumb_x = track.x + track.width * progress;
        let radius = self.thumb_radius;
        let border = if self.hovered || self.pressed { 2.5 } else { 2.0 };
        cx.fill_rounded_rect_bordered(
            Rect::new(thumb_x - radius, center_y - radius, radius * 2.0, radius * 2.0),
            Color::WHITE,
            radius,
            border,
            self.accent,
        );
    }

    pub fn hit(&self, point: Point, rect: Rect) -> bool {
        self.enabled && rect.contains(point)
    }

    pub fn event(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        _text: &mut dyn TextEngine,
    ) -> Option<M> {
        match event {
            Event::PointerMoved(point) => {
                let hovered = rect.contains(*point);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_redraw();
                }
                if hovered {
                    cx.cursor = Cursor::PointingHand;
                }
                if self.pressed {
                    let value = self.value_from_point(*point, rect);
                    if self.set_value(value) {
                        cx.request_redraw();
                        return self.on_change.clone();
                    }
                }
                None
            }
            Event::PointerDown(point, PointerButton::Primary) => {
                if self.enabled && rect.contains(*point) {
                    self.pressed = true;
                    let value = self.value_from_point(*point, rect);
                    self.set_value(value);
                    cx.request_redraw();
                }
                None
            }
            Event::PointerUp(..) => {
                self.pressed = false;
                None
            }
            Event::PointerLeft => {
                if self.hovered || self.pressed {
                    self.hovered = false;
                    self.pressed = false;
                    cx.request_redraw();
                }
                None
            }
            Event::Key {
                key,
                pressed: true,
                shift,
                ..
            } => {
                let step = if *shift { 0.1 } else { 0.02 };
                let changed = match key {
                    Key::ArrowLeft => self.set_value(self.value - step),
                    Key::ArrowRight => self.set_value(self.value + step),
                    Key::Home => self.set_value(0.0),
                    Key::End => self.set_value(1.0),
                    _ => false,
                };
                if changed {
                    cx.request_redraw();
                    return self.on_change.clone();
                }
                None
            }
            Event::FocusLost => {
                self.pressed = false;
                None
            }
            _ => None,
        }
    }

    pub fn apply_theme(&mut self, theme: &Theme) {
        self.track_color = theme.text_muted.with_alpha(0.2);
        self.fill_color = theme.accent;
        self.accent = theme.accent;
    }
}

impl<M: Clone> WidgetView for Slider<M> {
    fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        Slider::measure(self, cx, constraints)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        Slider::paint(self, cx, rect);
    }

    fn hit(&self, point: Point, rect: Rect) -> bool {
        Slider::hit(self, point, rect)
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn apply_theme(&mut self, theme: &Theme) {
        Slider::apply_theme(self, theme);
    }
}

impl<M: Clone> MessageSource<M> for Slider<M> {
    fn emit(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        text: &mut dyn TextEngine,
    ) -> Option<M> {
        Slider::event(self, event, cx, rect, text)
    }
}

impl<M: Clone> Construct for Slider<M> {
    fn construct(theme: &Theme) -> Self {
        Self::new(theme)
    }
}

// ---------------------------------------------------------------------------
// Widget traits used by generated static UIs
// ---------------------------------------------------------------------------

/// Constructs a widget with the theme's defaults. A generated `Ui::new` calls
/// this for every node before applying the DSL's properties.
pub trait Construct {
    fn construct(theme: &Theme) -> Self;
}

impl Construct for Label {
    fn construct(theme: &Theme) -> Self {
        Self::new(theme)
    }
}

impl<M: Clone> Construct for Button<M> {
    fn construct(theme: &Theme) -> Self {
        Self::new(theme)
    }
}

impl Construct for Column {
    /// Containers start from their zero defaults; the DSL sets the theme's
    /// spacing/padding explicitly, which keeps the generated tree and a
    /// hand-written one comparable.
    fn construct(_theme: &Theme) -> Self {
        Self::default()
    }
}

impl Construct for Row {
    fn construct(_theme: &Theme) -> Self {
        Self::default()
    }
}

impl Construct for Panel {
    fn construct(_theme: &Theme) -> Self {
        Self::default()
    }
}

impl Construct for Center {
    fn construct(_theme: &Theme) -> Self {
        Self::default()
    }
}

impl Construct for Scroll {
    fn construct(theme: &Theme) -> Self {
        Self {
            track_color: Color::rgba(0.0, 0.0, 0.0, 0.05),
            thumb_color: theme.text_muted.with_alpha(0.5),
            ..Self::default()
        }
    }
}

/// The per-node interface a generated `Ui` uses for visual leaf widgets.
pub trait WidgetView {
    fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size;
    fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect);
    fn hit(&self, point: Point, rect: Rect) -> bool;
    fn focusable(&self) -> bool;
    fn apply_theme(&mut self, theme: &Theme);
}

impl WidgetView for Label {
    fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        Label::measure(self, cx, constraints)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        Label::paint(self, cx, rect);
    }

    fn hit(&self, _point: Point, _rect: Rect) -> bool {
        false
    }

    fn focusable(&self) -> bool {
        false
    }

    fn apply_theme(&mut self, theme: &Theme) {
        Label::apply_theme(self, theme);
    }
}

impl<M: Clone> WidgetView for Button<M> {
    fn measure(&mut self, cx: &mut MeasureCx<'_>, constraints: Constraints) -> Size {
        Button::measure(self, cx, constraints)
    }

    fn paint(&mut self, cx: &mut PaintCx<'_>, rect: Rect) {
        Button::paint(self, cx, rect);
    }

    fn hit(&self, point: Point, rect: Rect) -> bool {
        Button::hit(self, point, rect)
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn apply_theme(&mut self, theme: &Theme) {
        Button::apply_theme(self, theme);
    }
}

/// The per-node interface a generated `Ui` uses to emit messages from leaf
/// widgets. Non-interactive widgets implement it by returning `None`.
pub trait MessageSource<M> {
    fn emit(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        text: &mut dyn TextEngine,
    ) -> Option<M>;
}

impl<M: Clone> MessageSource<M> for Button<M> {
    fn emit(
        &mut self,
        event: &Event,
        cx: &mut EventCtx,
        rect: Rect,
        _text: &mut dyn TextEngine,
    ) -> Option<M> {
        Button::event(self, event, cx, rect)
    }
}

impl<M> MessageSource<M> for Label {
    fn emit(
        &mut self,
        _event: &Event,
        _cx: &mut EventCtx,
        _rect: Rect,
        _text: &mut dyn TextEngine,
    ) -> Option<M> {
        None
    }
}

/// The interface a generated `Ui` uses for container nodes. Containers do not
/// own their children; they only provide the layout algorithm and properties.
pub trait ContainerLayout {
    /// The constraints the container gives its children while measuring.
    fn child_constraints(&self, constraints: Constraints) -> Constraints {
        Constraints::loose(constraints.max)
    }

    fn measure_children(&mut self, constraints: Constraints, sizes: &[Size]) -> Size;
    fn arrange_children(&mut self, rect: Rect, sizes: &[Size], out: &mut [Rect]);

    /// The clip the container applies to its children while painting.
    fn child_clip(&self, _rect: Rect) -> Option<Rect> {
        None
    }

    /// Paints the container's own background before its children. Containers
    /// without a visual (Column, Row) keep the default no-op.
    fn paint_background(&self, _cx: &mut PaintCx<'_>, _rect: Rect) {}

    /// Paints on top of the children (scrollbars and similar overlays).
    fn paint_foreground(&self, _cx: &mut PaintCx<'_>, _rect: Rect) {}

    /// Handles a wheel event over the container. Returns `true` when it was
    /// consumed.
    fn wheel(&mut self, _delta: f32, _cx: &mut EventCtx) -> bool {
        false
    }
}

impl ContainerLayout for Column {
    fn measure_children(&mut self, constraints: Constraints, sizes: &[Size]) -> Size {
        self.measure(constraints, sizes)
    }

    fn arrange_children(&mut self, rect: Rect, sizes: &[Size], out: &mut [Rect]) {
        self.arrange(rect, sizes, out);
    }
}

impl ContainerLayout for Row {
    fn measure_children(&mut self, constraints: Constraints, sizes: &[Size]) -> Size {
        self.measure(constraints, sizes)
    }

    fn arrange_children(&mut self, rect: Rect, sizes: &[Size], out: &mut [Rect]) {
        self.arrange(rect, sizes, out);
    }
}

// ---------------------------------------------------------------------------
// Prelude
// ---------------------------------------------------------------------------

pub mod prelude {
    pub use crate::{
        Align, App, Button, ButtonStyle, Center, Checkbox, Color, Column, Constraints, Construct,
        ContainerLayout, Ctx, Cursor, Event, EventCtx, Key, Label, MeasureCx, MessageSource,
        NodeMeta, PaintCx, Panel, Point, PointerButton, Prim, Rect, Row, Scene, Scroll, Size,
        Slider, StaticUi, TextEngine, TextInput, TextStyle, Theme, WidgetView, WindowOptions, List,
    };
}
