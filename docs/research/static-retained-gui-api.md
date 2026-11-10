# FerriticUI:基于 Blade 的"纯静态保留模式" Rust GUI 库——调研与 API 设计建议

> 状态:调研 / 设计提案,未写实现代码。
> 上游依据:本地 `../FerroUI`(Avalonia 0.4 移植,动态保留模式)、本地 `../blade` 的 `main` 分支(`blade-graphics` 0.9,revision `30c94eb`,即 FerroUI 文档里的同一个 revision)、公开资料(见 §11)。
> 目标读者:决定 FerriticUI 要不要做、做成什么样的人。

---

## 0. TL;DR

**推荐结论:做"静态保留"的 GUI 库。有两条实现路线:稳妥线 B(运行时 arena 保留树 + 编译期强类型 API)与极致线 C′(扁平静态树,真正没有运行时节点图);建议先做 3-5 天 spike 再定路线。不要做编译期类型级树,也不要再做一个立即模式库。**

一句话定位:FerriticUI 是**一棵只构建一次、只被显式修改、从不重建/从不 diff 的控件树**,API 是**强类型字段**,更新是**消息到唯一根处理器**,渲染是**事件驱动、空闲零帧**,底层是 **blade-graphics 上的薄 2D 渲染器**。它没有 Avalonia/FerroUI 的属性系统、样式选择器、绑定、动画、合成器和 XAML,也没有 egui 的逐帧重建和 iced 的 `view()` 重建。

五个关键判断:

1. **"静态"要拆成三件事**:构造一次、显式更新、无反射/无动态属性系统。三者都做到,就同时躲开 Avalonia 的复杂度和 egui/iced 的重建模型;没必要(也不应该)把"静态"理解成"运行期一棵树都没有"。
2. **API 主形态 = 生成的强类型视图结构体 + 消息枚举**。用户在 `ui!` 里一次性描述树,宏生成 `struct Ui`、每个具名控件的类型化访问器、以及遍历/命中/绘制粘合代码。控件的状态留在控件里,业务状态留在 `App` 里,二者只在根部的 `update(&mut self, msg)` 相遇——这正是 Rust 借用检查最舒服的形状。
3. **两条实现路线,共用同一套渲染/布局/文本栈**:稳妥线 **B**(§3.2)——内部 arena 保留树 + 宏生成的类型化访问器;极致线 **C′**(§3.3b)——宏把树展开成扁平 struct 字段 + 生成的静态驱动代码,**真正没有运行时节点图**,API 是直接字段(`ui.value.set_text(..)`),代价是宏工程量更大、树形编译期固定。若"纯静态"是硬目标,选 C′;若要工程风险最小,选 B。真正的"编译期类型级树"(C)只作为极限记录,不实现。
4. **渲染不要照搬 FerroUI.Blade**。它的 `gpu/` 引擎(~1.5k 行,与 Avalonia 无关)可以复用;但它的文本是"字形轮廓逐个 tessellate + MSAA",对 UI 文本既慢又缺亚像素,应该新写"实例化 SDF 四边形 + 字形覆盖图集 + 纹理四边形"的渲染器。它的 swapchain/readback/gradient LUT 思路可直接借鉴。
5. **最大的隐藏成本不是渲染,是文本输入**(编辑模型、选区、IME、双向文本、无障碍)和**宏的工程质量**。路线图必须把它们当一等公民,而不是最后补。

差异化一句话:**Qt Widgets/LVGL 的更新模型 + Slint 生成式 API 的静态类型手感 + Masonry 的 Rust 所有权结构 + Blade 的 GPU 渲染**;目前 Rust 生态里没有这个组合。

---

## 1. "纯静态保留模式"到底指什么

### 1.1 三个正交的轴

任何 GUI 框架都可以在三个轴上定位:

| 轴 | 取值 | 说明 |
|---|---|---|
| A. 构造 | **一次构建** ↔ 每次重建 | 树是持久对象,还是每帧/每消息重新生成 |
| B. 更新 | **显式变更** ↔ 响应式传播 ↔ 消息+重建 | 谁把"数据变了"翻译成"界面变了" |
| C. 类型 | **静态类型/字段访问** ↔ 运行时反射/属性表 | 用户怎么碰到一个控件 |

- **保留模式(retained)** = A 轴的"一次构建":树持久存在,有身份、有状态、有布局缓存。
- **立即模式(immediate)** = A 轴的"每次重建"(egui、Dear ImGui)。
- **Elm/VDOM(iced、Dioxus/Freya)** = A 轴偏重建 + B 轴"消息 + 重新生成描述";它们虽然保留控件状态,但视图描述每次事件都重建、再 diff。
- **Avalonia/FerroUI** = A 轴保留 + B 轴响应式(属性优先级、样式、绑定、动画)+ C 轴运行时反射。
- **Slint** = A 轴保留 + B 轴响应式 + C 轴编译期生成访问器。
- **Qt Widgets / LVGL / Masonry** = A 轴保留 + B 轴显式变更 + C 轴(部分)类型化。**这是本项目的参照系。**

### 1.2 "静态"的四种读法

"纯静态"至少有四种意思,必须选一种,否则设计会摇摆:

| 读法 | 含义 | 代表 | 评价 |
|---|---|---|---|
| **S1 静态更新** | 树只建一次,值只能通过显式 setter 改;没有依赖图/绑定/响应式 | Qt Widgets、LVGL、Masonry | ✅ **本项目目标** |
| **S2 静态描述** | UI 描述在编译期固化(代码生成/DSL 编译),运行期不构建、不解析 | duilib 的 XML→C++ 生成模式、Slint 的 `.slint` 编译 | ✅ 作为**可选前端**(见 §4.12) |
| **S3 静态类型** | 没有反射/属性系统/`Any` downcast;用户 API 全是具体类型和字段访问器 | Slint 生成代码、Druid 泛型 | ✅ **本项目目标** |
| **S4 静态结构** | 运行期结构完全不可变,只重绘 | 极窄场景(仪表盘/HUD) | ❌ 放弃;真实应用总要有值变化和列表增删 |

本提案 = **S1 + S3**,把 S2 留作后续的设计器/热重载前端,放弃 S4。这样"静态"是**对用户可见的编程模型**的承诺(一次构建、显式更新、全程具体类型),而不是"内存里没有树"的极端承诺。

### 1.3 收益与代价(诚实版)

**收益**

- **可预测**:没有 diff、没有依赖图、没有隐式重算。一次事件 = 一次 `update` + 一次布局遍历 + 一次绘制。
- **空闲零帧**:静态 UI 只在有事发生时出帧,`ControlFlow::Wait` 下 CPU/GPU 可以完全空闲(对工具类、嵌入式和笔记本续航都重要)。
- **所有权简单**:树归 `App` 所有,消息回到 `update(&mut self)`,不需要 `Rc<RefCell<App>>`,不需要 `'static + FnMut` 回调,不会在事件派发中途触发 `BorrowMutError`。
- **二进制/代码量小**:没有属性元数据、没有反射、没有样式匹配、没有 XAML 加载器。FerroUI 的 XAML 层约 88k 行、合成器约 87k 行、属性系统约 5.8k 行、绑定约 13k 行、样式约 6.9k 行、动画约 10.4k 行——这些在静态库里都不存在。
- **可检查、可序列化**:树是 Rust 的普通结构;消息是普通 enum。测试、日志、inspect、甚至远程驱动都很直接。
- **可"整树重建"**:因为构建便宜、无隐藏状态,主题切换、热重载、设计器预览都可以用"丢掉重建"的粗暴办法(明确代价:控件内部状态会丢,见 §4.7)。

**代价**

- **忘记同步**:没有 `view()` 的"纯函数"保证,漏调 setter 就是不更新。这是 iced 相对静态库的最大优势,必须用别的手段补(§4.9 的 `sync`、debug 断言、inspector)。
- **结构变化成本高**:插入/删除/换行、条件分支、异构内容都要显式 API;不像 VDOM 那样随便重建。
- **没有动画**:核心不做(静态语义),需要的话是显式的逐帧 hook 或干脆重建。
- **主题/DPI 切换**:必须显式重放或重建。
- **文本输入**:控件的编辑状态、IME、选区、双向文本、字体回退,是任何 GUI 都绕不开的大工程。

### 1.4 与现有方案的差别(这是本项目存在的理由)

| 库 | 构造 | 更新 | 类型 | 本项目的差别 |
|---|---|---|---|---|
| egui | 每帧重建 | 逐帧闭包 | 传输式 `Ui` 句柄 | 不逐帧执行用户代码;控件是持久对象 |
| iced | 每消息重建 view + diff | `Message` + `update` | `Element<Msg>` | 不重建 view、不 diff;树就是控件本身 |
| FerroUI/Avalonia | 一次 | 属性系统/绑定/样式/动画 | 反射 + `Ref<dyn>` | 去掉整套动态机制,只留布局+输入+绘制 |
| Slint | 一次 | 响应式属性图 | 编译期生成的强类型句柄 | 纯 Rust、无 DSL/编译器、无属性图 |
| Qt Widgets | 一次 | setter + signal | `QObject` + 反射 + moc | 纯 Rust、无 moc/信号槽/样式表 |
| LVGL | 一次 | setter + 全局 invalidation | `lv_obj_t*` + `void*` | Rust 类型安全、GPU、细分失效 |
| Masonry | 一次 | `WidgetMut` + 路由 `Action` | `WidgetId` + `dyn Widget` | 用户 API 是强类型字段访问器,不是 id + downcast |
| GPUI | 一次 | `cx.notify()` 后重跑 render | `Entity` + 元素重建 | 不重建元素;状态在控件字段里 |

### 1.5 同一个计数器,三种写法

```rust
// egui:立即模式。没有控件对象,状态在闭包外面,每帧重新执行、重新布局
egui::CentralPanel::default().show(ctx, |ui| {
    ui.label(n.to_string());
    if ui.button("+").clicked() { n += 1; }
});

// iced:Elm。每条消息都重建 view 描述,再 diff 进控件树
fn update(&mut self, msg: Msg) -> Task<Msg> { self.n += 1; Task::none() }
fn view(&self) -> Element<Msg> { column![text(self.n), button("+").on_press(Msg::Inc)].into() }

// FerriticUI:保留 + 静态。构建一次;消息到根;根里显式改控件
impl App for Counter {
    type Msg = Msg;
    type Ui  = Ui;
    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        match msg { Msg::Increment => self.n += 1, Msg::Reset => self.n = 0 }
        self.ui.value().set_text(self.n.to_string());   // 没有 view()、没有 diff
    }
    fn view(&self) -> &Ui { &self.ui }
    fn view_mut(&mut self) -> &mut Ui { &mut self.ui }
}
```

三者的差别不在"谁更声明式",而在**每个事件要做多少工作**:egui 重跑整段 UI 代码,iced 重建整棵描述并 diff,FerriticUI 只跑 `update` 里那几行、再走一遍已缓存的布局。

---

## 2. 生态调研:API 形态能借什么

(完整调研笔记见 `notes/static-retained-api.md`,这里只留结论。)

### 2.1 立即模式(egui / Dear ImGui):反面的教材

egui 没有控件树:`ui.button(..)` 立即绘制并返回 `Response`,跨帧状态塞在 `Context::memory` 的 `IdTypeMap` 里,`Id` 由调用位置哈希得到。由此产生两个经典问题:插入控件会导致滚动位置错位(身份依赖构造顺序);大小相关的决策要 `request_discard` 跑第二遍。**静态库要反过来做**:身份显式、布局来自约束而不是调用顺序、持久状态放在控件字段上而不是哈希表里。

### 2.2 Elm/重建式(iced、Dioxus/Freya)

`update(msg)` → `view()` 重建 `Element` → diff 进控件树。优点是"更新是状态的纯函数",不可能忘记同步;缺点是每个事件都要重建并 diff 子树。**要借的是 `Message` 模型和 `Task`/`Subscription` 的边界感;不要借的是"每个事件重建视图"。**

### 2.3 声明式 + 响应式(Slint、SwiftUI、GPUI、Vizia、Floem、Xilem)

- **Slint** 最值得学:DSL 编译成一个 Rust struct,属性有生成的 getter/setter,回调是"字段式的 setter"(`ui.on_increment(..)`),列表用 `ModelRc`。**用代码生成换来"名字级别"的 API 手感,而不是运行期反射**——这正是 FerriticUI 的 `ui!` 宏要做的事。
- **GPUI**:`Entity<T>` + `impl Render`,在 `cx.notify()` 之后重跑 render 重建元素。它的"显式失效"很对,但元素重建不对。
- **Vizia/Druid 的 Lens**:`App::count.map(|a| a.to_string())` 是零反射的类型化投影。Druid 把 `&mut Data` 穿过每一趟布局/事件,类型会"感染"所有控件;这提示我们:**状态访问要收口到根部的 `update`,不要让数据类型污染控件类型。**
- **Xilem**:类型化 view 树 + `build/rebuild` 增量 patch,`ViewSequence` 处理元组/列表。它的"视图是类型"与本项目 §3.3 的编译期树最接近,但它仍然保留 view→widget 的两层和重建,本项目不需要。

### 2.4 保留 + 显式变更(Qt Widgets、LVGL、Masonry、Druid、Makepad、Ribir)

- **Qt Widgets** 是目标形状的参照实现:父对象拥有子对象、成员可变、setter 里做失效。**最值得偷的一个设计是失效粒度**:`setText` 触发 `update()`(重绘)而可能触发 `updateGeometry()`(重排);颜色只重绘,尺寸才重排。LVGL 只有一个 `lv_obj_invalidate`,粒度太粗,是反面例子。
- **LVGL**:`lv_obj_set_*` + 显式 `lv_obj_invalidate` + C 回调 + `void*`。它证明了"静态保留 + C"可行,但也暴露了 `void*` 类型擦除的痛苦——Rust 版必须用枚举和生成代码代替。
- **Masonry** 是 Rust 里最成熟的同构:**arena 拥有控件、`WidgetId` 泛型句柄、`WidgetPod<dyn Widget>`、每趟一个 trait 方法(`measure/layout/compose/paint/on_pointer_event/...`)、`WidgetMut` 统一变更入口、`ctx.request_layout()` / `request_paint_only()` 分级失效、`Action` 冒泡到根**。它证明:在 Rust 里做保留树,arena + 句柄 + 作用域变更守卫是最不痛的路。**FerriticUI 的内部结构应该像 Masonry,用户 API 不应该像 Masonry**(用户不该拿 `WidgetId` 再 `edit_widget::<Button>` downcast)。
- **Ribir / Makepad / KAS**:`#[derive(Declare)]`/`#[widget]` 宏生成声明式 API 与类型化 setter,证明"宏生成静态 API"在 Rust 里可行;它们的响应式/样式系统则是本项目要避免的。

### 2.5 十条可借鉴经验(来自调研笔记)

1. 树放 arena,对外只给 `Copy` 的世代 id;不暴露 `&mut Node`、不用 `Rc<RefCell<Widget>>`。
2. 变更只留**一个门**:作用域守卫(`ui.edit(id, |w, ctx| ..)`),让 setter 自己记录失效,而不是要求调用者记得。
3. 失效分三级:**重绘 / 重排 / 子节点变化**;每个内建 setter 选最便宜且正确的那一级。
4. 身份与构造顺序无关:显式 id 或键控子节点(`Vec<(Key, Id)>`)。
5. 类型的 `Action`/消息只路由到**唯一根部处理器**,那里同时拥有 `&mut App` 和树;控件内部行为尽量本地化。
6. 控件私有状态(滚动、按下/悬停、光标)放控件里;应用状态放 `App` 里;用 id 连接。绝不搞全局 `Memory` 哈希表。
7. 布局仍是约束的纯函数:`measure(constraints) -> Size`、`arrange(rect)`、`paint`;静态 ≠ 尺寸固定。
8. 集合控件要做**键控**的 `insert/remove/update`,而不是让应用持有 N 个句柄;需要时再上 model/view 虚拟化。
9. 用**代码生成**而不是反射来获得 Slint 式手感:派生宏/过程宏给每个控件生成 `set_x`/`on_x` 和正确的失效。
10. 提供一个 `sync(&mut self, data: &Data)` 的"整屏同步"逃生口:它是静态库版的 `view()`,在数据变化时跑,而不是每帧跑——**"状态 → UI"的纯函数,但没有 diff**。

---

## 3. 候选 API 形态

四种候选,先给结论:**B 为主 + A 作内部底座 + D 作后续前端;C 只作研究分支**。

### 3.1 形态 A:句柄/arena 型(LVGL/Qt/Masonry 式)

```rust
// 运行时树 + Copy 句柄 + 显式失效
let label = ui.label("0");                 // -> NodeId / 类型化 NodeId<Label>
let btn   = ui.button("+", |cx| cx.send(Msg::Inc));
ui.on(btn, Msg::Inc);
...
ui.set_text(label, "1");                   // setter 自带失效
ui.set_visible(btn, true);
```

- ✅ 与 Rust 所有权最兼容(树在 arena,句柄是 `Copy`);动态增删/异构/虚拟列表最自然;宏只需生成 id + 构建体。
- ❌ "静态类型"只做到一半:用户拿 id 再 `edit::<Label>` 是运行时 downcast,写错是 panic 不是编译错误;`ui.set_text(label, ..)` 丢掉了字段命名的手感。
- 适合做**内部实现**,不适合做**对外卖点**。

### 3.2 形态 B:强类型视图结构体 + 根消息(**推荐**)

```rust
ui! {
    msg = Msg;                             // 消息类型显式给出(宏需要)
    pub struct Ui {
        Window(title: "Counter", size: (320, 120)) {
            Column(padding: 12, spacing: 8) {
                value: Label(text: "0"),
                row: Row(spacing: 8) {
                    inc:   Button(text: "+",     on_click: Msg::Increment),
                    reset: Button(text: "Reset", on_click: Msg::Reset),
                }
            }
        }
    }
}

impl App for Counter {
    type Msg = Msg;
    type Ui  = Ui;
    fn update(&mut self, msg: Msg, cx: &mut Ctx) {
        match msg {
            Msg::Increment => self.n += 1,
            Msg::Reset     => self.n = 0,
        }
        self.ui.value().set_text(self.n.to_string());   // 显式、类型化
    }
    fn view(&self) -> &Ui { &self.ui }
    fn view_mut(&mut self) -> &mut Ui { &mut self.ui }
}
```

- ✅ 用户看到的是**静态类型字段访问器**(`ui.value().set_text(..)`),没有 id、没有 downcast、没有字符串;借用规则友好(树归 `App`);消息到根,跨控件联动容易;宏可以把遍历/命中/绘制粘合代码全部生成掉。
- ❌ 需要 `ui!` 过程宏(工程量与诊断质量是最大风险);消息类型要显式指定;集合与异构内容需要额外设计(§4.8)。
- **形态 B 的用户 API + 形态 A 的内部结构** 是本提案。

### 3.3 形态 C:编译期类型级树("纯静态"极致)

```rust
// 类型本身就是树:Window<Column<(Label, Row<(Button, Button)>)>>
// 无 arena、无 NodeId、无 dyn;布局/绘制/命中全部单态化
fn counter() -> impl View<Msg = Msg> {
    Window::new("Counter", Column::<(
        Label,
        Row<(Button<Msg>, Button<Msg>)>,
    )>::new(..))
}
ui! 生成 struct Ui(TreeType) + 访问器:
    fn value(&mut self) -> &mut Label { &mut self.0.content.0 }
```

- ✅ 字面意义的"纯静态":零运行时节点、零 id、零动态分派、零分配;布局/绘制完全单态化;缓存友好。
- ❌ 类型随 UI 规模增长(嵌套元组类型、编译期和 rust-analyzer 压力);异构/条件内容要用 `Option<C>`、`Either<A, B>`/手写 enum;宏要计算嵌套路径;错误信息与调试困难;`impl View` 组件会遮住内部字段。**对一个要长期维护的库,v1 不建议。**
- 但它是"如果将来要极致静态/嵌入式"的合理分支,且用户 API 与 B 可以长得几乎一样(底层从 arena 换成内联元组),保留升级路径。

### 3.3b 形态 C′:扁平静态树(要"完全极致"时,推荐这个而不是 C)

思路:树形由宏在**编译期**展开成"扁平 struct 字段 + 生成的节点地址枚举 + 静态驱动代码"。节点是 `Ui` 的字段(不是 arena 槽位);子节点关系是生成的 `const` 表;所有 pass(measure/arrange/paint/hit/event/focus/a11y)用生成的 `match` 做静态分派。**没有堆上的节点图、没有 `dyn`、没有反射、也没有 C 那种类型级嵌套。**

```rust
ui! { msg = Msg; pub struct Ui {
    Window(title: "Counter") { Column(padding: 12) {
        value: Label(text: "0"),
        inc:   Button(text: "+", on_click: Msg::Increment),
    }}
}}

// 展开(概念):
pub struct Ui<M> {
    pub value: Label,        // 节点 = 字段,直接访问
    pub inc:   Button<M>,
    window: WindowProps,     // 容器只存属性 + 算法,不拥有子节点
    column: ColumnProps,
}
enum Node { Window, Column, Value, Inc, Row(usize) }   // 生成的"节点地址"(Copy)
const WINDOW_CHILDREN: &[Node] = &[Node::Column];
const COLUMN_CHILDREN: &[Node] = &[Node::Value, Node::Inc];
const FOCUS_ORDER:     &[Node] = &[Node::Inc];          // 焦点顺序编译期确定

impl<M: Clone> Ui<M> {
    fn measure_node(&mut self, n: Node, c: Constraints) -> Size {
        match n {                                       // 静态分派,生成的 match
            Node::Value => self.value.measure(c),
            Node::Inc   => self.inc.measure(c),
            Node::Column => {
                let sizes = COLUMN_CHILDREN.iter().map(|&ch| self.measure_node(ch, c));
                self.column.arrange(sizes)              // 容器 = 属性 + 布局算法
            }
            _ => Size::ZERO,
        }
    }
    // arrange / paint / hit_test / event / apply_theme / a11y 同理
}

// 用法:
self.ui.value.set_text(self.n.to_string());             // 直接字段,连访问器都不用
```

- ✅ **真正"没有运行时树"**:节点是 struct 字段,内存布局编译期决定;没有 arena、没有节点分配、没有 `dyn`/`Any`/反射;子节点表/焦点顺序/命中顺序/a11y id 全是生成的 const 或代码;静态分派;结构零堆分配(只有 `Vec`/字符串例外),`no_std + alloc` 友好;API 是直接字段,比 B 更好用。
- ❌ **树形编译期固定**:运行时只能改值、改同构 `Vec` 行、`Option` 显示/隐藏;不能运行时插入异构节点;异构列表要预先用 enum 枚举;宏要生成结构体 + 各 pass 驱动 + const 表,是全项目最大的工程量;`Ui<M>` 是一整块大 struct(1k 节点约百 KB,启动分配一次);编译期代码量 O(节点 × pass);组件是不透明字段(内部结构不参与宏展开)。
- ⚠ **仍需要运行时数据**:值、字符串、列表、focus/hover/scroll、布局数值、文本缓存、节点地址 `Node`(静态类型、运行时值)。"零运行期数据"对交互式 GUI 不可能;能去掉的是**堆节点图、动态分派、反射、运行期建树**。

和 C 的区别:C 把树编进**类型**,换来"零节点地址/零 match",代价是类型爆炸和不可控的编译期;C′ 把树编进**生成的代码与 const**,只保留一个 `Copy` 的节点地址枚举。两者都是"编译期树",但 C′ 的工程风险低一个量级——**如果要"完全极致",应该做 C′,不是 C**。

### 3.4 形态 D:静态描述 + 代码生成(duilib 的 XML→Code / Slint 式)

```text
ui.xml / .ui  ──build.rs──▶  生成的 Rust 代码  ──▶  与 B 相同的静态树
```

- ✅ 设计器友好、运行期零解析、可序列化、可 diff/评审;与本项目的"静态"气质最合。
- ❌ 需要 DSL 或 XML 方言、生成器、错误体验;不适合 v1 手写 API 的核心。
- **定位:后续前端**,生成 B 的 Rust 代码;这也让"设计器/热重载"不需要运行时解析。FerroUI 的 XAML 层约 88k 行,静态版应该用 1-2k 行代码生成器实现同样的"设计时格式"。

### 3.5 决策矩阵

| 维度 | A 句柄 | **B 类型视图** | C 类型级树 | **C′ 扁平静态树** | D 代码生成 |
|---|---|---|---|---|---|
| 静态类型手感 | 中 | **高** | 高 | **最高**(直接字段) | 高 |
| Rust 借用友好 | 高 | **高** | 高 | **高** | 高 |
| 动态增删/异构 | **高** | 中 | 低 | 低-中(仅 Vec/Option) | 中 |
| 宏工程量 | 低 | **中高** | 高 | **最高** | 高(需 DSL) |
| 编译期压力 | 低 | **低** | 高(类型膨胀) | 中(O(节点×pass) 代码) | 低 |
| "无运行时树"纯度 | 低 | 低 | 高 | **最高** | 低 |
| 可实现性(v1) | 高 | **高** | 中低 | 中(先 spike) | 中 |
| 与 egui/iced 区分度 | 中 | **高** | 最高 | **最高** | 高 |
| **结论** | 内部结构 | **稳妥主形态 / 退路** | 不推荐 | **极致主形态(目标选它)** | 后续前端 |

**怎么选**:目标是"能长期维护的应用框架、风险最小"→ **B**(arena 保留树 + 类型化访问器,就是本报告 §4 的设计);目标是"完全极致、和别人都不一样、能讲清楚'没有运行时树'"→ **C′**(扁平静态树),接受宏工程量与树形编译期固定;两者共用渲染/布局/文本/窗口栈,先在 spike 里验证 C′ 的编译期与宏成本,再定。C 只作为"类型级极限"的记录,不推荐实现。

---

## 4. 推荐设计:静态类型视图 + 保留树 + 根消息

> 以下代码都是 **API 草案**,不是可编译实现;写出来是为了把形态固定下来。
> §4 以 **B(arena 保留树)** 为主线写;**C′ 与它的差别只在"树存在哪、怎么分派"**(§3.3b):布局算法、事件模型、消息、主题、文本输入、渲染栈全部共用;把 §4 里的"访问器 `ui.value()`"换成"直接字段 `ui.value`"、把 arena 换成宏生成的扁平 struct + 静态驱动,就是 C′。

### 4.0 十条设计原则(判断"这个 API 好不好"的标尺)

1. **树只构建一次**;之后只有 setter 和显式的结构操作,永远没有 `view()`、没有 diff、没有重建(整树重建只作为"换肤/热重载"的显式选择)。
2. **用户 API 里不出现 id、`Any`、downcast、字符串键**;只有具体类型和生成的方法。
3. **消息到唯一根**:业务状态在 `App`,`App::update(&mut self, msg)` 是唯一同时拥有状态和树的地方;控件的交互状态(悬停/按下/滚动/光标)留在控件里。
4. **失效分级但无失效图**:setter 自己标"重绘/重排/子节点变化";框架出帧时全量遍历 + 节点 revision 早退。静态 UI 的节点量小,遍历比维护依赖图便宜且可靠。
5. **空闲零帧**:没有事件、没有显式 `request_redraw`,就不出帧。
6. **主题在构建期解析**:主题是纯数据;没有 selector/级联/优先级;覆盖是显式的。
7. **扩展点只有一个**:`View` trait;动态分派只作为显式逃生口(`Dyn`)。
8. **"动态"只允许出现在两处**:值的变化、显式的结构操作;不允许出现在类型、分派方式和构建流程里。
9. **可测试性是一等公民**:headless harness、消息断言、像素快照、inspector。
10. **把静态的代价写进文档**:忘记同步、结构变化、无动画、主题切换、文本输入,都要在 API 层给出明确答案,而不是留给用户踩坑。

### 4.1 全景

```rust
// 应用 = 业务状态 + 视图(树) + 消息处理
struct Counter { n: i32, ui: Ui }

fn main() -> ferritic::Result<()> {
    ferritic::launch(Counter::new)          // launch: 建窗口、建 GPU、进入事件循环
}

impl Counter {
    fn new(theme: &Theme) -> Self {
        Self { n: 0, ui: Ui::new(theme) }
    }
}

impl App for Counter {
    type Msg = Msg;
    type Ui  = Ui;
    fn update(&mut self, msg: Msg, cx: &mut Ctx) { /* 见 §4.2 */ }
    fn view(&self) -> &Ui { &self.ui }
    fn view_mut(&mut self) -> &mut Ui { &mut self.ui }
}
```

分层:

```text
ferritic          facade: launch(), App, Ctx, Theme, re-exports
ferritic-core     树/arena/View trait/布局/事件/命中/Scene/失效/测试 harness
ferritic-macros   ui! 宏(生成 struct Ui + 访问器 + View 粘合)
ferritic-text     parley/swash 封装 + 字形图集 + 文本测量缓存
ferritic-widgets  Button/Label/TextInput/Scroll/List/Grid/...
ferritic-blade    blade-graphics 上的 2D 渲染器(§5)
ferritic-winit    winit 0.30 窗口/输入/IME/剪贴板/DPI 平台层
```

### 4.2 更新模型:消息到唯一根

```rust
enum Msg { Increment, Reset }

impl App for Counter {
    fn update(&mut self, msg: Msg, cx: &mut Ctx) {
        match msg {
            Msg::Increment => self.n += 1,
            Msg::Reset     => self.n = 0,
        }
        self.ui.value().set_text(self.n.to_string());
    }
}
```

要点:

- **消息不携带数据**(默认)。因为树是保留的,`update` 里可以直接读控件:`self.ui.name().text()`。这样消息是 `Copy`/轻量 enum,没有 `Msg::TextChanged(String)` 的克隆和分配——这是相对 iced 的一个实际优势,**也是"保留"带来的直接红利**。
- 需要数据的场景(列表第几行):控件自己记住,`update` 里读 `self.ui.list().clicked_index()`;或消息带 `usize` 载荷,由控件在派发时填(见 §4.8)。
- 处理完消息后框架统一做一次布局/绘制(见 §4.4),**不要求** `update` 手动请求重绘;`update` 之外的异步变化(timer、网络)用 `cx.request_redraw()`。
- `update` 拿 `&mut self`,树和业务状态在同一个 `self` 里,借用是顺序的:派发消息时只借 `view_mut()`,`update` 时只借 `self`,不存在同时借用。

### 4.3 构建:`ui!` 宏生成什么

```rust
ui! {
    msg = Msg;
    pub struct Ui {
        Window(title: "Counter", size: (320, 120)) {
            Column(padding: 12, spacing: 8) {
                value: Label(text: "0"),
                inc:   Button(text: "+",     on_click: Msg::Increment),
            }
        }
    }
}
```

宏展开为(概念上):

```rust
pub struct Ui { tree: Tree<Msg>, /* 每个具名控件的 NodeId */ }

impl Ui {
    pub fn new(theme: &Theme) -> Self { /* 建 arena、按树的顺序建节点、解析主题 */ }
    pub fn value(&mut self) -> WidgetRefMut<'_, Label> { ... }   // 类型化访问器
    pub fn inc(&mut self)   -> WidgetRefMut<'_, Button<Msg>> { ... }
}

impl View for Ui {
    type Msg = Msg;
    fn measure(&mut self, cx: &mut MeasureCx, c: Constraints) -> Size { ... }
    fn arrange(&mut self, cx: &mut ArrangeCx, r: Rect) { ... }
    fn paint(&mut self, cx: &mut PaintCx) { ... }
    fn hit_test(&self, p: Point) -> Option<Path> { ... }
    fn event(&mut self, cx: &mut EventCx, path: &[NodeId], ev: &Event) -> Option<Msg> { ... }
}
```

- 只生成**具名控件**的访问器;匿名控件(纯布局用)不生成。
- 访问器返回 `WidgetRefMut<'_, T>`,实现 `Deref/DerefMut` 到 arena 里的 `T`。这样 `ui.value().set_text(..)`、`ui.value().text()` 都自然,调用结束借用即释放。
- 访问器只生成 `&mut self` 版本(应用侧总是在 `update` 里改);只读访问通过 `&*` 或 `ui.value().text()` 完成。内部 `&self` 遍历由 `View` 实现直接走 arena。
- `msg = Msg` 需要显式给出:过程宏不做类型推断,与其生成不可读的 `Ui<_>` 泛型,不如把类型写在最前面。以后可以用 `#[ferritic::app]` 属性宏收掉这个参数(§4.11)。

**构建顺序 & 主题**:`Ui::new(theme)` 在构建时把主题解析成每个控件的默认值(颜色/字体/圆角/间距)。这不是样式系统:没有选择器、没有级联、没有优先级,只有"构造参数 + 主题默认值"。(§4.7)

### 4.4 帧循环与失效:三级、全量遍历、零空闲

```text
winit event
  ├─ 输入事件 → View::event(path, ev) ──► Option<Msg>
  │                                        └─ Some(msg) → app.update(msg, cx)
  ├─ 若无消息但控件内部状态变了(悬停/按下/滚动) → 标记需要重绘
  └─ 若需要出帧:
       布局(整树重算 / taffy compute_layout;文本测量走缓存)
        paint(把绘制命令写进 Scene)
        render(blade,只在有变化时)
```

- **不需要失效传播图**。setter 只改自己节点的 revision;框架在出帧时全量重算布局(静态 UI 的节点数一般在百级,成本可以忽略),真正贵的是文本整形,而它被缓存挡住。若用 taffy,`compute_layout` 本身在无改动时也可以跳过。
- **三级失效仍存在,但只在节点内部**:`set_text` → 重排+重绘;`set_color` → 只重绘;`list.set_rows` → 子节点变化。这不是为了让框架少走几步,而是为了让**缓存**能早退(尤其是文本)。
- **空闲零帧**:没有输入、没有 `cx.request_redraw()`,事件循环 `Wait`,不出帧。这直接对应"静态"的产品价值:工具类应用挂后台时不烧电。
- **不做 damage/局部重绘 v1**:每次出帧全窗口重画(整帧只有几条 draw call)。保留接口,以后可以按脏矩形做 scissor 局部重绘(swapchain 需要保留上一帧,见 §5.6)。

### 4.5 事件模型:三层

1. **输入事件层**(`PointerMoved/Pressed/Released/Wheel/Key/Ime/Text`):框架做命中测试,得到从根到目标节点的路径,沿路径派发。控件自己维护 hover/pressed/focus/scroll 等交互状态,必要时只请求重绘。
2. **语义层**:控件把交互翻译成消息:`Button` 被点击 → `Msg::...`;`TextInput` 文本变化 → `Msg::...`;`Slider` 拖动 → `Msg::...`。消息沿 `View::event` 的返回值上传到根。
3. **应用层**:`App::update` 消费消息,改业务状态,通过访问器改控件值。

为什么不用闭包:

- `Box<dyn FnMut(&mut App)>` 需要把 `App` 塞进树 → 自引用/`Rc<RefCell<App>>`/借用 panic,是手写 Rust 保留 GUI 的经典坟场。
- 消息 enum 可 `Clone`、可日志、可测试(测试 harness 可以断言"点了按钮产生了什么消息")、可序列化;静态分发、无分配。
- 需要"局部行为"时,控件内部状态已经处理了(滚动、悬停),不需要闭包。

**焦点/键盘**:`EventCx` 持有 focus id、hover id、mouse 位置、修饰键、IME 状态;控件用 `cx.focus()`/`cx.is_focused()`;Tab 顺序来自树顺序(宏可生成 `next_focus`),这是静态树的一个红利——焦点顺序就是结构顺序,不需要注册表。

### 4.6 布局:taffy(flex/grid)+ 文本测量,或自写 measure/arrange

布局是"买还是造"的分水岭。调研结论(§5 的详细笔记):**v1 建议买——用 taffy 0.15 做 flex/grid 布局,自己只包一层类型化 API 和文本测量**;自写 measure/arrange 只在"完全固定的控件集 + 想要 Avalonia 式语义"时才值得。

```rust
// 用户 API 仍然是静态的、声明式的;布局后端藏在 Layout trait 后面
ui! {
    msg = Msg;
    pub struct Ui {
        Window(title: "Counter") {
            Column(spacing: 8, padding: 12, align: Align::Center) {
                value: Label(text: "0"),
                inc:   Button(text: "+", on_click: Msg::Increment),
            }
        }
    }
}
```

- **taffy 0.15**(MIT,零依赖):flexbox + grid + block。`TaffyTree::new()`、`new_with_children(Style, &[NodeId])`、`compute_layout(root, AvailableSpace)`、`tree.layout(node)`。把 `ui!` 里的 `Row/Column/Stack/Grid/Padding/Align/SizedBox` 映射成 `taffy::Style`;每个控件节点持一个 taffy 节点 id。
- **文本测量是唯一硬缝**:taffy 需要 min-content / max-content 尺寸,从 `ferritic-text` 的缓存里取——同一段落分别用 `break_all_lines(None)`(max-content)和最小宽度断行,缓存 `(text, style, available_width, dpi) -> Size`。绝不在命中测试或绘制时重新整形。
- **自写 measure/arrange 的备选**:参考 Avalonia/WPF 的算法(FerroUI `layoutable.rs` + `layout_manager.rs` 约 1.5k 行真实逻辑,可读后重写),语义是"固定 / 适应内容 / 拉伸 + min/max + 对齐"。好处是 API 更贴合静态 UI(没有 CSS flex 的怪异之处),坏处是你要自己维护 flex/grid 语义。
- **缓存**:每个节点存 `(last_style_revision, last_measured_size, last_rect)`;setter 递增 revision。静态 UI 的布局遍历本来就是 O(n) 便宜操作,taffy 的 `compute_layout` 在无改动时也可以跳过。
- 不做:inline flow / float / 百分比魔法 / 媒体查询 / 容器查询——静态布局用固定尺寸、间距、对齐、stretch 和 grid 轨道足够。

### 4.7 样式与主题:构建期解析,无选择器

```rust
pub struct Theme {
    pub palette: Palette,     // 纯数据表(参考 Fluent 的 color_palette_resources)
    pub fonts: FontSet,       // 字体族/字号层级
    pub metrics: Metrics,     // 间距/圆角/控件度量
}

Label::new("hi")                     // 构造时从 theme 取默认值
Button::new("OK").primary()          // 显式覆盖
```

- **没有** selector、cascade、pseudo-class、样式帧、`!important`。`Button` 的按下/悬停外观由控件内部按状态直接查主题(静态数据表),不是样式匹配。
- 主题切换(`ui.set_theme(&theme)`):宏生成的遍历把"没有显式覆盖"的属性重设为新主题值;显式覆盖保留。控件内部状态(文本、滚动、焦点)不丢。
- 更粗暴也合法的做法:整树重建(`self.ui = Ui::new(&new_theme)`)。构建便宜是静态设计的红利,代价是控件状态丢失——文档里写清楚适用场景(换肤、热重载、设计器预览)。
- **不做动画**:静态语义下,过渡就是"多帧 + 显式插值"。如果以后要,提供逐帧 hook 而不是内建 transition 系统(避免又长出一个 Avalonia)。

### 4.8 集合、条件与结构变化

- **条件显示**:`Option<C>` 作为子节点(固定类型),用 `ui.foo().set_visible(false)` 控制;真正要在树里增删节点时走集合 API。
- **列表**:`List<T>`(容器)持有 `Vec<T>`;`T` 是该行控件的具体类型(同构行,静态分派)。`List` 负责滚动、可见行裁剪、行高缓存。
  - 行内事件:行控件在构造时带 `Msg::Row(i)`(i 是行号)或由 `List` 在派发时把 `i` 填进 `Msg` 的载荷;行号语义由 `List` 维护(`set_rows` 之后重新编号)。
  - 结构变化:`list.set_rows(rows)`、`list.insert(i, row)`、`list.remove(i)`——显式,且**整表重排**(静态库不做 keyed diff;需要增量时用键控插入 API,内部按 key 找位置)。
  - 虚拟化:静态库的定位是"万级以内";真正的百万行表格后续用"数据 + 行工厂"的 `Table`(行工厂是 `fn` 指针,不是存储的闭包)。
- **异构内容**:默认可选**装箱逃生口** `Dyn(Box<dyn View<Msg = M>>)`,只在用户显式选择时付出动态分派;核心路径仍然全静态。
- **不可变结构**:类型级树(C)在结构变化上会强制"整树类型改变";B 形态用 arena 就没有这个问题——这是选 B 作 v1 的主要工程原因之一。

### 4.9 "忘记同步"的补偿

静态库失去 iced 的"view 是纯函数"保证,用四样东西补:

1. **测试 harness**:`harness.click(ui.inc())` 会派发事件并断言消息;快照测试比对渲染出的 PNG。
2. **debug 断言**:`update` 返回后,框架在 debug 下检查"是否有控件被改了却没标脏"?做不到完全检测,但可以检查"scene 与 widget 状态不一致"?成本高,不做;改为**约定 + inspector**。
3. **inspector**:`F12` 打印/高亮整棵树(结构、字段值、布局结果、最近消息),这是静态树最容易做的调试工具。
4. **`sync` 逃生口**:每个 screen 可以有一个 `fn sync(&mut self)`(或 `fn sync(&mut self, data: &Data)`),在数据变化时被调用,把状态整体刷进控件;它就是"静态库版的 `view()`,没有 diff"。这给不想写散装 setter 的应用一条中间路。

### 4.10 文本输入(单独 crate,第一等公民)

- 显示:parley 整形/换行/对齐/双向 + swash 光栅化进图集(§5.4)。
- 编辑:编辑模型(`String` + 光标/选区/预编辑)、点击定位、拖选、双击选词、键盘(退格/删除/方向/Home/End/剪贴板)、IME(`winit` 的 `Ime` 事件,预编辑下划线)。
- 这是全项目**最大的单点工程**,建议单独 `ferritic-textedit` 思路,并且 v1 只承诺"单行/简单多行 + 常用键 + 一种 IME 路径",双向文本/复杂脚本后续再做。
- 备选:接入现成的编辑器内核(如 `cosmic-text`/`parley` 自带的编辑辅助,或第三方 `text` 编辑 crate),但要评估 API/维护性。

### 4.11 自定义控件与逃生口

```rust
// 1) 组合:普通 struct 实现 View(内部可以有泛型子节点)
pub struct Card<C: View> { body: C, padding: f32 }
impl<C: View> View for Card<C> { type Msg = C::Msg; ... }

// 2) 在 ui! 里使用:card: Card(...) —— 宏按普通构造器处理

// 3) 动态内容:Dyn(Box<dyn View<Msg = M>>) —— 显式选择才付出动态分派

// 4) 底层绘制:自定义 paint 直接往 Scene 里写 Prim
```

- `View` trait 是唯一扩展点;它按"趟"拆方法(measure/arrange/paint/hit_test/event),与 Masonry 一致,避免一个巨型 `update`。
- 自定义控件默认不生成访问器(宏不展开其内部);它自己的方法就是它的 API。

### 4.12 后续:静态描述前端(D)

- `ui.xml`/`.ui` / 设计器文件 → `build.rs` 生成 `ui!` 等价的 Rust 代码。运行期零解析、零反射。
- 这也天然支持"设计器预览"和"整树重建式热重载":改的是源文件,重新编译或重新生成,树重建;不需要 Avalonia 的 XAML 运行时。
- FerroUI 的 XAML 移植(~88k 行)是"运行期解析 + 反射"的路;FerriticUI 应该走"编译期生成"的路,量级小两个数量级。

### 4.13 完整一点的例子:Todo(表单 + 列表 + 行内事件)

```rust
enum Msg { Add, Toggle, Delete, ClearDone }

struct Todo { items: Vec<Item>, ui: Ui }
struct Item { text: String, done: bool }

ui! {
    msg = Msg;
    pub struct Ui {
        Window(title: "Todo", size: (420, 520)) {
            Column(padding: 16, spacing: 8) {
                row: Row(spacing: 8) {
                    entry: TextInput(placeholder: "what to do?", on_submit: Msg::Add),
                    add:   Button(text: "Add", on_click: Msg::Add),
                },
                list:  List(Row),                    // 同构行 → 静态分派
                footer: Row(spacing: 8) {
                    count: Label(text: "0 items"),
                    clear: Button(text: "Clear done", on_click: Msg::ClearDone),
                }
            }
        }
    }
}

// 行控件就是普通控件组合,不是"模板/组件系统"
struct Row { check: Checkbox, text: Label, del: Button<Msg> }

impl Todo {
    fn push_row(&mut self, text: &str) {
        let row = Row {
            check: Checkbox::new(false, Msg::Toggle),      // 消息不带行号
            text:  Label::new(text),
            del:   Button::new("×", Msg::Delete),
        };
        self.ui.list().push(row);
    }
}

impl App for Todo {
    type Msg = Msg;
    type Ui  = Ui;
    fn update(&mut self, msg: Msg, _cx: &mut Ctx) {
        match msg {
            Msg::Add => {
                let text = self.ui.entry().text().trim().to_owned();
                if text.is_empty() { return; }
                self.push_row(&text);
                self.items.push(Item { text, done: false });
                self.ui.entry().set_text("");
            }
            // 行号不放进消息里:静态树里控件自己记着行号,update 里读
            Msg::Toggle => {
                let i = self.ui.list().clicked_index();
                self.items[i].done = !self.items[i].done;
            }
            Msg::Delete => {
                let i = self.ui.list().clicked_index();
                self.items.remove(i);
                self.ui.list().remove(i);                  // 显式结构操作
            }
            Msg::ClearDone => { /* 重建 list 的行:list.set_rows(..) */ }
        }
        self.ui.count().set_text(format!("{} items", self.items.len()));
    }
    fn view(&self) -> &Ui { &self.ui }
    fn view_mut(&mut self) -> &mut Ui { &mut self.ui }
}
```

这个例子暴露了静态设计的取舍,值得逐条看:

- `entry().text()` 直接读控件,所以 `Msg::Add` 不需要带 `String`;`clicked_index()` 直接读列表,所以行号不需要进消息。**状态留在控件里,是静态设计的核心红利。**
- 结构变化是显式的:`push` / `remove` / `set_rows`。没有 diff;行控件由应用自己构造,`Row` 是普通 struct。
- 滚动、悬停、按下由 `List`/`Row` 内部处理,不进消息、不惊动应用;应用只在"语义事件"上被叫醒。
- 如果偏好"行号载荷"风格,也可以让 `List` 在派发时把当前行号填进消息;两种都行,但要统一约定,别混用。

### 4.14 这个设计明确**不**做什么

- 不做 XAML/DSL 运行时解析(要设计时格式,走 §4.12 的代码生成)。
- 不做样式表/选择器/伪类/级联/优先级。
- 不做数据绑定、响应式信号、依赖图。
- 不做内建动画/过渡(留逐帧 hook)。
- 不做合成器/服务端视觉/多线程渲染(v1 单线程渲染)。
- 不做运行时反射、类型注册、属性元数据。
- 不做"任意类型都可当控件"的插件系统(只有 `View` trait + 可选 `Dyn`)。

---

## 5. 渲染后端:blade-graphics 上的 2D UI 渲染器

### 5.1 blade-graphics 的能力边界

`blade-graphics` 是 GPU HAL:**它本身没有任何 2D 绘制**。已核对 `main`(revision `30c94eb`)的源码:

| 有 | 没有 |
|---|---|
| `Context`(instance/adapter/device/queue)、`Context::init` | 路径填充/描边 |
| `Surface`(swapchain)、`acquire_frame`、`present`、**`present_with_damage`** | 抗锯齿(MSAA 之外的任何 AA) |
| `Buffer`/`Texture`/`TextureView`/`Sampler`、`upload_buffer` | 文本/字形/字体 |
| WGSL shader(naga 前端)、`ShaderData`(按字段名绑定) | 裁剪栈、层、圆角、渐变、透明度组 |
| `RenderPipelineDesc`(含 `instanced` fetch)、`VertexLayout`(按字段名) | 合成器、混合模式(只有固定混合状态) |
| `RenderPipelineEncoder`:`bind_vertex`、`draw(first_vertex, vertex_count, first_instance, instance_count)`、`draw_indexed(..)`、`set_scissor_rect`、`set_viewport` | UI 控件/布局/事件 |
| `CommandEncoder`:`start`/`present`/`wait_for`、pass 之间自动 barrier;`InitOp::{Load, Clear, DontCare}` / `FinishOp::{Store, Discard, ResolveTo}`(显式 MSAA resolve 与 load/store 省略) | 字形图集/2D 批处理 |
| `ContextDesc` 的能力查询:`sample_count_mask`、`dual_source_blending`、`compute`、`indirect_draw`、`binding_array`、`timing`;`DisplaySync::{Block, Recent, Tear}` + damage rects | — |
| `blade-util::BufferBelt`(帧内分配器)、readback(`copy_texture_to_buffer`) | — |

FerroUI 的 Blade 后端文档把同一件事说得很清楚:"`blade-graphics` 是低层 GPU 抽象:没有路径渲染、字形渲染、裁剪器、层栈、渐变,或固定功能混合状态之外的任何混合语义。绘制契约要求的一切都由本 crate 在它之上绘制。"(`FerroUI/docs/porting/blade-backend.md` §1.2)

`blade-egui`(约 420 行)是"外部 UI 渲染器接入 blade"的参考实现,值得逐行读:一个 `Globals` uniform + 一个 `Locals`(纹理/采样器)绑定组;顶点格式与 `egui::epaint::Vertex` 编译期等大;预乘混合(`color: One/OneMinusSrcAlpha`,`alpha: OneMinusDstAlpha/One`);两个 `BufferBelt` 上传顶点/索引,提交后 `flush(sync_point)`、每帧 `trim`;纹理经 **vertex belt** 中转后 `copy_buffer_to_texture`;纹理销毁用 `Vec<(GuiTexture, SyncPoint)>` 延迟到 `wait_for(&sp, 0)` 确认后再做;每个 mesh 一次 draw(一次 scissor + 一次纹理绑定);`present` 后 `wait_for(上一帧)` 才复用 belt。它的局限也正是新渲染器要补的:每 draw 一次纹理绑定、只有矩形 scissor、没有圆角裁剪、没有 MSAA、没有 damage、没有 stencil。

### 5.2 三条路线

| 路线 | 做法 | 优点 | 缺点 |
|---|---|---|---|
| **R1 复用 FerroUI.Blade** | 直接移植/依赖它的 `gpu/`(lyon tessellation + 轮廓字形 + MSAA + 逐层提交) | 已跑通、有测试、渐变/clip/layer 都有;~1.5k 行干净代码 | 文本走"逐字形轮廓 tessellate",大段文本 CPU 成本高、无亚像素;层/蒙版每层 submit+wait;MSAA 依赖设备;它是为 Avalonia 契约长的 |
| **R2 新写 UI 原语渲染器(推荐)** | 实例化 SDF 四边形 + 字形覆盖图集 + 纹理四边形;单 pass、少 draw call、无 MSAA | 快、质量稳定、所有 blade 后端一致、代码量可控(~3-5k 行) | 任意曲线路径(图标/自绘)要另想办法(预光栅化或 lyon 兜底) |
| **R3 外部 2D 光栅器** | vello(CPU/GPU)或 resvg 光栅化到纹理,再 blit 到 blade surface | 矢量质量最高、支持任意 path | 与 blade 的 GPU 抽象重复(vello 自带 GPU 抽象)、CPU 模式带宽/延迟;违背"基于 blade"的初衷 |

**推荐:R2 为主,R1 的零件复用,R3 只用于"图标/SVG 的预光栅化"**(构建期/启动期用 resvg 渲进图集,运行期仍是纹理四边形,不是每帧光栅化)。

R1 里值得直接拿的零件:

- **渐变 LUT**:FerroUI 把渐变的 stops 采样成 256 项纹理,片元按像素位置采样;线性/径向/扫描三种共用一个 shader(`FerroUI.Blade/gpu/renderer.rs` 的 `GradientParams`)。
- **readback + 像素对比 harness**:渲染到离屏 target → `copy_texture_to_buffer` → PNG,做快照/回归(见 §5.7)。
- **swapchain 的时序模式**:`acquire_frame` → `start` → pass → `present` → `wait_for(前帧)` 后再销毁资源。
- **MSAA target 的写法**(如果 R2 将来需要给路径兜底)。

### 5.3 R2 渲染器设计

**图元与管线**

| 管线 | 输入 | 实现 |
|---|---|---|
| `Quad`(实心/圆角/边框/阴影) | 每图元:矩形、四角半径、颜色(预乘)、边框宽/色 | 顶点是单位四边形;片元用圆角矩形 SDF,`fwidth` 做解析 AA;边框 = `abs(sdf) - width/2` 的覆盖率;软阴影用偏移 SDF 一次画完 |
| `TexQuad`(纹理) | 矩形、UV 矩形、tint、纹理 | 字形图集(A8)与图片/emoji(RGBA8)共用;按纹理分批;tint 预乘 |
| `Gradient`(可选) | 矩形、类型(线性/径向/扫描)、stops LUT、变换 | 复用 R1 的 LUT 思路;静态 UI 的主题色大多不需要,可延后 |
| `Composite`(可选) | 离屏纹理、目标矩形、圆角、透明度 | 圆角裁剪/透明度组/模糊阴影;v2 |

实现上更省事的做法(调研建议):**一条管线 + 每实例 `prim_kind` 分支**——实例记录携带 `rect, radii, colors, uv, clip_id, prim_kind`,顶点着色器展开单位四边形,片元按 `prim_kind` 走 SDF / 采样 / 渐变。这样"按管线排序"退化成"按 (layer, pipeline, atlas, clip) 排序",draw call 更少。

**批处理与帧结构**

```
acquire_frame → encoder.start()
  render pass(swapchain texture)
    bind globals(screen size, dpi, time 可选)
    for run in scene.runs:            // 按 (layer, pipeline, atlas, clip) 排序后的连续段
        bind pipeline + vertex/instance buffer(BufferBelt 分配)
        set_scissor_rect(只在 clip 变化时)
        draw(0, 6, first_instance, instance_count)  /  draw_indexed(..)
  present(frame) 或 present_with_damage(frame, &dirty_rects)
  wait_for(prev_sync_point)          // 再销毁上一帧资源
```

- 静态 UI 的典型帧:**一次 submit、3-6 条 draw call**(实心、字形图集、图片、可选层),没有逐控件 draw call。
- 顶点/实例数据每帧从 `BufferBelt` 分配(不自己管环缓冲);`CommandEncoderDesc { buffer_count: 2 }`;一个静态四边形索引缓冲 + 每帧一次的实例缓冲 `alloc_pod`。
- **裁剪**:轴对齐矩形一律 `set_scissor_rect`(免费);圆角/路径裁剪才走离屏层。**层纹理按 clip 的 bounds 分配**(不是窗口大小),`InitOp::DontCare` + scissor 代替整层 clear,按内容 revision 缓存,只在失效时重画——"每张圆角卡片一个窗口大小的层"是毁掉 UI 帧预算的经典做法。
- **颜色/混合**:预乘 sRGB,`SurfaceConfig { color_space: ColorSpace::Srgb }`,与 FerroUI Blade 后端对齐("未标记 sRGB、预乘、与 Skia 的 untagged N32 一致")。注意:覆盖率是线性的、目标是 sRGB 编码时,`One / OneMinusSrcAlpha` 直接混进 sRGB 会让文字边缘偏暗——要么渲进线性 target 再配 sRGB surface format(`Rgba8UnormSrgb`),要么给覆盖率加一条固定的对比度曲线(不要用启发式)。
- **DPI**:布局用逻辑像素,字形按 `px * scale_factor` 光栅化,顶点阶段乘 `scale_factor` 得到物理像素;SDF 在物理像素空间做 AA;`ScaleFactorChanged` 时清图集并重排。分数缩放也能保持清晰。
- **抗锯齿**:SDF 解析 AA + 字形覆盖 AA + 图片双线性,**不依赖 MSAA**——这是相对 R1 的关键收益:所有后端一致、没有 resolve、没有 2x/4x 带宽。
- **MSAA 的定位**:只给"lyon tessellate 出来的任意路径/图标"兜底(`TextureDesc { sample_count: 4 }` + `FinishOp::ResolveTo`),不进 UI 主路径。
- **任意路径**:图标默认"构建期/启动期 resvg → 图集"或离线 MSDF;需要运行时矢量时单独用 lyon tessellate 走 MSAA 兜底(不在 v1 主路径)。

### 5.4 文本栈

流水线:**itemize → shape → layout → rasterize → atlas → quads**。

- **整形**:**parley 0.12**(整形引擎是 `harfrust` 0.15 —— HarfBuzz 的 Rust 移植,cosmic-text 0.19 也用;`rustybuzz` 已基本停更)。`LayoutContext::ranged_builder(...)` → `build` → `break_all_lines(Some(max_advance))` → `set_alignment(..)`,再遍历 `layout.lines()` 与 `PositionedLayoutItem::{GlyphRun, InlineBox}` 取 `positioned_glyphs()`。**用 `quantize = true`**:parley 文档明确说像素量化坐标是最省事的防模糊手段。字体枚举/回退交给 **`fontique` 0.12**(系统枚举、通用族、按 script/locale 回退、共享 mmap 加载),不要手写回退。
- **光栅化**:**swash 0.2.10** 是成熟选择:`ScaleContext` → `builder(FontRef).size(px).hint(true)` → `Render::new(&[Source::Outline, Source::ColorOutline(0), Source::ColorBitmap(..)]).format(Format::Alpha).offset(x, y).render(..)`。swash 带 hinting(1x 屏 11–13px 的小字必需)、亚像素偏移、COLR/CPAL + CBDT/sbix emoji。备选:`skrifa` 0.49 轮廓 + `vello_cpu` 的 `fill_glyphs`(parley 自带该示例),质量更高但没 hinting,更适合 2x DPI。
- **图集**:文字用 **A8**,彩色 emoji 才用 RGBA8。key = `(font_id, glyph_id, size_q, subpixel_x ∈ 0..4, subpixel_y ∈ 0..4, hint, format)`:4 个 x 相位保证分数间距正确(只量化整像素会在大段文字里出现间距不均)。图集分配用 `etagere`/`guillotiere`,或自己写 150 行的 shelf 分配器;满页新建一页 + LRU。
- **为什么 FerroUI.Blade 的"轮廓 tessellate"对 UI 文本很贵**:一个 hint 过的 14px 字形经 lyon 变成 200–400 个顶点(约 5–10 KB),图集方案是 4 个顶点、约 96 字节;60 个字形的行 = ~500 KB 顶点流量 + 每帧每字形 10–50µs 的重新 tessellate,图集是 ~6 KB 且 CPU 零成本。而且轮廓路径没有 hinting、没有亚像素定位控制、只能靠 MSAA(小字偏糊)、每次换字号/DPI 都要重算、无法彩色 emoji。轮廓只适合大号展示文字/logo/任意变换文字。
- **测量缓存**:`ferritic-text` 提供 `ShapedParagraph`,key = (UTF-8 文本 + 字体/字号/字重 + 最大宽度 + dpi);`Label` 的布局只查缓存,**绝不在命中测试或绘制时重新整形**。
- **DPI/主题切换**:清图集 + 重排;`ScaleFactorChanged` 时 `px * scale_factor` 重新光栅化。
- **编辑**:见 §4.10;`parley` 的 selection/cursor 几何可以复用,但编辑模型(撤销、输入法、剪贴板、双击选词)要自己写。
- 中文/CJK 是本地用户的第一场景:**必须在 M1 就测 CJK 字体的回退、全角标点、断行**;不要留到最后。CI 里自带字体文件(不依赖系统字体),保证可重复。

### 5.5 窗口与帧循环(winit 0.30)

- `winit::application::ApplicationHandler`(`resumed`/`window_event`/`about_to_wait`/`suspended`)+ `EventLoop::run_app`;`resumed` 里 `event_loop.create_window(..)`;blade `Context::init(ContextDesc { presentation: true, validation: cfg!(debug_assertions), timing: cfg!(debug_assertions), .. })`;`create_surface_configured(&window, SurfaceConfig { size, usage: TextureUsage::TARGET, display_sync: DisplaySync::Recent, color_space: ColorSpace::Srgb, .. })`——`winit::Window` 实现 `raw-window-handle` 0.6 的 `HasWindowHandle + HasDisplayHandle`,blade 直接接受。
- **重绘调度(静态 UI 的关键)**:`event_loop.set_control_flow(ControlFlow::Wait)`;只在状态变化、IME 更新、显式 `request_redraw` 时调 `window.request_redraw()`;在 `RedrawRequested` 里出帧,**不要在 `about_to_wait` 里无条件重绘**(blade 的 bunnymark 那样做是因为它持续动画——正好是静态 UI 的反模式)。需要逐帧动画时用 `ControlFlow::WaitUntil(next_frame)`;提交前 `window.pre_present_notify()`(Wayland/X11 的帧同步)。
- **resize**:`Resized` → 跳过 `width == 0 || height == 0`;否则 `reconfigure_surface` + 重排。`ScaleFactorChanged` → reconfigure + 重排 + 清字形图集。
- **输入**:`CursorMoved/MouseInput/MouseWheel/KeyboardInput/ModifiersChanged/Touch/Focused` 翻译成自己的事件枚举(逻辑像素 = physical / scale_factor),再做保留树命中测试(矩形/圆角距离、逆绘制序、尊重 clip)。
- **IME**:`WindowEvent::Ime(Ime::{Enabled, Preedit, Commit, Disabled})`;聚焦文本框时 `window.set_ime_allowed(true)`,并用 `window.set_ime_cursor_area(pos, size)` 让候选窗落在正确位置。
- **剪贴板**:winit 没有;`arboard` 3.x 覆盖文本/图片,X11 上放 worker 线程。
- **无障碍**:`accesskit` 0.25 + `accesskit_winit` 适配器;从保留树生成 `accesskit::TreeUpdate`(role/name/bounds/focus/actions),变化时推送,把 `ActionRequest` 翻译回自己的事件;预算真实时间——它会反向约束节点身份与失效设计。
- **资源生命周期**:blade 没有 `Drop` 释放 GPU 资源;销毁必须和 `SyncPoint` 对齐(照 `blade-egui` 的 `Vec<(Texture, SyncPoint)>` 延迟销毁模式)。
- 多窗口:v1 单窗口;`launch` 的 API 从第一天就留 `WindowId` 意识,避免以后重写。

### 5.6 依赖清单(版本为调研时刻核对过的最新稳定版,实现时再核对)

| 角色 | 选择(版本) | 许可证 | 决定 |
|---|---|---|---|
| GPU HAL | `blade-graphics` 0.9 + `blade-util` 0.5 + `blade-macros` 0.3 | MIT | **依赖**,`gpu` 模块包一层,**blade 类型绝不进公共 API**;naga 是 pin 的 git fork,需要 `cargo vendor` 保证可复现 |
| 窗口 | `winit` 0.30.13 + `raw-window-handle` 0.6 | Apache-2.0 | 依赖 |
| 几何 | `kurbo` 0.13 / `peniko` 0.6 + `lyon` 1.0 | MIT OR Apache-2.0 | 依赖(路径/图标/描边;UI 主路径用 SDF 不 tessellate) |
| 布局 | `taffy` 0.15.1 | MIT | **依赖**(flex/grid;不要自己写 flexbox) |
| 文本整形/排版 | `parley` 0.12 + `fontique` 0.12(内部 `harfrust` 0.15、`skrifa` 0.49) | MIT OR Apache-2.0 | 依赖,包一层 `text` 模块 |
| 字形光栅化 | `swash` 0.2.10 | MIT OR Apache-2.0 | 依赖(hinting + 彩色字体) |
| CPU 光栅 oracle | `vello_cpu` 0.3 | MIT OR Apache-2.0 | **仅 dev-dependency**(golden 图) |
| 图集分配 | `etagere` 0.3 / `guillotiere` 0.7 | MIT OR Apache-2.0 | 依赖,或自己写 150 行 shelf 分配器 |
| 剪贴板 | `arboard` 3 | MIT OR Apache-2.0 | 可选依赖 |
| 无障碍 | `accesskit` 0.25 + `accesskit_winit` | MIT OR Apache-2.0 | 可选依赖(v2),成对锁版本 |
| 快照 PNG | `png` 0.18 / `image` 0.25 | MIT OR Apache-2.0 | 仅 dev-dependency |
| 日志/剖析 | `log`、`profiling` | MIT OR Apache-2.0 | 可选 |

**买 vs 造**:布局、整形、回退、字形光栅化、窗口——买;渲染器(batching、盒式解析 AA、clip 栈、层缓存、图集 key、damage)——造,因为没有现成的东西会说 blade,而 UI 的质量门槛(1px 边框清晰、12px 文字干净、圆角裁剪正确)全在这一层。

### 5.7 性能模型与风险

- **基线目标**:空闲 0 帧;一个 1k 控件、含文本的静态界面,整帧重绘 CPU < 2ms、GPU 单 pass < 1ms;内存 < 64MB;首帧到可交互 < 150ms。
- **风险 1:draw/state 爆炸**。逐图元绑纹理/设 scissor(`blade-egui` 的模型)会把上限压到每帧几千个图元。缓解:单一图集集合;按 `(layer, pipeline, atlas, clip)` 排序;每帧一个实例缓冲;一条管线按 `prim_kind` 分支;scissor 只在 clip 真变化时设置;用 `ContextDesc::timing` + `Timing::pass_durations()` 盯着。
- **风险 2:AA/阴影/离屏层带来的填充率与 overdraw**。缓解:解析覆盖率代替 MSAA(省一半带宽、免第二趟 UI pass);层纹理按 clip bounds 分配 + `InitOp::DontCare` + scissor;层按内容 revision 缓存;阴影用"一个四边形里的 SDF 模糊近似",不是真模糊层;debug 构建常开 overdraw 热力图。
- **风险 3:文本成本与失效抖动**。逐帧重新整形、resize 清图集、轮廓 tessellate 都会表现为帧尖峰。缓解:按 `(text, style, width, dpi)` 缓存整形结果;A8 图集 + 4 相位亚像素 key + LRU;每帧一次 staging 上传;只在字体/DPI/主题变化时显式失效。**静态库的收益在这里最大**:首帧之后,不滚动、不动画的窗口应该用 damage rects(`present_with_damage`)甚至干脆不 present。
- **风险 4:颜色/混合一致性**。sRGB/预乘、透明窗口、HDR、macOS wide-gamut,需与参考渲染器做像素对比(见 §5.8)。
- **风险 5:blade 后端矩阵**。macOS=Metal、Windows/Linux=Vulkan、Android=Vulkan、Web=WebGL2;D3D12 在 fork 分支。v1 只承诺 macOS(Metal),Windows(Vulkan)作为 M3 目标;`present_with_damage` 在 Vulkan/EGL 上才有真实语义,其他后端忽略,正好符合"先全帧、后局部"的策略。

### 5.8 验证手段

- **像素快照**:照 blade 自己的 `tests/snapshot.rs`——离屏 target(`Rgba8UnormSrgb`,`TARGET|COPY`)→ render → `transfer.copy_texture_to_buffer` → `submit` + `wait_for` → PNG;比较用 **SSIM(8×8 块,阈值 0.95,C1=6.5025,C2=58.5225)** + 逐像素最大差分通道(避免小 AA 差异掩盖真实回归);golden 只认一个后端(CI 用 `lavapipe`、`presentation: false`),其他后端只做 smoke;固定时间/DPI(测 1.0 和 2.0)/字体文件随仓库携带;`BLESS=1` 重新生成 golden。
- **第二 oracle**:同一场景用 `vello_cpu` 渲一张,作为覆盖率质量的参照(不做像素相等断言)。
- **布局/事件单测**:headless `TestHarness`(参照 Masonry 的 `TestHarness`):构造 UI、注入输入、断言消息/焦点/布局矩形。
- **性能基准**:`criterion` + 帧时间统计(blade 的 timing;`last_timing` 需要先 wait 对应 submission)。
- **对比渲染**:同一场景分别用 FerroUI.Blade(R1)和 FerriticUI(R2)渲染,人工/像素对比,用来校准文本和 AA。

---

## 6. 从 FerroUI 能复用/借鉴什么

FerroUI 是 13.9k 个 `.rs` 文件的庞大工程:`FerroUI.Base` 约 435k 行,`FerroUI.Controls` 约 369k 行,XAML 相关约 88k 行,合成器约 87k 行。关键结论:**它的动态层不是可插拔模块,而是地基**——`StyledElement`/`Visual`/`Layoutable`/`InputElement` 都继承自 `FerroObject` 并通过 `&'static StyledProperty<T>` 句柄读写,控件、主题、XAML、`ferroui-view` 全都建立在这上面。所以 FerriticUI 不能"依赖 FerroUI 加一个 static feature",只能**重写核心,复用算法与独立组件**。

| 资产 | 处置 | 说明 |
|---|---|---|
| `FerroUI.Blade/gpu/`(~1.5k 行:renderer/ops/paint/texture/target/shaders) | **复用/改写** | 与 Avalonia 无关,只碰 `kurbo`/`peniko`/blade;唯一耦合是 `TileMode` 枚举 |
| Blade 的 gradient LUT、MSAA target、readback、snapshot 思路 | **借鉴** | 见 §5 |
| Blade 的 swapchain(`window_render_target.rs`) | **重写** | 目前 macOS/`objc2` 专用;新库用 `raw-window-handle` + `blade::Surface` 通用实现 |
| Blade 的 15 个 `*_impl.rs` + 平台接线(~5.4k 行) | **丢弃** | 为 `ferroui_base::platform` 契约而存在 |
| `FerroUI.Base/media/text_formatting/`(~19.6k 行) | **可读后移植** | 已验证**零** `StyledProperty`/`FerroObject` 引用;双向、换行、断行、省略号、对齐齐全;依赖仅 `ITextShaperImpl`。若不想用 parley,这是最近的现成实现 |
| `FerroUI.HarfBuzz`(788 行) | **可复用** | 独立的 shaper 实现 |
| 布局 `layoutable.rs` + `layout_manager.rs` | **只读语义参考,不移植** | v1 用 taffy 0.15 做 flex/grid(§4.6);Avalonia 的 measure/arrange 算法在需要自写布局后端时再借鉴 |
| 输入/焦点/手势(`input/`,~24k 行) | **重写** | 算法可用,但它假设 `InputElement: Interactive: StyledElement` 的视觉树与路由事件 |
| Fluent 主题颜色表 | **提取为 const 表** | 忽略 `compiled_xaml.rs`(43.6k 行生成代码) |
| 属性系统/绑定/样式/动画/`ferroui-view`/XAML/合成器 | **丢弃** | 正是"静态"要去掉的 |
| 控件库(~231k 行,357 文件) | **窄化重写** | 每个控件都与属性系统绑定;只借算法与外观 |

工作量估算(聚焦一个人/小团队):**可用的静态 v1 ≈ 8-12 人周**(核心树+布局+消息+blade 渲染器+窗口+5-8 个控件+测试),其中文本输入单独再加 3-6 周才谈得上"能用"。

---

## 7. 路线图

| 阶段 | 内容 | 出口标准 | 估时 |
|---|---|---|---|
| **M0 骨架** | `ferritic-core`(arena/View/布局/Scene/事件)+`ui!` 最小宏 + `ferritic-blade`(实例化 SDF 四边形)+ `ferritic-winit`(窗口/DPI)+ Label/Button/Rect + 快照 harness | 计数器 demo 跑在 macOS 窗口;SSIM 快照测试通过 | 2-3 周 |
| **M1 文本** | parley 整形/换行 + swash 字形图集 + Label 测量缓存 + 单行 TextInput(光标/选区/常用键) | 7GUIs 的 Counter/Temperature/Timer 完成;CJK 字体回退/断行通过 | 3-4 周 |
| **M2 控件与布局** | taffy 封装的 Row/Column/Grid/Scroll、Checkbox/Slider/List/Image、焦点/Tab、主题 | 7GUIs 全部;CRUD 列表 demo | 3-4 周 |
| **M3 平台与工具** | Windows(Vulkan)/Linux(Vulkan)、IME、剪贴板、inspector、测试 harness、benchmark | 跨平台 demo;性能基线(静态 UI 空闲 0 帧、1k 控件 60fps) | 3-4 周 |
| **M4 可选** | 代码生成前端(D)、多窗口、无障碍(accesskit)、局部重绘、动画 hook | — | 视需求 |

> M0 的核心树可以按 **C′ 或 B** 任一路线实现(渲染/布局/文本/窗口栈相同)。建议先做一个 **3-5 天的 C′ spike**(单窗口、2-3 个控件、1 个列表、焦点/Tab、快照),用数据决定:宏代码量、20/100/300 节点时的干净构建与增量构建时间、故意写错的错误信息可读性、以及"直接字段"API 的实际手感。判定标准见 §10。

---

## 8. 风险与未决问题

1. **宏的工程质量(最高风险)**:`ui!` 要同时生成类型、访问器、遍历、命中、绘制粘合;错误信息不好会毁掉开发体验。缓解:先做最小宏 + 手写对照实现;用 `trybuild` 做 UI 测试;宏只生成"薄"代码,重逻辑放库里。
2. **文本输入**(第二大):编辑模型/IME/选区/双向文本。缓解:v1 明确范围;单行优先;IME 用 winit 事件 + 预编辑;预留 `ferritic-textedit` 独立演进。
3. **借用与访问器**:`WidgetRefMut` 的 `DerefMut` 用法要验证(尤其是同时访问两个控件时的借用冲突)。缓解:访问器返回短生命周期守卫;文档给出"先取值再改"的模式。
4. **渲染质量**:SDF 圆角在 1px 边框/非整数尺寸下的清晰度、sRGB 混合、DPI 分数缩放、彩色 emoji。缓解:参考 Skia/FerroUI 的比较框架做像素对比;MSAA 作为可选后备。
5. **主题切换**:显式覆盖与主题默认值的记录方式。缓解:`Option<T>` 覆盖字段 + `set_theme` 重放;整树重建作为备选。
6. **"静态"定位过窄**:如果用户其实想要表单联动/动画,会被劝退。缓解:提供 `sync` 逃生口和逐帧 hook;明确"动态 UI 用 FerroUI,静态 UI 用 FerriticUI"的分工。
7. **blade 的覆盖**:macOS=Metal、Windows/Linux=Vulkan、Android=Vulkan、Web=WebGL2;D3D12 在 fork 分支。v1 建议 macOS 优先(本地可验证),Windows Vulkan 次之;不在 v1 承诺 D3D12/DX12。

---

## 9. 命名与仓库结构建议

```text
FerriticUI/
├─ crates/
│  ├─ ferritic/            # facade + launch + prelude
│  ├─ ferritic-core/
│  ├─ ferritic-macros/
│  ├─ ferritic-text/
│  ├─ ferritic-widgets/
│  ├─ ferritic-blade/
│  └─ ferritic-winit/
├─ examples/  counter, 7guis, gallery
├─ docs/      design/, research/
└─ tests/     snapshot/, harness/
```

命名:项目名 FerriticUI;"ferritic"是铁素体,和 Ferro(铁)同源,延续 `FerroUI`/`FerriticUI` 的家族感;crate 前缀统一 `ferritic-`。

---

## 10. 未决问题(需要拍板)

- **路线**:**C′ 扁平静态树**(极致:无运行时节点图、直接字段 API、树形编译期固定)还是 **B arena**(稳妥:运行时结构可变、访问器 API)?建议先做 3-5 天 C′ spike,判定标准:① 宏代码量是否可控(≤ 2-3 倍 B);② 100 节点干净构建 < 60s、增量 < 10s;③ 故意写错时错误信息仍可读;④ 焦点/列表/主题重放能生成出来;⑤ 300 节点不触发 rustc 极限。任一硬指标不过 → 回 B。
- "静态"是否接受**运行时 arena**(用户 API 静态、内部保留树)?如果必须"编译期树",v1 范围要砍到很小(§3.3b 的限制清单)。
- **布局**:买 taffy(推荐,快)还是自写 Avalonia 式 measure/arrange(贴合但慢)?taffy 的 CSS flex 语义是否可接受?
- v1 平台范围:macOS only,还是 macOS+Windows(Vulkan)?
- 文本:parley + swash 自建(推荐),还是移植 FerroUI 的 `text_formatting` + HarfBuzz?
- 是否需要 `Dyn` 逃生口(异构内容/插件)?如果需要,它是唯一的动态分派点,要写进设计。
- 消息模型:纯 `Msg` enum,还是同时提供 `#[app]` 宏把方法名绑定成处理器(duilib/Qt 风格糖)?
- 无障碍/IME 是否进 v1,还是 v2?(它们会反向约束节点身份与失效设计,越晚做越贵。)

---

## 11. 参考

- 本仓库的调研笔记:
  - `notes/static-retained-api.md`:立即/保留/Elm/响应式/静态各形态的 API 调研(egui、iced、Slint、GPUI、Masonry、Qt、LVGL、Druid、Xilem…)
  - `notes/ferroui-blade-audit.md`:FerroUI / FerroUI.Blade 的动态机制清单与复用/重写/丢弃决策(带 file:line)
  - `notes/blade-ui-engine-research.md`:blade 上 2D 引擎的渲染/文本/布局/窗口/测试调研(版本、API、性能数字)
- API 形态(笔记里的原始链接):
  - egui `Ui`:https://docs.rs/egui/latest/egui/struct.Ui.html ;egui `Memory`:https://docs.rs/egui/latest/egui/struct.Memory.html
  - iced:https://docs.rs/iced/latest/iced/ ;Dioxus:https://dioxuslabs.com/learn/0.6/ ;Freya:https://github.com/marc2332/freya
  - Slint 生成代码:https://docs.slint.dev/latest/docs/rust/slint/docs/generated_code/struct.SampleComponent
  - Masonry:https://docs.rs/masonry/latest/masonry/ ;容器控件指南:https://github.com/linebender/xilem/blob/main/masonry/src/doc/implementing_container_widget.md
  - Xilem 架构:https://github.com/linebender/xilem/blob/main/xilem/ARCHITECTURE.md
  - Qt Widgets:https://doc.qt.io/qt-6/qtwidgets-index.html ;信号槽:https://doc.qt.io/qt-6/signalsandslots.html
  - LVGL 对象:https://docs.lvgl.io/master/widgets/obj.html ;事件:https://docs.lvgl.io/master/overview/event.html ;样式:https://docs.lvgl.io/master/overview/style.html
  - GPUI:https://docs.rs/gpui/latest/gpui/ ;Vizia:https://docs.rs/vizia/latest/vizia/ ;Floem:https://github.com/lapce/floem ;Druid Lens:https://docs.rs/druid/latest/druid/trait.Lens.html ;Ribir:https://ribir.org/docs/next/concepts/declarative_ui/ ;KAS:https://docs.rs/kas/latest/kas/ ;Makepad:https://github.com/makepad/makepad
- 渲染/文本/布局栈:
  - parley:https://github.com/linebender/parley ;swash:https://github.com/dfrg/swash ;taffy:https://github.com/DioxusLabs/taffy ;lyon:https://github.com/nical/lyon ;skrifa:https://github.com/googlefonts/fontations ;vello_cpu:https://github.com/linebender/vello ;accesskit:https://github.com/AccessKit/accesskit
- 本地 FerroUI:实现与移植账本
  - `FerroUI/docs/porting/blade-backend.md`(Blade 后端的设计、已建/未建、文本走轮廓、层/clip 成本)
  - `FerroUI/src/Blade/FerroUI.Blade/{gpu/,drawing_context_impl.rs,window_render_target.rs}`
  - `FerroUI/src/FerroUI.Base/{styled_property.rs,property_store/,layout/,styling/,data/,animation/,rendering/composition/}`
  - `FerroUI/docs/declarative-ui.md`(`rsx!`/信号/reconciler,即"动态"的上层)
- 本地 blade:https://github.com/kvark/blade (`main`,`blade-graphics` 0.9)
  - `blade-graphics/src/lib.rs`:`Context`/`Surface`/`CommandEncoder`/`RenderPipelineDesc`(`instanced`)/`VertexFetchState`/`ShaderData`
  - `blade-graphics/src/traits.rs`:`RenderPipelineEncoder`(`draw`/`draw_indexed`/`set_scissor_rect`)、`present_with_damage`
  - `blade-egui/src/lib.rs`:外部 UI 渲染器接入 blade 的参考实现(BufferBelt、纹理、预乘混合、延迟销毁、present)
  - `blade-graphics/tests/snapshot.rs`:离屏渲染 + SSIM 快照测试的模板
  - `examples/bunnymark/main.rs`:winit 0.30 + blade surface 的最小应用循环
