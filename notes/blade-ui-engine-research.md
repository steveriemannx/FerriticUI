# Engine architecture for a static, retained 2D GUI on `blade-graphics`

*Versions verified at time of writing: blade 0.9.0, winit 0.30.13, parley 0.12.0, taffy 0.15.1, swash 0.2.10, lyon 1.0.19.*

## 1. What blade provides, and what it deliberately doesn't

`blade-graphics` 0.9 (MIT, Rust ≥ 1.92) is a thin, explicit HAL: `Context::init(ContextDesc { presentation, validation, overlay, timing, .. })` over Vulkan/Metal/GLES3; WGSL via a pinned naga fork; `create_render_pipeline(RenderPipelineDesc { data_layouts, vertex, vertex_fetches, primitive, depth_stencil, fragment, color_targets, multisample_state })` and `create_compute_pipeline`; buffers (`Memory::{Device, DeviceTransient, Shared, Download, Upload}`); textures (`TextureDesc { format, size, sample_count, usage: COPY|TARGET|RESOURCE|STORAGE }`), views, samplers; and a `CommandEncoder` with `init_texture()`, `transfer()`, `render(name, RenderTargetSet { colors, depth_stencil })`, `present(frame)`, `present_with_damage(frame, &[ScissorRect])`, `submit() -> SyncPoint`, `wait_for(&sp, ms)`. Targets take `InitOp::{Load, Clear, DontCare}` and `FinishOp::{Store, Discard, ResolveTo(TextureView), Ignore}`, so MSAA resolve and load/store elision are explicit. `Capabilities` exposes `sample_count_mask`/`compute`; `DisplaySync::{Block, Recent, Tear}` plus damage rects set presentation policy.

It provides **nothing 2D**: no path filling, no AA policy, no batching or draw sorting, no clip stack, no gradients, no text, no glyph atlas, no resource lifetime manager (you own every `destroy_*`, sequenced against `SyncPoint`s), no frame pacing or resize policy beyond `reconfigure_surface`.

**`blade-egui` (~420 LoC) is the reference integration.** One pipeline: group 0 `Globals { r_uniforms }` (screen size), group 1 `Locals { r_texture, r_sampler }`; vertex `GuiVertex { pos: [f32;2], uv: [f32;2], color: u32 }` matching `egui::epaint::Vertex`; premultiplied blend (`One / OneMinusSrcAlpha`). Geometry goes through two `BufferBelt`s (`alloc_pod`, `flush(&sync_point)` after submit, `trim(4, &context)` per frame). Textures are `Rgba8Unorm`, `COPY|RESOURCE`, staged through the vertex belt and blitted with `copy_buffer_to_texture(src, 4 * width, TexturePiece { .. }, extent)` after `init_texture()`; destruction is deferred through `Vec<(GuiTexture, SyncPoint)>`, drained only when `wait_for(&sp, 0)` reports completion. Per draw: clip rect → physical px → `set_scissor_rect`, bind group 1, `bind_vertex(0, buf)`, `draw_indexed(index_buf, IndexType::U32, count, 0, 0, 1)`. Per frame: `acquire_frame()` → `init_texture(frame.texture())` → render into `frame.texture_view()` → `present(frame)` → `submit()`, with `wait_for(previous_sync_point)` gating belt reuse.

Its limits define the work: one texture binding per draw, scissor-only clipping, no MSAA, no damage tests, no stencil.

## 2. Geometry: tessellation vs analytic coverage

**Analytic boxes (primary).** For rects, rounded rects, borders and shadow silhouettes, don't tessellate: one quad, signed distance in the fragment shader (`sdRoundedBox`), coverage `clamp(0.5 - d / fwidth(d), 0.0, 1.0)`. Resolution-independent, no MSAA, crisp 1 px borders (band between inner and outer rounded rects), per-corner radii free, 4 vertices per primitive.

**`lyon` for paths and icons.** `FillTessellator::tessellate_path(&path, &FillOptions::default(), &mut BuffersBuilder::new(&mut vb, ctor))`, `StrokeTessellator` for strokes. lyon has *no* built-in AA ("achieved by users ... msaa, taa, fxaa"), so paths need MSAA (`sample_count: 4` + `FinishOp::ResolveTo(frame_view)`, forcing a second non-MSAA UI pass — blade's `particle` example). Fine for icons, bad for 1 px strokes.

**Compute rasterization (`vello` 0.11, wgpu-bound).** vello's pipeline — flatten to segments, bin tiles in compute, resolve analytic per-pixel coverage — is the quality ceiling: no MSAA, exact thin-feature coverage. Blade can host it (compute, `STORAGE` textures, indirect dispatch), but porting means rewriting vello's bind-group/shader plumbing into blade's `ShaderData` + naga (weeks), and GLES has no compute. Borrow the design; use `vello_cpu` 0.3 (`RenderContext`, `fill_path`, `fill_glyphs`) as golden oracle. **Rejected:** stencil-then-cover (two passes per path, pathological on tiled GPUs); SDF/MSDF except for icons.

**Batching.** Key by `(layer, pipeline, atlas, blend, clip-id)`; a GUI uses one or two atlases (A8 glyphs, RGBA8 icons/emoji), so a frame collapses to a handful of draws: one instance buffer per frame, one static quad index buffer, `draw(0, 6, first_instance, instance_count)` with `instanced: true`, one pipeline branching on `prim_kind`.

**Clipping/layers.** An axis-aligned clip is `set_scissor_rect` intersected on CPU — free. Rounded clips and group opacity need a `TARGET|RESOURCE` layer at the *clip's bounds* (not window-sized) plus a composite pass masking it with a rounded rect: allocation, clear, one extra read+write of that area, a pipeline switch. Cache by content revision; window-sized layers per card are the classic budget killer.

## 3. Text

Pipeline: itemize → shape → layout → rasterize → atlas → quads.

- **Shaping**: `harfrust` (Rust HarfBuzz port, used by cosmic-text 0.19 and parley 0.12); `rustybuzz` 0.20.1 is stalled (2024). **Fallback**: `fontique` 0.12 (enumeration, generic families, script/locale-aware fallback) or the lighter `fontdb` 0.24.
- **Layout**: `LayoutContext::ranged_builder(&mut font_cx, text, scale, quantize)` → `.build(text)` → `break_all_lines(Some(max_advance))` → `set_alignment(Alignment::Start, AlignmentOptions::default())`; iterate `layout.lines()` / `PositionedLayoutItem::{GlyphRun, InlineBox}`, `glyph_run.positioned_glyphs()` yielding `Glyph { id, x, y, .. }`. Pass `quantize = true` — parley calls pixel-quantized coordinates "the easiest way to avoid blurry text".
- **Rasterization**: `swash` 0.2.10 — `builder(FontRef::from_index(data, 0)?).size(px).hint(true).build()`, then `Render::new(&[Source::Outline, Source::ColorBitmap(StrikeWith::BestFit)]).format(Format::Alpha).offset(x, y).render(&mut scaler, glyph_id)` → `Image { placement, data }`. Hinting (essential at 11–13 px on 1x), subpixel offsets, COLR/CPAL and CBDT/sbix emoji. Alternative: `skrifa` 0.49 outlines into `vello_cpu`'s `fill_glyphs` — quality AA without hinting, a 2x-DPI path.
- **Atlas**: A8 for text, RGBA8 for emoji. Key `(font_id, glyph_id, size_q, subpixel_x 0..4, subpixel_y 0..4, hint, format)`; four x-phases fix fractional spacing without hinting. Lay out logically, rasterize at `px * scale_factor`, round origins to physical pixels before emitting quads.
- **Gamma**: coverage is linear, the display sRGB; `One / OneMinusSrcAlpha` into an sRGB target over-darkens edges. Render linear (`ColorSpace::Srgb`, `Rgba8UnormSrgb`) or apply a fixed coverage contrast curve (the "too bold / too thin" platform knob).
- **Why tessellated outlines (FerroUI's blade backend) cost so much**: a 14 px glyph is order 200–400 lyon vertices (~5–10 KB) vs 4 vertices / ~96 bytes from an atlas, so a 60-glyph line is ~500 KB of vertex traffic plus ~10–50 µs/glyph of tessellation *per frame* — with no hinting, no subpixel control, MSAA-only AA, and no color emoji. Tessellation only pays for large or transformed display text, where MSDF caching still wins.

## 4. Layout

- **`taffy` 0.15.1** (MIT, zero deps): flexbox + grid + block; `TaffyTree::new()`, `new_leaf(Style)`, `new_with_children(Style, &[NodeId])`, `compute_layout(root, AvailableSpace::Definite(w))`, `tree.layout(node) → &Layout { size, location, .. }`. Buy it; flexbox semantics are a swamp.
- **`morphorm` 0.9** (MIT) is smaller and opinionated (Floem); **custom layout** is unjustifiable.
- **Text measurement** is the hard seam: taffy wants min/max-content sizes. Break the same parley `Layout` with `break_all_lines(None)` and with the min-content width; cache `(text, style, width, dpi) → (size, Layout)`. Never re-shape during hit-test or paint. **Bidi/line breaking** come free with parley (ICU4X) or cosmic-text; defer float/inline-flow.

## 5. Platform/windowing

winit 0.30 `ApplicationHandler` (`resumed`, `window_event`, `about_to_wait`, `suspended`) + `EventLoop::new()?.run_app(&mut app)`; create the window in `resumed` via `create_window(Window::default_attributes())`, then `context.create_surface_configured(&window, config)` (blade needs raw-window-handle 0.6 `HasWindowHandle + HasDisplayHandle`).

- **Resize / DPI**: `WindowEvent::Resized` → `reconfigure_surface(&mut surface, SurfaceConfig { size, usage: TARGET, .. })`, skipping frames while any dimension is 0; `ScaleFactorChanged` also relayouts and flushes the atlas.
- **Redraw**: static UI → `event_loop.set_control_flow(ControlFlow::Wait)`, `request_redraw()` only on state/animation/IME change, render in `RedrawRequested` (not `about_to_wait`, which blade's `bunnymark` uses because it animates). For animation, `ControlFlow::WaitUntil(next_frame)` + `pre_present_notify()` before submit.
- **Input**: translate `CursorMoved`, `MouseInput`, `MouseWheel`, `KeyboardInput { event: KeyEvent { physical_key, text, state, .. } }`, `ModifiersChanged` into your own enum in *logical* px (`physical / scale_factor`), then hit-test the tree (rounded-rect distance, reverse paint order, clip-respecting).
- **IME**: `WindowEvent::Ime(Ime::{Enabled, Preedit(text, cursor), Commit(text), Disabled})`, `set_ime_allowed(true)` on focus, `set_ime_cursor_area(pos, size)`.
- **Clipboard**: winit has none; `arboard` 3 (worker thread on X11). **Accessibility**: `accesskit` 0.25 + `accesskit_winit`; push `TreeUpdate` from the retained tree, map `ActionRequest`s into your event enum — it constrains node identity, so design early.

## 6. Crate choice table

| Role | Candidate (version) | Maturity / license | Decision |
|---|---|---|---|
| GPU HAL | `blade-graphics` 0.9, `blade-util` 0.5, `blade-macros` 0.3 | pre-1.0, churny / MIT | Depend, pin `=0.9.x`, wrap in `gpu`, never leak blade types. naga is a pinned git fork → `cargo vendor`. |
| Tessellation | `lyon` / `lyon_tessellation` 1.0.19 / 1.0.23 | mature / MIT OR Apache-2.0 | Depend (paths, icons, strokes only). |
| Text layout + shaping | `parley` 0.12, `fontique` 0.12, `harfrust` 0.15 | pre-1.0, active / MIT OR Apache-2.0 | Depend; wrap so the API is yours. |
| Glyph raster | `swash` 0.2.10 | mature / MIT OR Apache-2.0 | Depend (hinting, color fonts). |
| Font parsing + CPU raster | `vello_cpu` 0.3, `skrifa` 0.49, `kurbo` 0.13 | young / MIT OR Apache-2.0 | Dev-dependency (goldens, software fallback). |
| Layout | `taffy` 0.15.1 | mature / MIT | Depend. |
| Windowing | `winit` 0.30.13 (0.31 beta) | mature / Apache-2.0 | Depend. |
| Clipboard, a11y | `arboard` 3, `accesskit` 0.25 + `accesskit_winit` | active, churny / MIT OR Apache-2.0 | Optional; pin the pair. |
| Atlas packing | `etagere` 0.3 / `guillotiere` 0.7 | stable / MIT OR Apache-2.0 | Depend, or vendor ~150 lines of shelf allocator. |

**Build vs buy**: buy layout, shaping, fallback, glyph raster, windowing. Build the renderer — batching, box analytic AA, clip stack, layer cache, atlas keying, damage — because nothing else speaks blade, and that layer is the quality bar.

## 7. Headless testing

blade's `tests/snapshot.rs` is the template: an `OffscreenTarget { texture, view, readback, size, texel_size }` with `TextureDesc { format: Rgba8UnormSrgb, usage: TARGET | COPY, sample_count: 1, .. }` and a `BufferDesc { size: w * h * 4, memory: Memory::Shared }`. Render with `RenderTargetSet { colors: &[RenderTarget { view, init_op: InitOp::Clear(TextureColor::TransparentBlack), finish_op: FinishOp::Store }], depth_stencil: None }`, then:

```rust
{ let mut t = encoder.transfer("readback");
  t.copy_texture_to_buffer(texture.into(), readback.into(), w * 4, extent); }
let sp = context.submit(encoder);
context.wait_for(&sp, 120_000).unwrap();
unsafe { std::ptr::copy_nonoverlapping(readback.data(), pixels.as_mut_ptr(), bytes) };
```

Compare with SSIM (blade: `SSIM_THRESHOLD = 0.95`, 8×8 blocks) against `tests/reference/*.png`, plus a per-pixel max-delta check so AA drift can't hide a regression. Keep goldens on one canonical backend (lavapipe in CI, `presentation: false`) and smoke-test the rest. Fix time, DPI (1.0 and 2.0) and animations; ship fonts in-repo; `BLESS=1` regenerates.

## 8. Recommended layering and top three risks

`platform` (winit loop, surface, input/IME/clipboard/a11y) → `ui` (retained tree, styles, invalidation, hit test, event routing) → `layout` (taffy + measurement cache) → `text` (fonts, shaping, atlas, quads) → `geom` (paths, SDF params, clip math) → `batch` (display list → sorted draws, damage) → `gpu` (blade wrapper), plus a dev-only `snapshot`.

The tree emits a display list only when dirty; `batch` turns it into a `RenderPlan` (instance arena, draw runs, scissor changes); `gpu` executes it. Keep the display list plain data (`Rect`, `Radii`, `Color`, `TextureId`, `ClipId`) so it is testable headlessly.

**Risk 1 — draw/state explosion.** Per-primitive binds and scissor sets (the `blade-egui` model) cap you at a few thousand primitives. *Mitigation*: one atlas set, sort by `(layer, pipeline, atlas, clip)`, one instance buffer per frame, scissor changes only on `clip_id` change; measure with `Timing::pass_durations()`.

**Risk 2 — fill/overdraw** from AA, shadows, layers. *Mitigation*: analytic coverage instead of MSAA, layers at clip bounds with `InitOp::DontCare` + scissor instead of full clears and cached by revision, shadows as one SDF quad instead of blurred layers, debug overdraw heat-map.

**Risk 3 — text cost and invalidation churn.** Re-shaping per frame, per-resize atlas clears and tessellated outlines show up as spikes. *Mitigation*: cache shaped layouts keyed `(text, style, width, dpi)`, A8 atlas with 4-phase keys and LRU, one staging upload per frame, invalidation only on font/DPI change. Biggest win here: after the first frame, an animation-free window should present damage rects or not present at all.
