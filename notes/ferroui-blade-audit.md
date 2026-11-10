# 调研笔记:FerroUI / FerroUI.Blade 审计——哪些是"动态"的,哪些可复用

> 调研子任务产出(2026-11),供 `docs/research/static-retained-gui-api.md` §6 使用。
> 计数不含 FerroUI 内的 `.dev/cargo-home` 缓存;LOC 为非测试 Rust 代码。

## A. FerroUI 分层

| 层 | crate / 目录 | 非测试 LOC | 备注 |
|---|---|---|---|
| 对象模型 + 属性系统 | `src/FerroUI.Base`(根 `.rs`、`property_store/`、`data/core/value_type.rs`) | ~23k | `FerroObject`、`TypeInfo`、`StyledProperty`、`DirectProperty` |
| 可视树 | `FerroUI.Base/visual.rs`、`visual_tree/`、`logical_tree/` | ~2.2k | `Visual` 同时是布局/输入/渲染节点 |
| 布局 | `FerroUI.Base/layout/` | 2.5k | measure/arrange + 失效队列 |
| 输入/事件 | `FerroUI.Base/input/`、`interactivity/` | ~24k | 焦点、手势、键盘、路由事件 |
| 样式 | `FerroUI.Base/styling/` | 6.9k | 选择器、样式、控件主题、setter |
| 数据绑定 | `FerroUI.Base/data/` | ~13k | 路径、转换器、`DataContext` |
| 动画 | `FerroUI.Base/animation/` | 10.4k | 时钟、过渡、关键帧、缓动 |
| 合成器 | `FerroUI.Base/rendering/composition/` | ~87k(32.5k 生成) | UI 线程 + 服务端 visual、动画机 |
| 渲染管线 | `FerroUI.Base/rendering/`(非 composition)、`platform/` | ~15k | 渲染循环、命中测试、`IPlatformRenderInterface` |
| 绘制/媒体模型 | `FerroUI.Base/media/` | 81.4k | 画刷、几何、文本格式化、图像 |
| 声明式 API | `FerroUI.View`、`FerroUI.View.Macros` | ~9k + 646 | `rsx!`、信号、reconciler |
| XAML | `Markup/FerroUI.Markup.Xaml{,.Loader,.WinUI}`、`FerroUI.Build.Tasks` | ~88k | XamlX 移植 + Rust 发射器 + IL 运行时 |
| 控件 | `FerroUI.Controls` | 231k(137k 测试) | 357 个根文件 + 40 个子目录 |
| 主题 | `FerroUI.Themes.{Fluent,Simple,WinUI,MacOS,Material}` | Fluent 47k(其中 `compiled_xaml.rs` 43.6k 生成) | 86 个 `.xaml` |
| 后端 | `Skia/`(40k)、`Vello/`(47k)、`Blade/`(10.4k)、`HarfBuzz/`(788)、OpenGL/Metal/Vulkan | | |
| 平台 | `FerroUI.Desktop`、`X11`(36k)、`Wayland`(24.7k)、`Win32`(49k)、`Headless`(6.8k)、`Browser`(21k) | | |

定义架构的 14 个文件:

1. `src/FerroUI.Base/lib.rs:8-80` — 对象模型模块地图。
2. `src/FerroUI.Base/styled_property.rs:21` — `StyledProperty`(样式化、按优先级解析、可继承)。
3. `src/FerroUI.Base/property_store/value_store.rs:78` — `ValueStore`,每对象的帧/优先级存储。
4. `src/FerroUI.Base/property_store/effective_value.rs:10` — `EffectiveValueDyn`、强制转换、继承解析。
5. `src/FerroUI.Base/styled_element.rs:32` — `StyledElement`(`DataContext`、`Styles`、`Theme`、`:951` 应用样式)。
6. `src/FerroUI.Base/visual.rs:28` — `Visual`:bounds、transform、clip、`render`(`:123`)。
7. `src/FerroUI.Base/layout/layoutable.rs:740,773` — `measure`/`arrange`,虚方法 `:252`/`:342`。
8. `src/FerroUI.Base/layout/layout_manager.rs:130,430` — 队列化布局 pass。
9. `src/FerroUI.Base/input/input_element.rs:31` + `input/focus_manager.rs:19` — 输入与焦点。
10. `src/FerroUI.Base/styling/style_base.rs:305` — `StyleInstance`(选择器命中的 setter 落地)。
11. `src/FerroUI.Base/rendering/composition/compositor.rs:49` → `server/server_compositor.rs:101` — 合成器。
12. `src/FerroUI.Base/platform/i_drawing_context_impl.rs:12` — 每个后端实现的绘制契约。
13. `src/FerroUI.View/mount.rs:542` — reconciler(`patch:1123`、`patch_control:1173`)。
14. `src/Blade/FerroUI.Blade/gpu/renderer.rs:546` — GPU 帧。

## B. 动态机制清单

### B1 属性系统(~5.8k LOC,基石)

`StyledProperty`(`styled_property.rs:21-428`)持有 `FerroProperty` 基类、按类型的 `MetadataTable`、路由;值存在 `ValueStore`(`value_store.rs:78`)里作为优先级帧:`Animation(-1) < LocalValue(0) < StyleTrigger(1) < Template(2) < Style(3) < Inherited(4) < Unset`(`data/binding_priority.rs:6`)。`EffectiveValue<T>`(`effective_value.rs:371`)选赢家、强制转换(`:400`)、算继承、发变更通知(`:391`)。`DirectProperty`(`direct_property.rs:286`)包装 getter/setter,永远本地优先级(`value_store.rs:238`)。

**移除:不干净。** `Visual`、`Layoutable`、`InputElement`、`StyledElement`、357 个控件、合成器、`ferroui-view`、XAML 全都假设 `&'static StyledProperty<T>` 句柄和 `type_system.rs`(1.6k)的类初始化机制。对象模型里没有"无属性"路径。

### B2 数据绑定(~13k LOC)

`BindingExpression`(`data/core/binding_expression.rs:88`)走 `CompiledBindingPath`(`compiled_binding_path.rs:303`)或 `ReflectionBinding`(`reflection_binding.rs:29`),经 `data/converters/`(7.7k,含 `composite_format.rs:1357`)转换,按优先级写回 store。`DataContext` 是 `StyledElement` 上的可继承 `StyledProperty`(`styled_element.rs:283-293`)。**移除:较干净**,是 store 的叶子消费者;代价是 `BindingValue`/`BoxedValue` 被 `ferroui-view` 复用。

### B3 样式(6.9k LOC,+测试 9.5k)

选择器链(`styling/selector.rs:10`,child/descendent/not/nth-child/property-equals/template),`Style`(`style.rs:12`)持选择器 + `Setter`s(`setter.rs:42`),`apply_style`(`styled_element.rs:966`)在 `FrameType::Style` 实例化 setter;`Styles`(`styles.rs:22`)、`ControlTheme`(`control_theme.rs:9`)、`ThemeVariant`(`theme_variant.rs:20`)、`ResourceDictionary`(`controls/resource_dictionary.rs:33`)。**移除:可当子系统删,但侵入**,因为伪类(`:pressed`/`:pointerover`)与失效路径挂在样式帧上。

### B4 动画/过渡(10.4k LOC)

`Animatable`(`animation/animatable.rs:16`)订阅属性变更、匹配 `Transition`、以 `BindingPriority::Animation` 绑 `AnimationInstance`;`TransitionBase`(`transition_base.rs:13`)暴露 `apply`/`value_type`;时钟可插拔。**移除:干净**(只有 `value_store.add_binding(..., Animation)` 与 `StyledElement` 的继承链)。

### B5 声明式 API(`ferroui-view`,~9k 手写 + 10.8k 生成 builder + 646 宏)

`rsx!`(`FerroUI.View.Macros/lib.rs:49`)、`#[component]`(`:408`)展开到 builders;`Element`(`element.rs:382`)按值比较;`mount.rs:542` 调和;`reactive.rs` 的 `Signal:316`/`Memo:457`/`effect:527`/`Owner:607` 是 thread-local 槽位句柄。**移除:干净**(纯上层)。

### B6 XAML(~88k LOC)

`FerroUI.Markup.Xaml.Loader`(146 文件,~60k):AST、~74 个 transformer、Rust 发射器、IL 运行时、解释器;`FerroUI.Markup.Xaml`(20.5k)是发射代码调用的运行时;`FerroUI.Build.Tasks`(3.7k)是构建期编译器;Fluent 出货 `compiled_xaml.rs`(43.6k 生成)。**移除:干净且强烈推荐**(全树最大可删块)。

### B7 合成器(~87k LOC)

UI 线程 `Compositor`(`compositor.rs:49`)镜像到 `ServerCompositor`(`server/server_compositor.rs:101`);`server/`(17.6k)有 20+ `ServerComposition*Visual`、池、脏矩形、锁;`generated/`(32.5k)是 schema 生成的 WinUI 合成属性块;`CompositingRenderer`(532)驱动 `render_loop.rs:15`/`DefaultRenderLoop:35` 与时钟。**移除:原地不干净,但概念上不需要**:静态库的树自己知道绝对变换,直接遍历发 op 即可。

## C. 静态库能"抬"走什么

- **文本(整块抬走)**:`FerroUI.Base/media/text_formatting/`(19.6k)**零** `StyledProperty`/`FerroObject` 引用(已 grep 验证)。`TextLayout`(`text_layout.rs:75`)、`TextFormatter`、`TextLine`、bidi(`bidi_reorderer.rs`)、断行、TAB、两端对齐、省略号;唯一外部依赖 `ITextShaperImpl`(`platform/i_text_shaper_impl.rs:8`),经 `FerroLocator` 解析(`text_shaper.rs:8`)。`FerroUI.HarfBuzz`(788,`harf_buzz_text_shaper.rs`)插在下面。字体(`media/fonts/`)是普通 struct。
- **布局(读后重写)**:`layoutable.rs`(894)+ `layout_manager.rs`(610)是忠实 Avalonia/WPF measure-arrange;`MinMax`、`LayoutQueue`。对属性系统的依赖局限在 `DesiredSize` 是 `DirectProperty`(`:359`)和 `affects_measure/arrange`(`:828`,`:842`,只是订阅 `property.changed()`)。换成纯字段 + 显式 `invalidate_measure()` 后算法几乎可逐字移植。面板实现(Grid 2.8k、StackPanel、DockPanel、WrapPanel、虚拟化栈)在 `FerroUI.Controls`,属性化,不能直抬,但算法可重写。
- **输入/焦点/手势(带改移植)**:`input/input_element.rs`(1178)、`focus_manager.rs`(1156)、`gestures.rs`(349)、`pointer_over_pre_processor.rs`(400)、`key.rs`(995)、`physical_key.rs`(804)。都写在 `InputElement: Interactive: StyledElement` 之上;`interactivity/`(1.3k,`RoutedEvent` 等)本身属性负载轻,值得抄。
- **主题资源(纯数据)**:`ControlTheme`/`ResourceDictionary` 是属性驱动,但内容是字面表(`Themes.Fluent/color_palette_resources.rs` 413 行、`Accents/`),可做成 struct/const 表,跳过 `compiled_xaml.rs`。
- **绘制契约**:`media/brush.rs`(443)、`brushes.rs`(727)、`drawing_context.rs:196`、`platform/i_drawing_context_impl.rs:12`(`push_clip:73`、`push_layer:85`、`push_opacity:104`、`push_geometry_clip:116`、`draw_glyph_run:66`)是干净的保留式绘制词汇;但约 47 个 media 文件是 styled object,静态版应用普通 enum。
- **平台后端**:`FerroUI.Desktop`(588)是薄胶水;`Headless`(6.8k)最可复用、最佳 CI 目标;`Win32`(49k)/`X11`(36k)/`Wayland`(24.7k)只做窗口,基本独立于属性系统。

## D. FerroUI.Blade

**模块**:`lib.rs:20-60` 列出 `gpu/`(引擎)、`helpers/`(图像编解码、mipmap、路径测量、像素格式)、`perf.rs`,以及 15 个实现 `ferroui_base::platform` 契约的 `*_impl.rs`。入口:`BladePlatform::initialize`(`blade_platform.rs:14`)、`use_blade`(`blade_application_extensions.rs:20`)、`BladeContext`(`blade_backend_context.rs:31`)、`DrawingContextImpl::new`(`drawing_context_impl.rs:84`);`gpu::Renderer`/`gpu::BladeGpu` 是 `pub`。

**场景/op 模型**:`gpu/ops.rs:49` 的 `SceneOp` = `Fill`/`Stroke`/`FillRect`/`ClearRect`/`MaskedQuad`/`TextureQuad`,每个都带已变换几何 + `PixelRect` scissor(`ops.rs:14`)。Paint = `Solid|Gradient|Texture`(`gpu/paint.rs:81`);渐变带 stops + shader uniform(不是顶点色)。**这个模型不碰任何 Avalonia 类型**,只用 `kurbo`、`peniko`、`TextureRef`。

**帧流程**:`BladeGpu::render`(`gpu/mod.rs:196`)→ `Renderer::render`(`gpu/renderer.rs:546`):`build_batches`(`:723`)用 lyon(`FillTessellator`/`StrokeTessellator`,tolerance 0.1)把 op 变成 `Batch::{Colors,Texture,Masked,Gradient}`;`draw_batches`(`:645`)经 `BufferBelt` 上传,4 条管线(`Renderer::new:365`、`PipelineSet:337`),预乘 alpha 混合、可选 MSAA、scissor。shader 在 `gpu/{color,gradient,masked,texture,present}.wgsl`。

**文本路径**:`font_manager_impl.rs:170` → `BladeTypeface`/`BladeFontFace`(`blade_typeface.rs:56`,skrifa 轮廓,合成粗斜体)→ `GlyphRunImpl`(`glyph_run_impl.rs:24`,skrifa glyph id + 位置,`glyph_outline_cache.rs` 缓存展平轮廓)。渲染是**轮廓式**:`draw_glyph_run`(`drawing_context_impl.rs:1001`)把每个 glyph 轮廓展平成 `BezPath` 再 `SceneOp::Fill`——**没有字形图集、没有亚像素 AA**。这是相对 Skia 的真实质量/性能差距,也是独立渲染器要用覆盖图集的原因。

**层/clip**:轴对齐 clip 变 scissor(`drawing_context_impl.rs:150-160`);非矩形 clip 渲 mask target 再经 `MaskedQuad` 合成(`create_mask:173`);分层/裁剪组用 scratch target(`push_composite_layer:195`、`compose_masked_surface:234`、`create_scratch_layer:291`)。**场景在 `dispose` 时渲染**(`:1186-1213`),这是 Avalonia 形状的生命周期,不是引擎必需。

**可复用拆分**(按 `ferroui_base` import 统计):~10.0k 库行中,约 **5.4k(52%)** 是 `*_impl.rs` + 平台接线;约 **3.3k(32%)** 完全 ferroui-free:`gpu/{renderer,ops,paint,texture,target,shaders}`(1.5k)+ `helpers/{mipmap,image_decoding,path_measure}`(1.0k)+ `perf.rs`(345)+ `glyph_outline_cache.rs`(134)。只有 `gpu/mod.rs:118` 碰 `ferroui_base::media::TileMode`(一个枚举即可解耦)。剩下 ~1.3k 混合(字体/typeface/bitmap 助手,重命名 trait 后可复用)。`window_render_target.rs` 是 **macOS 专用**(`lib.rs:37`,raw-window-handle + objc2);`framebuffer_render_target.rs` 是可移植的慢路径(GPU → 回读 → 平台 framebuffer)。

## E. 复用/重写/丢弃

| 资产 | 决定 | 工作量 | 原因 |
|---|---|---|---|
| `Blade/gpu/`(renderer/ops/paint/texture/target/shaders) | **复用**(vendor 成小 crate) | 3-5 天 | 零 Avalonia 类型;只内联 `TileMode` |
| Blade 几何:lyon tessellation + `geometry_impl` | **复用 tessellation,重写 wrapper** | 3-5 天 | `build_batches` 是价值;`geometry_impl.rs` 是契约仪式 |
| Blade 渐变/图像画刷/蒙版 | **复用** | 2-3 天 | LUT 纹理 + uniform 设计干净 |
| Blade 文本(skrifa 轮廓路径) | **复用,但计划替换** | 1-2 周 | 轮廓逐字正确但粗糙;保留 `BladeFontFace`/`glyph_outline_cache`,规划图集 |
| Blade swapchain(`window_render_target.rs`) | **重写** | 1 周 | macOS/AppKit 专用;用 raw-window-handle + `blade::Surface` 通用化 |
| Blade 15 个 `*_impl.rs` + 平台接线(~5.4k) | **丢弃** | 0 | 没有 `ferroui_base::platform` 就不需要 |
| 布局 measure/arrange 算法 | **移植**(翻译,不依赖) | 2-3 周 | ~1.5k 真实逻辑;去掉 `DesiredSize` 属性 + `affects_measure` 订阅 |
| 文本格式化(bidi/断行/排版) | **几乎逐字复用** | 1-2 周集成 | 无属性依赖;只保留 `ITextShaperImpl` + `ReadOnlyMemory<u16>` |
| HarfBuzz shaper crate | **复用** | 2 天 | 788 LOC,已是隔离的 `ITextShaperImpl` |
| 输入路由/焦点/手势 | **带重设计移植** | 3-4 周 | 算法好;`Interactive`/`StyledElement` 基底要换成自己的节点句柄 |
| 主题颜色/度量 as data | **提取成 const 表** | 1 周 | 忽略 `compiled_xaml.rs`,从 `color_palette_resources.rs` + `.xaml` 取值 |
| 属性系统、绑定、样式、动画 | **丢弃** | 0 | 静态设计的全部意义所在 |
| `ferroui-view`(rsx/reconciler/signals) | **丢弃** | 0 | 纯上层 |
| XAML 加载器/编译器(~88k) | **丢弃** | 0 | 最大可删块 |
| 合成器服务端(~87k)+ `Composition*` 模型 | **丢弃** | 0 | 直接用树遍历发 `SceneOp` |
| 控件库(231k) | **丢弃/窄化重写** | 4-6 周(最小集) | 每个控件都与属性系统绑定;只借算法与外观 |
| 平台窗口(Win32/X11/Wayland/Headless) | **选择性复用** | 每个 1-2 周 | 基本独立于属性系统;`Headless` 优先做 CI |

**结论**:FerroUI 的动态层不是可摘除的模块,而是基底(`StyledElement`/`Visual`/`Layoutable`/`InputElement` 都继承 `FerroObject` 并读 `&'static` 属性句柄)。静态库应把 FerroUI 当**参考实现与可移植算法来源**,而不是依赖。两处无条件收益:`FerroUI.Base/media/text_formatting`(19.6k,无属性)和 `FerroUI.Blade/gpu/`(~1.5k 干净 GPU 引擎,外加 lyon tessellation 与 gradient/mask 设计)。其余——布局 ~1.5k 逻辑、输入路由、主题表——是"读后重写",约 **8-12 工程师周**可做出聚焦的、Blade 渲染的静态 v1。
