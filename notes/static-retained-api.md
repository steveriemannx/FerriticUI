# API shape for a pure static retained-mode Rust GUI

**Scope.** A retained widget tree that is constructed once and mutated in place. Excluded: per-frame rebuild, Elm `update`→`view` rebuild, VDOM/view diffing, reactive signals, runtime property reflection, animation. Every design in this space must answer three questions: **who owns state**, **how a change reaches the tree** (setter / message / rebuild), and **how invalidation is expressed** (layout vs paint vs nothing). Most of the ergonomic pain in Rust comes from question two; most of the correctness pain from question three.

---

## 1. Immediate mode: rebuild-as-layout

egui has no widget tree. `Ui` is a transient handle to a rectangle plus a `Layout` cursor; `ui.add(widget)` allocates space, paints into a `Painter` immediately, and returns a `Response`. Layout is *derived from call order*, so it can only be known by running the code:

```rust
CentralPanel::default().show(ctx, |ui| {
    if ui.button("Increment").clicked() { count += 1; }   // no widget exists afterwards
    ui.label(format!("{count}"));
    ui.collapsing("Advanced", |ui| {});                    // open state lives in ctx.memory
});
```

Anything that must survive a frame lives in `Context::memory`'s `IdTypeMap`, keyed by an `Id` hashed from the scope id, an `id_salt`, and an auto-id counter (`ui.push_id`, `ui.next_auto_id`). Identity is therefore *positional*: insert a widget before a scrolling region and the scroll offset binds to the wrong widget — the canonical immediate-mode bug. Multi-pass layout (`ctx.request_discard`) exists because size-dependent decisions need a second run.

Dear ImGui is the same model in C++: `Begin`/`End`, widgets returning `bool`, ambient "last item" state (`IsItemHovered`), `PushID`/`PopID`, persistence in `ImGuiStorage::GetIntRef`, a fresh `ImDrawList` every frame. imgui-rs Rustifies the closures (`ui.input_text("n", &mut buf).build()`).

**A static retained design must not:** (a) derive layout from construction order; (b) make the `Ui`/context handle the only route to a widget; (c) key persistent state by hashed call-site position; (d) require a full pass to answer "what is under the mouse"; (e) rebuild vertex/draw data for untouched subtrees. egui's `Memory` is exactly the hidden side-table a real tree replaces with fields on nodes.

## 2. Elm and rebuild-with-diff

iced keeps *retained widget state* (text cursors, scroll offsets in `widget::Tree`) but throws away the *view description* every event:

```rust
impl Application for App {                 // `Program` in newer releases
    type Message = Msg;
    fn update(&mut self, m: Msg) -> Task<Msg> { ... }
    fn view(&self) -> Element<Msg> { column![button("+").on_press(Msg::Inc), text(self.n)].into() }
}
```

`view()` builds a cheap `Element` tree that is diffed (`Widget::diff`, `children()`) into the existing widget tree, so the app never names a node. `Task`/`Subscription` add async effects. Dioxus/Freya is the same shape with an RSX macro and a real VDOM (`rsx!{ div { button { onclick: .. } } }`), Freya adding CSS-like styling over the native renderer. Cost: an event → whole-subtree description rebuild → diff. Benefit: updates are a pure function of state, so the "I forgot to call the setter" bug class cannot happen; a static design must replace that guarantee (lesson 10).

## 3. Declarative + reactive

**Slint** is the most instructive: a `.slint` DSL is compiled into a Rust struct that is a cheap `Clone` handle to an item tree, with generated typed accessors and callback slots:

```rust
slint::include_modules!();      // counter.slint -> struct Counter
let ui = Counter::new()?;       // handle, not a tree of Box<dyn Widget>
ui.set_count(3);                // generated setter: typed, no downcast, no string key
ui.on_increment(|| { ... });    // generated callback: a field with a setter, not a trait
ui.show()?;
```

Properties form a dependency graph, so bindings re-evaluate automatically; lists use `ModelRc<T>`/`VecModel`. The lesson for static mode is the *generated façade*: name-level ergonomics (setters/getters/callbacks per property) achieved by codegen at compile time instead of runtime reflection.

**SwiftUI**: `body` recomputed as a function of `@State`/`@Observable`; `some View` avoids erasure until you write `AnyView`; `@Binding` is a writable projection. **GPUI** is SwiftUI's model with explicit invalidation — retained `Entity<T>` + `impl Render`, elements rebuilt on `cx.notify()`:

```rust
impl Render for Counter {
    fn render(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(self.count.to_string())
           .on_click(cx.listener(|this, _, cx| { this.count += 1; cx.notify(); }))
    }
}
let view: Entity<Counter> = cx.new(|_| Counter { count: 0 });
```

**Vizia** and **Druid** use lenses instead of signals — a zero-reflection, type-checked path from `&App` to `&Field`:

```rust
#[derive(Lens)] struct App { count: i32 }
Button::new(cx, |cx| Label::new(cx, App::count.map(|a| a.to_string())))
    .on_press(|cx| cx.emit(AppEvent::Inc));    // Handle<T> + cx.emit/needs_layout
```

**Floem** (`create_signal`, `view!`, `SignalGet/Set`) marks dirty `ViewId`s and re-renders only those nodes; **Xilem** is Elm-shaped but diffs *typed views* rather than a VDOM — `View::build`/`rebuild(&mut self, prev, ..) -> ChangeFlags` with `ViewId` identity, so child widgets are patched in place and `ViewSequence` handles keyed lists. Reactive truth: all of these still re-run user code per change; they only narrow *what* re-runs.

## 4. Retained static / mutable

**Qt Widgets** is the reference implementation of the target shape. Parent-owned `QObject` tree, mutable members, setters that invalidate:

```cpp
auto *label = new QLabel("0", panel);            // parenting = ownership
auto *btn   = new QPushButton("Increment", panel);
connect(btn, &QPushButton::clicked, label, [label]{ label->setText(...); });
```

`setText` calls `update()` (repaint) and possibly `updateGeometry()` (relayout up the parent chain via `QLayout::invalidate`) — explicit, separated granularity, the single best idea to steal. Events are virtuals (`event`, `paintEvent`) or an `installEventFilter`; signals/slots need `moc`/`Q_OBJECT`, and QSS selectors (`QPushButton#ok:hover`) need the `Q_PROPERTY` reflection a static Rust design is trying to avoid.

**LVGL** is the embedded-C version: raw `lv_obj_t*`, explicit parents, `lv_obj_set_*` setters, shared `lv_style_t` objects applied by (part, state) selector, `lv_obj_add_event_cb(obj, cb, LV_EVENT_CLICKED, user_data)`, and `lv_obj_invalidate(obj)` when external data a custom draw depends on changes.

```c
lv_obj_t *btn = lv_button_create(lv_screen_active());
lv_obj_set_size(btn, 120, 44);
lv_obj_add_style(btn, &style_btn, LV_PART_MAIN | LV_STATE_PRESSED);
lv_obj_add_event_cb(btn, on_click, LV_EVENT_CLICKED, app);   // void* erasure
```

**Masonry** is the Rust-native answer and the closest template: widgets live in an arena owned by the render root, addressed by generational `WidgetId`; children are `WidgetPod<dyn Widget>`; the `Widget` trait is *one method per pass* (`measure`, `layout`, `compose`, `paint`, `on_pointer_event`, `update`, `register_children`, `children_ids`, `accessibility`) with an associated `type Action`. Mutation only through a `WidgetMut<'_, W>` guard carrying the invalidation context, from `root.edit_widget(id, |w| ..)` or deferred `mutate_self_later`:

```rust
impl Widget for Counter {
    type Action = CounterAction;
    fn on_pointer_event(&mut self, ctx: &mut EventCtx<'_>, _p: &mut PropertiesMut<'_>, ev: &PointerEvent) {
        if matches!(ev, PointerEvent::Down(_)) {
            self.n += 1;
            ctx.request_paint_only();                     // or ctx.request_layout()
            ctx.submit_action::<CounterAction>(CounterAction::Changed(self.n)); // bubbles to root
        }
    }
    fn children_ids(&self) -> ChildrenIds { self.children.iter().map(|c| c.id()).collect() }
}
```

**Druid** threaded `&mut Data` through every pass (`event(.., data: &mut App)`, `update(&mut self, ctx, old, new)`, `layout(.., bc) -> Size`) and wrapped children in `LensWrap` to project sub-state — no `Rc<RefCell>`, but the data type infects every widget. **KAS** did the same generic-data trick with a `#[widget]` derive generating typed setters and `Sender<T>` messages; **Ribir** uses `#[derive(Declare)]` + a `widget!` macro, CSS-like classes, and `Stateful`/`Writer` for fine-grained dirt; **Makepad** puts the tree in a `live_design!` DSL over `#[derive(Live, Widget)]` structs (`draw_walk`/`handle_event`).

## 5. Comparison on the axes that matter

| Control | State owner | Update channel | Invalidation | Styling | Events |
|---|---|---|---|---|---|
| egui / ImGui | hidden `Memory` map | rebuild frame | none (always) | `Style` struct clone | return values |
| iced / Dioxus | app struct | rebuild + diff | implicit via diff | theme fn / CSS | `Message` enum |
| Slint | generated handle | setters + bindings | property graph | `.slint` + selectors | generated callbacks |
| SwiftUI / GPUI | view struct / `Entity` | body / `render` re-run | `cx.notify()` | modifiers | closures |
| Vizia / Floem / Xilem | model / signal / app | events + diffs | dirty flags | modifier fns | events, lenses |
| Qt Widgets | QObject members | setters | `update` / `updateGeometry` | QSS (needs reflection) | signals/slots, virtuals |
| LVGL | per-obj user data | `lv_obj_set_*` | `lv_obj_invalidate` (coarse) | style objects + part/state | C fn + `void*` |
| Masonry | widget fields + app `Action` | `WidgetMut` | `request_layout/paint_only` | plain fields / theme | routed `Action` |
| Druid | app `Data` + `Lens` | `&mut Data` per pass | `ctx.request_layout()` | `Env` | commands/selectors |

**Lists.** egui/iced/Vizia rebuild the whole list description each event; Qt does *not* — `QListView` + `QAbstractItemModel` means N rows are not N widgets, the key precedent for collections. LVGL has no virtualization: `lv_list` is a container with N children. Masonry/Ribir make you own the `Vec<WidgetPod<_>>` and call `ctx.children_changed()`. A static API should ship a keyed collection widget (`insert/remove/update(index)`) so app code never holds N handles.

**Text.** Text forces a shaped-layout cache inside the widget (`Parley`/`cosmic-text`) plus cursor/selection state; `set_text` must request layout, not paint.

**Static vs dynamic.** Qt/LVGL are maximally runtime-dynamic (`qobject_cast`, `lv_obj_set_user_data`); Slint/Makepad trade that for compile-time-generated accessors, Druid/KAS for generics. Static retained sits in the middle: fixed tree, dynamic *values*.

## 6. What breaks in Rust

- **Self-referential trees.** Children cannot borrow parents; either arena + index (Masonry) or `Rc<RefCell<Node>>` (Ribir, Slint-like handles). The arena keeps `Send`/`no-cycle`/deterministic drop; `Rc<RefCell>` keeps ergonomics and leaks cycles.
- **Callbacks borrowing state.** `on_click(|| app.count += 1)` needs `'static + FnMut`, so app state becomes `Rc<RefCell<App>>` or `Arc<Mutex<_>>`, and the handler runs while the tree is borrowed → `already borrowed: BorrowMutError` panics at runtime. Solutions seen in the wild: queue events and drain after the pass (LVGL's event loop), or bubble typed `Action`s to the root (Masonry), or take `&mut Data` explicitly (Druid/KAS).
- **`&mut self` while walking the tree.** You cannot hold `&mut Parent` and `&mut Child` from the same arena slot. Masonry's `WidgetMut` splits the widget borrow from the register/context borrow; the alternative is `Option::take`/`mem::replace` juggling, which is where most hand-rolled retained Rust GUIs rot.
- **Heterogeneous tree + typed setters.** `Box<dyn Widget>` erases `set_text`; recovering it needs `Any` downcasts (panics on mismatch) or codegen. This is exactly why Slint generates setters and why Masonry hides downcasts behind `edit_widget::<W>(id, ..)`.
- **Handle identity.** Raw pointers (Qt/LVGL) dangle; generational ids (Masonry `WidgetId`) need `Option` lookups but survive removal; `Rc` handles (Slint) are `Clone` and cheap but must be `Weak`-able to avoid leaking the app.
- **Callback erasure vs enums.** `Box<dyn FnMut(&mut Ctx)>` allocates per handler, kills `Clone`, and needs HRTB `for<'a>`; an event enum is `'static + Clone + Send` and serializes for free. Static-retained Rust APIs should default to the enum and offer closures only as sugar that *queues* an event.

## 7. Lessons for a static retained Rust API

1. **Own the tree in an arena; hand out `Copy` generational ids.** Never expose `&mut Node` or `Rc<RefCell<Widget>>`. `WidgetId` + `get(id) -> Option<&W>` preserves aliasing rules and makes removal detectable.
2. **One mutation door: a scoped guard.** `ui.edit(id, |w: &mut Button, ctx: &mut EditCtx| ..)` (Masonry's `WidgetMut`) lets setters record invalidation instead of asking the caller to remember. No `set_*` that silently fails to repaint.
3. **Split invalidation into paint / layout / children-changed** and have every built-in setter pick the cheapest correct one: colour → paint, text or size → layout, insert/remove → children-changed. Qt's `update()` vs `updateGeometry()` is the model; LVGL's single `lv_obj_invalidate` is the counter-example.
4. **Identity independent of construction order** — explicit id or keyed children `Vec<(Key, WidgetId)>`. A static tree is *edited*, so positional hashing (egui/ImGui) is unusable.
5. **Route typed `Action`s to one root handler** where `&mut App` lives; keep stored closures for purely local behaviour only. That root handler is the sole place app state and tree access coexist safely.
6. **Widget-local state in the widget** (scroll, press/hover, cursor), app state in the app, joined by id — never a global `Memory` map keyed by hash. Widgets stay `Default`-constructible and headless-testable.
7. **Layout stays a pure function of constraints and cached text metrics** (`measure(bc) -> Size`, `layout(size)`, `paint(rect)`). "Static" must not mean "fixed size".
8. **Ship a keyed collection widget** over `Vec<Row>` with `insert/remove/update` rather than N child handles; adopt Qt's model/view split if virtualization is ever needed.
9. **Get Slint-style ergonomics from codegen, not reflection:** a derive or hand-written typed wrapper per widget emitting `set_x`/`on_x` with the right invalidation, so users never downcast and never touch strings.
10. **Offer one `sync(&mut self, data: &App)` per screen** as the escape hatch from setter sprawl: the static-retained analogue of `view()`, run on data change rather than per frame — the total "state → UI" function without a diff.

**References:** [egui `Ui`](https://docs.rs/egui/latest/egui/struct.Ui.html) · [egui `Memory`](https://docs.rs/egui/latest/egui/struct.Memory.html) · [Dear ImGui](https://github.com/ocornut/imgui) · [imgui-rs `Ui`](https://docs.rs/imgui/latest/imgui/struct.Ui.html) · [iced](https://docs.rs/iced/latest/iced/) · [Dioxus](https://dioxuslabs.com/learn/0.6/) · [Freya](https://github.com/marc2332/freya) · [Slint generated code](https://docs.slint.dev/latest/docs/rust/slint/docs/generated_code/struct.SampleComponent) · [SwiftUI `View`](https://developer.apple.com/documentation/swiftui/view) · [Vizia](https://docs.rs/vizia/latest/vizia/) · [Floem](https://github.com/lapce/floem) · [GPUI](https://docs.rs/gpui/latest/gpui/) · [Xilem](https://github.com/linebender/xilem) · [Qt Widgets](https://doc.qt.io/qt-6/qtwidgets-index.html) · [Qt signals/slots](https://doc.qt.io/qt-6/signalsandslots.html) · [LVGL objects](https://docs.lvgl.io/master/widgets/obj.html) · [LVGL events](https://docs.lvgl.io/master/overview/event.html) · [LVGL styles](https://docs.lvgl.io/master/overview/style.html) · [Masonry](https://docs.rs/masonry/latest/masonry/) · [Masonry container guide](https://github.com/linebender/xilem/blob/main/masonry/src/doc/implementing_container_widget.md) · [Druid `Lens`](https://docs.rs/druid/latest/druid/trait.Lens.html) · [Makepad](https://github.com/makepad/makepad) · [KAS](https://docs.rs/kas/latest/kas/) · [Ribir declarative UI](https://ribir.org/docs/next/concepts/declarative_ui/)
