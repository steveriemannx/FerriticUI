//! The counter example written by hand in the shape the `ui!` macro
//! generates. Kept as a reference for the generated code; compare its snapshot
//! with the macro version.

use ferritic::prelude::*;
use ferritic::{WindowOptions};

#[derive(Clone, Debug)]
enum Msg {
    Increment,
    Reset,
}

struct Counter {
    count: i32,
    ui: CounterUi,
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

// The flat static tree: nodes are fields, the graph is generated code.
const NODE_COUNT: usize = 5;

#[derive(Clone, Copy, Debug)]
#[repr(usize)]
enum Node {
    Root = 0,
    Value = 1,
    Row = 2,
    Inc = 3,
    Reset = 4,
}

const ROOT_CHILDREN: &[Node] = &[Node::Value, Node::Row];
const ROW_CHILDREN: &[Node] = &[Node::Inc, Node::Reset];
const FOCUS_ORDER: &[Node] = &[Node::Inc, Node::Reset];

fn from_u32(value: u32) -> Option<Node> {
    match value {
        0 => Some(Node::Root),
        1 => Some(Node::Value),
        2 => Some(Node::Row),
        3 => Some(Node::Inc),
        4 => Some(Node::Reset),
        _ => None,
    }
}

fn children(node: Node) -> &'static [Node] {
    match node {
        Node::Root => ROOT_CHILDREN,
        Node::Row => ROW_CHILDREN,
        _ => &[],
    }
}

struct CounterUi {
    value: Label,
    inc: Button<Msg>,
    reset: Button<Msg>,
    root: Column,
    row: Row,
    meta: [NodeMeta; NODE_COUNT],
}

impl CounterUi {
    fn new(theme: &Theme) -> Self {
        Self {
            value: Label::new(theme).text("0"),
            inc: Button::new(theme).text("+").on_click(Msg::Increment),
            reset: Button::new(theme).text("Reset").on_click(Msg::Reset),
            root: Column::new()
                .padding(theme.padding)
                .spacing(theme.spacing)
                .align(Align::Center),
            row: Row::new().spacing(theme.spacing),
            meta: [NodeMeta::ZERO; NODE_COUNT],
        }
    }

    fn measure_node(
        &mut self,
        text: &mut dyn TextEngine,
        node: Node,
        constraints: Constraints,
    ) -> Size {
        let size = match node {
            Node::Value => self.value.measure(&mut MeasureCx { text }, constraints),
            Node::Inc => self.inc.measure(&mut MeasureCx { text }, constraints),
            Node::Reset => self.reset.measure(&mut MeasureCx { text }, constraints),
            Node::Root => {
                let mut sizes = [Size::ZERO; ROOT_CHILDREN.len()];
                for (index, child) in ROOT_CHILDREN.iter().enumerate() {
                    sizes[index] =
                        self.measure_node(text, *child, Constraints::loose(constraints.max));
                }
                self.root.measure(constraints, &sizes)
            }
            Node::Row => {
                let mut sizes = [Size::ZERO; ROW_CHILDREN.len()];
                for (index, child) in ROW_CHILDREN.iter().enumerate() {
                    sizes[index] =
                        self.measure_node(text, *child, Constraints::loose(constraints.max));
                }
                self.row.measure(constraints, &sizes)
            }
        };
        self.meta[node as usize].measured = size;
        size
    }

    fn arrange_node(&mut self, node: Node, rect: Rect) {
        self.meta[node as usize].rect = rect;
        match node {
            Node::Root => {
                let sizes = child_sizes(&self.meta, ROOT_CHILDREN);
                let mut rects = [Rect::ZERO; ROOT_CHILDREN.len()];
                self.root.arrange(rect, &sizes, &mut rects);
                for (index, child) in ROOT_CHILDREN.iter().enumerate() {
                    self.arrange_node(*child, rects[index]);
                }
            }
            Node::Row => {
                let sizes = child_sizes(&self.meta, ROW_CHILDREN);
                let mut rects = [Rect::ZERO; ROW_CHILDREN.len()];
                self.row.arrange(rect, &sizes, &mut rects);
                for (index, child) in ROW_CHILDREN.iter().enumerate() {
                    self.arrange_node(*child, rects[index]);
                }
            }
            _ => {}
        }
    }

    fn paint_node(&mut self, text: &mut dyn TextEngine, node: Node, scene: &mut Scene) {
        let rect = self.meta[node as usize].rect;
        match node {
            Node::Value => self.value.paint(&mut PaintCx { scene, text, clip: None }, rect),
            Node::Inc => self.inc.paint(&mut PaintCx { scene, text, clip: None }, rect),
            Node::Reset => self.reset.paint(&mut PaintCx { scene, text, clip: None }, rect),
            Node::Root | Node::Row => {
                for child in children(node) {
                    self.paint_node(text, *child, scene);
                }
            }
        }
    }

    fn hit_test(&self, node: Node, point: Point) -> Option<Node> {
        if !self.meta[node as usize].rect.contains(point) {
            return None;
        }
        for child in children(node).iter().rev() {
            if let Some(hit) = self.hit_test(*child, point) {
                return Some(hit);
            }
        }
        match node {
            Node::Inc | Node::Reset => Some(node),
            _ => None,
        }
    }

    fn dispatch_leaf(&mut self, node: Node, event: &Event, cx: &mut EventCtx) -> Option<Msg> {
        let rect = self.meta[node as usize].rect;
        match node {
            Node::Inc => self.inc.event(event, cx, rect),
            Node::Reset => self.reset.event(event, cx, rect),
            _ => None,
        }
    }
}

fn child_sizes(meta: &[NodeMeta; NODE_COUNT], children: &[Node]) -> Vec<Size> {
    children
        .iter()
        .map(|child| meta[*child as usize].measured)
        .collect()
}

impl StaticUi for CounterUi {
    type Msg = Msg;

    fn apply_theme(&mut self, theme: &Theme) {
        self.value.apply_theme(theme);
        self.inc.apply_theme(theme);
        self.reset.apply_theme(theme);
        self.root = Column::new()
            .padding(theme.padding)
            .spacing(theme.spacing)
            .align(Align::Center);
        self.row = Row::new().spacing(theme.spacing);
    }

    fn measure(&mut self, text: &mut dyn TextEngine, constraints: Constraints) -> Size {
        self.measure_node(text, Node::Root, constraints)
    }

    fn arrange(&mut self, rect: Rect) {
        self.arrange_node(Node::Root, rect);
    }

    fn paint(&mut self, text: &mut dyn TextEngine, scene: &mut Scene) {
        self.paint_node(text, Node::Root, scene);
    }

    fn event(
        &mut self,
        cx: &mut EventCtx,
        event: &Event,
        text: &mut dyn TextEngine,
    ) -> Option<Msg> {
        match event {
            Event::PointerMoved(position) => {
                let hit = self.hit_test(Node::Root, *position);
                if hit.map(|node| node as u32) != cx.hovered {
                    if let Some(previous) = cx.hovered.and_then(from_u32) {
                        self.dispatch_leaf(previous, &Event::PointerLeft, cx);
                    }
                    cx.hovered = hit.map(|node| node as u32);
                }
                if let Some(node) = hit {
                    self.dispatch_leaf(node, event, cx)
                } else {
                    None
                }
            }
            Event::PointerLeft => {
                if let Some(previous) = cx.hovered.take().and_then(from_u32) {
                    self.dispatch_leaf(previous, event, cx);
                }
                None
            }
            Event::PointerDown(position, _) => {
                let hit = self.hit_test(Node::Root, *position);
                cx.pressed = hit.map(|node| node as u32);
                if let Some(node) = hit {
                    self.dispatch_leaf(node, event, cx)
                } else {
                    None
                }
            }
            Event::PointerUp(position, _) => {
                let target = cx
                    .pressed
                    .take()
                    .and_then(from_u32)
                    .or_else(|| self.hit_test(Node::Root, *position));
                if let Some(node) = target {
                    self.dispatch_leaf(node, event, cx)
                } else {
                    None
                }
            }
            Event::Key {
                key: Key::Tab,
                pressed: true,
                shift,
                ..
            } => {
                self.focus_next(cx, !shift, text);
                cx.request_redraw();
                None
            }
            Event::Key { .. } => {
                if let Some(node) = cx.focus.and_then(from_u32) {
                    self.dispatch_leaf(node, event, cx)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn focus_next(&mut self, cx: &mut EventCtx, forward: bool, _text: &mut dyn TextEngine) {
        let Some(last) = cx.focus.and_then(from_u32) else {
            cx.focus = FOCUS_ORDER.first().map(|node| *node as u32);
            return;
        };
        let Some(index) = FOCUS_ORDER.iter().position(|node| *node as usize == last as usize) else {
            cx.focus = FOCUS_ORDER.first().map(|node| *node as u32);
            return;
        };
        let count = FOCUS_ORDER.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        cx.focus = Some(FOCUS_ORDER[next] as u32);
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

/// Renders one frame offscreen (no window) and writes it as a PNG. This is the
/// first snapshot harness: it verifies layout, text and the glyph atlas without
/// a display.
fn snapshot() -> Result<(), String> {
    let theme = Theme::light();
    let mut app = Counter::new(&theme);
    let mut text = ferritic::SystemText::new()?;
    let scale = 2.0;
    text.set_scale(scale);
    let size = Size::new(360.0, 220.0);
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
    let atlas = text.atlas_update();

    let mut gpu = ferritic::render::Gpu::new_offscreen()?;
    let snapshot =
        ferritic::render::render_offscreen(&mut gpu, size, scale, &scene, atlas.as_ref())?;
    let path = std::env::temp_dir().join("ferritic-counter-manual.png");
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
