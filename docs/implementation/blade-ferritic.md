# vendor 的 blade 分支:`blade-ferritic`

## 结论

`FerriticUI/vendor/blade-main` 已经替换为 **`vendor/blade-ferritic`**。它来自上一层
`../blade` 仓库里新建的长期集成分支 `blade-ferritic`:

- 基于 `main`(`30c94eb`,与 kvark/blade 上游一致);
- 合并 `feat/d3d12-backend`(18 个提交,D3D12 后端 phases 0-5 + DXR,约 8.7k 行,Windows-only);
- 合并 `feat/mit-licensed-deps`(关闭 winit 默认特性,去掉可避免的 Apache-2.0-only `ab_glyph` 链);
- 追加一个整理提交:许可策略文档、把 D3D12 演示图片从仓库根目录移到 `blade-graphics/etc/`。

当前 pin 的 revision:**`4c7b6953cb91fb48b7028209dc6c15a43a323b8a`**(记录在
`vendor/blade-ferritic.rev`)。分支只在本地,没有 push 到 origin。

## 分支拓扑(建议长期保持)

```text
main (30c94eb,镜像上游)
 ├─ feat/d3d12-backend      18 commits:D3D12 设备层/纹理/管线/交换链/DXR(短生命周期)
 ├─ feat/mit-licensed-deps  1 commit:winit 特性裁剪(短生命周期)
 └─ blade-ferritic          main + merge(d3d12) + merge(mit-deps) + 整理提交
```

组织原则:

1. **`blade-ferritic` 是唯一被 vendor 的长期分支**。`feat/*` 保持短生命周期,完成后合入该分支。
2. 上游更新时,先把 `origin/main` 合进 `blade-ferritic`,解决冲突后再跑一次
   `./scripts/update-blade.sh` 重新 vendor。
3. 不直接 vendor 工作区(有未提交改动/切换中的分支);只用 `git archive` 导出的干净树。
4. 需要可复现时,`vendor/blade-ferritic.rev` + 分支名就是版本标识;CI 可用
   `./scripts/update-blade.sh --check` 检查是否过期。

## 分支里的 D3D12 怎么组织

- **后端放在 `blade-graphics/src/d3d12/`**,和 `metal/`、`vulkan/`、`gles/` 同级,用
  `cfg(all(windows, feature = "d3d12"))` 门控;依赖只有 `windows` crate(MIT/Apache-2.0,
  可选、target-gated)。
- **示例**在 `blade-graphics/examples/d3d12-*.rs`,Cargo.toml 里逐个用
  `required-features = ["d3d12"]` 声明,默认构建不会碰 Windows 代码。
- **设计文档**在 `blade-graphics/etc/d3d12.md`;演示截图放在 `blade-graphics/etc/`。
- **不单独拆 crate**:它是 HAL 的原生后端,不是可复用库;等 HAL traits/backend selection
  接入时,按 `metal`/`vulkan` 的同一套方式挂到 `hal` 上即可。现在 `d3d12` 模块是独立设备层,
  **还没有接进 HAL**,所以 FerriticUI 的渲染路径(blade HAL 的 Metal/Vulkan)不会自动用 D3D12。

## FerriticUI 这边怎么接

- 路径依赖改成 `vendor/blade-ferritic/{blade-graphics,blade-macros,blade-util}`;
- `ferritic-blade` 暴露一个透传特性:`features = ["d3d12"] → ["blade-graphics/d3d12"]`,
  仅 Windows 有意义;`cargo check -p ferritic-blade --features d3d12` 在 macOS 上也能通过
  (Windows 模块被 cfg 掉),真正跑起来需要在 Windows 上构建、并先完成 HAL 接入;
- 更新流程:`./scripts/update-blade.sh`(重新导出分支)/ `--check`(比对 pin)。

## 许可证:能做什么、不能做什么

**项目代码**:blade 与 FerriticUI 都是 MIT;分支里新增了 `docs/licensing.md` 记录策略。

**依赖的现实**:严格"只允许 MIT 许可证依赖"**做不到**:

- `winit 0.30.13` 的许可证字段是 `Apache-2.0`(纯 Apache,不是 MIT 双许可);
- `codespan-reporting 0.13.1`(blade-graphics 的着色器诊断依赖)也是 `Apache-2.0`;
- `feat/mit-licensed-deps` 的措辞也是"尽量避免 Apache-2.0-only":它去掉了**可避免**的
  `wayland-csd-adwaita → sctk-adwaita → ab_glyph/owned_ttf_parser` 链,把不可避免的
  Apache-2.0 crate 数量从 15 降到 12。

**当前做法**(已应用到本仓库):

- FerriticUI 的 `winit` 改成和分支相同的
  `default-features = false` + `rwh_06/x11/wayland/wayland-dlopen`;
  代价是 Wayland 窗口没有客户端装饰,macOS/Windows 不受影响;
- 验证:`Cargo.lock` 里 `ab_glyph`、`sctk-adwaita` 已消失(0 条);
- 离线部分审计(260 个 lock 包中缓存里能读到 191 个):只剩
  `winit`、`codespan-reporting` 两个 Apache-2.0-only 的"不可避免"项;
  其余 69 个未缓存包(多为 Linux/Android/Windows 专用)需要在有网络或完整 cargo 缓存时补审。

**建议**:如果目标只是"可以整体按 MIT 分发 + 依赖全部宽松许可",现在的组织就够;
如果要**严格 MIT-only**,只能换掉 `winit`(自写窗口层)和 `codespan-reporting`(换 MIT
诊断库),成本很高,需要单独决策。CI 建议加 `cargo-deny` + `deny.toml`
(allow MIT/Apache-2.0/BSD/ISC/Zlib,Apache-2.0-only 走显式 exception 列表)。

## 本轮验证情况

```text
./scripts/cargo.sh build --workspace --examples     # 通过,vendor 换成 blade-ferritic 后
cargo test -p ferritic                              # 11 tests passed
./scripts/check.sh                                  # 9 goldens exact(渲染结果不变)
cargo check -p ferritic-blade --features d3d12      # macOS 上通过(Windows 代码被 cfg 掉)
./scripts/update-blade.sh --check                   # pin 与 branch 一致
```

**未验证**:Windows 上 D3D12 的实际编译/运行(macOS 上没有 Windows toolchain);
blade 分支自身的 Windows 示例(`d3d12-*`)也没有在这里跑;全量许可证审计需要联网或完整缓存。

## Windows 远程验证记录

在 `steve@192.168.137.1` 的 Windows 机器(`D:\projects-main\FerriticUI`,rustc 1.99.1
msvc)上做了实际构建与 D3D12 验证:

```text
cargo build --workspace --examples                 通过(0 warnings)
cargo test -p ferritic                            11 tests passed
cargo check -p ferritic-blade --features d3d12    通过
cargo test -p blade-graphics --features d3d12 --lib   49 passed(vendor/blade-ferritic)
d3d12-probe / smoke / compute / render / mesh / texture / wgsl / raytracing   全部 OK
```

D3D12 示例需要 DXC:本机 Windows SDK 自带
`C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\dxcompiler.dll`,
设置 `BLADE_DXC_PATH` 指向它即可(compute/render 等在 NVIDIA GTX 1650 与 WARP 上都验证过;
ray tracing 硬件因驱动报 tier 0 跳过,WARP DXR 1.1 路径通过)。

两点环境限制:

- **FerriticUI 的 Vulkan HAL 在这台 Windows 上跑不起来**:`--snapshot` 和窗口都报
  `no supported device found`(Vulkan loader 存在,但当前会话没有可用 Vulkan 设备)。
  所以 GUI 的离屏快照/窗口回归只能在 macOS 上做;这正是需要把 D3D12 接入 HAL 的原因。
- `d3d12-window` 在 SSH 非交互会话里创建 swapchain 失败(`0x887A0022`
  `DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`);改用计划任务在用户的交互桌面会话(session 9)里运行后,
  窗口正常创建并渲染:硬件 GTX 1650,800x600,连续 present 600 帧。截图保存在
  [`docs/images/windows-d3d12-window.png`](../images/windows-d3d12-window.png)。
- **升级驱动后的 GUI 验证(2026-11-08)**:机器升级到 NVIDIA 驱动 621.14 + Vulkan Loader 1.4.357
  之后,Vulkan 设备已支持新特性;但 loader 同时发现旧的 AMD 集显 Vulkan ICD
  (`amdvlk64.dll`,API 1.2.133,2020 年),枚举返回 `VK_ERROR_INCOMPLETE`,所以直接运行仍失败。
  在**非提权**的交互会话里把 `VK_DRIVER_FILES` 指向 NVIDIA 的
  `nv-vk64.json` 后一切正常:
  - `counter.exe` 窗口成功渲染(`FerriticUI: Counter`,DPI 1.25,Arial);
  - 全部 9 张离屏快照在 Windows 上渲染成功,与 macOS golden 的 SSIM 为 0.82–0.96
    (差异来自 Arial vs Helvetica 和字体度量);
  - `widgets` 窗口也成功渲染。
  截图:[counter 窗口](../images/windows-counter-window.png)、
  [widgets 窗口](../images/windows-widgets-window.png)、
  [widgets 快照](../images/windows-widgets-snapshot.png)、
  [form 快照](../images/windows-form-snapshot.png)。
- 注意:Vulkan loader 会忽略提权进程里的 `VK_DRIVER_FILES`;所以必须在普通(非管理员)终端里跑。
  仓库里的 `scripts/run-windows.ps1` 会自动找 NVIDIA Vulkan ICD 和 Windows SDK 的 `dxcompiler.dll`
  并设置好环境,直接 `powershell -ExecutionPolicy Bypass -File scripts\run-windows.ps1 -Example counter`
  即可。要永久生效也可以设置用户环境变量 `VK_DRIVER_FILES`,或更新/移除旧的 AMD Vulkan ICD。

顺带修复:Windows 的 rustc 1.99 对 `ui!` 里的裸浮点字面量(如 `padding: 24.0`)会报
`f32: From<f64>` 推断回退警告;`ferritic-macros` 现在给这些字面量自动加 `f32` 后缀,
Windows 构建已恢复 0 warnings。
