# 截图 / 拍照翻译功能（`translate` feature）

本文面向**开发者与打包者**，说明如何构建、运行与打包 synstudy 的本地
「截图 / 拍照翻译」功能，以及它的架构、模型分发与合规约束。

> 该功能**默认不编译**。所有重依赖（OCR / LLM 推理引擎、截屏、下载器）都挂在
> 可选 Cargo feature `translate` 之下，因此**默认构建、CI 与移动端打包不受
> 影响**，也不需要 cmake / C++ 工具链 / 多 GB 模型。

---

## 1. 分阶段交付状态

| 阶段 | 平台 | 状态 |
| --- | --- | --- |
| Phase 1 | macOS（Metal） | **代码已落地**，`cargo check --features translate` 在 Apple silicon macOS 上**编译全绿**；运行时端到端（截屏→框选→OCR→译文）待实机点验 |
| Phase 2 | Windows / Linux（Vulkan，可选 CUDA） | **构建入口已补齐**：`translate-vulkan` feature（→ `llama-cpp-2/vulkan`，编 `GGML_VULKAN=ON`）已接通，Windows 侧提供 `scripts/check_translate.ps1` 一键配好 MSVC + Vulkan SDK 工具链。端到端仍需在装有 Vulkan SDK 与 ≥ 一款 Vulkan 显卡的实机上点验 |
| Phase 3 | iOS（Metal）/ Android（CPU-NEON） | **入口与打包链路已接通**（见下）；原生 C++ 依赖仍需首次实机验证 |

Phase 3 iOS 更新（2026-10）：

- `Cargo.toml` 把 `xcap` 移到 `[target.'cfg(not(any(ios, android)))'.dependencies]`，`translate` feature 在 iOS target 上不再拉入桌面截屏库；`rfd` 保持桌面专属。
- `pages/translate.rs` 在 iOS/Android 分支加入通过 `document::eval` + `FileReader.readAsDataURL` + `dioxus.send()` 回传 base64 的“选择图片”按钮，走 WKWebView 原生的 **拍照 / 相册 / 文件** 系统选择器；解码在后台线程复用同一个 `capture_out` 队列。同时保留 **「识别文本」+「翻译此文本」** 作为纯文本入口。
- `nav_bar.rs` 的移动端导航入口随构建而变：带 `translate` 时 `Page::Translate` 直接占底部 tab 第 2 位（原「词库」tab 让位，`Page::Categories` 收进 **Consolidate**（“巩固”）hub —— 「学习」页里本来就有直达词库的按钮）；不带 `translate` 的构建保留「词库」tab，因为此时翻译页只是一句“当前构建未包含翻译功能”的占位，不该占一个主 tab。桌面端始终在顶部 navbar 显示「翻译」。
- `Dioxus.toml` 补 `NSCameraUsageDescription` / `NSPhotoLibraryUsageDescription`（iOS 强制项，缺失在用户点击拍照时崩溃）。
- `scripts/package_ios.sh` 支持 `--features translate`：自动写一份 iOS CMake toolchain 到 `.cargo/ios-cmake-toolchain.cmake`，并导出 `CMAKE_TOOLCHAIN_FILE` / `CMAKE_OSX_SYSROOT` / `CMAKE_OSX_ARCHITECTURES` / `CFLAGS` / `CXXFLAGS` / `BINDGEN_EXTRA_CLANG_ARGS`，让 `llama-cpp-2`（llama.cpp，Apple 自动启用 Metal）和 `mnn-sys`（MNN）交叉到 iOS 模拟器目标。
- **仍需实机点验**：
  1. iOS 模拟器上 `llama-cpp-2` 的 Metal shader（`.metal` → `.metallib`）是否被正确编译。
  2. `mnn-sys` 的 cmake 在 iOS toolchain 下是否产出静态 `.a`。
  3. `.ipa` 体积：带 `translate` 的二进制大约 +80~120 MB。
  4. 内存：HY-MT1.5-1.8B Q4_K_M 常驻 ~1.1 GB + KV cache（`n_ctx=1024`）。

Phase 1 已实现：`translate` feature + 桌面截屏 + OCR + HY-MT 翻译全链路 + 模型
按需下载 / 校验 + `Page::Translate` 桌面入口 + 流式 UI + 许可证与声明。

---

## 2. 架构与模块

全部新增代码以 `#[cfg(feature = "translate")]` 门控（页面 UI 另有「功能未编入」
的降级分支，两种配置都能编译）。

```
src/
├── capture.rs              # 桌面截屏（xcap 全屏 + macOS `screencapture -i` 区域）；仅非移动端编译
├── translate/
│   ├── mod.rs              # 编排层：常驻后台线程引擎 + 命令 / 消息通道 + LANGS
│   ├── models.rs           # 模型清单 / 下载 / SHA-256 校验 / 状态机
│   ├── ocr.rs              # rust-paddle-ocr 封装（Det + Rec）
│   └── mt.rs               # llama-cpp-2 封装（HY-MT greedy 解码 + 流式回调）
└── pages/translate.rs      # 页面 UI（截屏 / 框选区域 / 打开图片 → ROI → OCR → 流式译文）
```

### 并发模型（重要）

推理是 CPU/GPU 重负载，**必须脱离 UI 线程**：

- `translate::engine()` 是**进程级单例**，首次访问时懒启动一个名为
  `synstudy-translate` 的 `std::thread` worker。
- worker 持有（懒加载的）OCR 与翻译模型句柄，串行处理 `Command`
  （`Translate` / `DownloadOcr` / `DownloadMt` / `Unload`）。
- worker **不直接写 Dioxus `Signal`**（Signal 只能在 reactive 作用域内写）。
  它把 `Msg` 推进一个 `Arc<Mutex<VecDeque<Msg>>>` 共享收件箱。
- 页面用 `use_future` + `futures_timer::Delay`（80ms）轮询 `engine().drain()`，把消息
  折叠回组件自己的 `Signal`。这样既遵守 Dioxus 的响应式规则，又天然
  remount-safe（避免 `Receiver` 无法跨组件共享的问题）。定时器用
  `futures-timer` 而非 `TimeoutFuture`：Dioxus 0.8 桌面 prelude **不提供**
  任何 timer future（其 `TimeoutFuture` 来自 gloo，仅 wasm 可用）。

流程：`图像 → OCR 文本行 → HY-MT 翻译（逐 token 流式回传）`。OCR 完成即启动
翻译，无需等待用户确认（但 UI 保留可编辑纠正入口）。

---

## 3. 构建前置（系统依赖）

`rust-paddle-ocr`（经 MNN）与 `llama-cpp-2`（经 llama.cpp）都会**从源码编译
C++**，因此构建机 / CI 需预装：

| 依赖 | 说明 |
| --- | --- |
| **cmake ≥ 3.22** | llama.cpp 与 MNN 的 C++ 构建系统 |
| **C++17 工具链** | macOS: Xcode Command Line Tools（clang）；Windows: MSVC（VS Build Tools）；Linux: gcc / clang |
| **pkg-config** | Linux 上定位原生库 |

各平台额外项：

- **macOS**：`xcode-select --install` + `brew install cmake`。GPU 走 **Metal**，
  由 llama.cpp 在 Apple 平台**自动启用**，无需额外 feature。截屏（xcap）首次
  运行会触发系统「屏幕录制」授权。
- **Linux**：`cmake`、`clang`/`gcc`、`pkg-config`；截屏需 X11 / Wayland 开发库
  （`libxcb`、`libxrandr`、`libdbus`、`wayland-client` 等，随发行版包名不同）；
  Vulkan 后端需 Vulkan 加载器与 `glslc`（Vulkan SDK / `shaderc`）。
- **Windows**：MSVC 构建工具（VS 2017+「使用 C++ 的桌面开发」工作负载，或 Build Tools）
  + `cmake`；Vulkan 后端需从 LunarG 安装 **Vulkan SDK**（勾选「Add to PATH」，安装器会设
  `VULKAN_SDK`）。llama.cpp 的 `build.rs` 在 Windows 上会**硬性读取 `VULKAN_SDK`**
  定位头文件与 `%VULKAN_SDK%\Lib\vulkan-1.lib`（未设置直接 panic），并用
  `%VULKAN_SDK%\Bin\glslc` 把 GLSL 着色器编成 SPIR-V。**普通 PowerShell 里没有
  `cl.exe`**（MSVC 只在「Developer PowerShell for VS」中才在 PATH 上），因此请直接用
  `scripts/check_translate.ps1`（自动 `vswhere` 定位 VS、导入 vcvars、探测 Vulkan SDK），
  原理见第 4 节。
- **CUDA（可选）**：需 CUDA Toolkit，且用 clang 作为主机编译器。

---

## 4. 构建与运行

```bash
# 桌面（macOS，Metal 自动启用）
cargo run --release --features translate

# 或用 dx CLI（热重载）
dx serve --platform desktop --features translate

# Windows / Linux：启用 Vulkan 后端
cargo run --release --features translate-vulkan

# NVIDIA：启用 CUDA 后端
cargo run --release --features translate-cuda
```

> **macOS 本地构建请用 `scripts/check_translate.sh`**：
> `bash scripts/check_translate.sh [check|build|run|clippy]`。
> 若你的 `PATH` 里 **Android NDK 的 clang 排在 `/usr/bin/clang` 之前**（本项目同时
> 配了 Android 交叉编译，很容易命中），直接 `cargo … --features translate` 会在编译
> ggml / MNN 的 C/C++ 时报 `fatal error: 'stdio.h' / 'functional' file not found`。
> 脚本强制改用 Apple clang 并显式传 `-isysroot $(xcrun --show-sdk-path)`，把 bindgen
> 指向 Xcode 自带 libclang，一次性绕开这些坑（原理见第 8 节排查表）。

> **Windows 本地构建请用 `scripts/check_translate.ps1`**（Vulkan 后端，等价于 macOS 的
> `check_translate.sh`）：
>
> ```powershell
> # 在仓库根，普通 PowerShell 即可（脚本会自行导入 MSVC 开发者环境与 Vulkan SDK）
> powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 check          # cargo check --features translate-vulkan
> powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 build -Release # cargo build --release --features translate-vulkan
> powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 run           # cargo run --features translate-vulkan
> powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 env           # 只自检工具链（cl/cmake/glslc/VULKAN_SDK），不构建
> powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 pin           # 首次构建报 E0106/E0308 时，把 mnn 固定回 v1.4.2 兼容 rev
> ```
>
> 脚本做的事：① `vswhere` 枚举装了 MSVC x64 工具的 Visual Studio，选「本机 CMake
> 认识其生成器的最高版本」（如 VS 2022）并运行 `VsDevCmd.bat -arch=amd64` 导入
> `cl.exe`/Windows SDK，同时把 `CMAKE_GENERATOR` 锁定（有 ninja 优先 Ninja，避开 VS 多配置
> 生成器在 Rust build script 下的子项目探测坑），并向 `CMAKE_C/CXX_FLAGS` 环境变量追加
> `/FS`（防并发 cl.exe 写同一 PDB 报 C1041）；② 探测 Vulkan SDK（`VULKAN_SDK` 未设时在
> `C:\VulkanSDK\*` 与注册表里找最新版）并把 `%VULKAN_SDK%\Bin` 加进 PATH 供 `glslc` 使用；
> ③ bindgen 工具链同源对齐：探测含 `libclang.dll` 的完整 LLVM 安装（已有 `LIBCLANG_PATH` →
> 常见 LLVM 目录 → VS 的 C++ Clang 组件），把它的 bin 前置到 PATH 并回设 `LIBCLANG_PATH`，
> 避开 AMD ROCm 抢先把自家 clang 头文件塞给 bindgen 造成的 `__builtin_ia32_*` 未声明错误；
> ④ 仓库路径偏深时（llama Vulkan 子项目中间路径超 CMake 250 字符安全上限）自动把构建
> 目录搬到短 base（设 `CARGO_TARGET_DIR=C:\dst`，dx/cargo 都优先读它；`-TargetDir <路径>`
> 可指定、`-KeepTargetDir` 可关闭）；④ 设 `CARGO_NET_GIT_FETCH_WITH_CLI=true` 后用系统
> git 拉 `rust-paddle-ocr`/`mnn` git 源。若本机是 NVIDIA 显卡且装了 CUDA Toolkit，把
> `-Feature translate-cuda` 覆盖默认后端即可。首次 clean 构建前请先跑一次 `pin`（本仓库
> 不跟踪 `Cargo.lock`，`mnn` 浮动 git 依赖会漂到破坏性 HEAD，详见第 8 节）。

feature 一览（见 `Cargo.toml`）：

| feature | 含义 |
| --- | --- |
| `translate` | 基础：OCR + 翻译 + 截屏 + 下载器（Apple 平台自动带 Metal） |
| `translate-vulkan` | `translate` + llama.cpp 的 Vulkan 后端（Win/Linux） |
| `translate-cuda` | `translate` + llama.cpp 的 CUDA 后端（NVIDIA） |

新增依赖（全部 `optional = true`，仅随上述 feature 拉入）：
`rust-paddle-ocr`（git）、`llama-cpp-2` `0.1`、`xcap` `0.9`、
`reqwest` `0.12`（`blocking` + `rustls-tls`）、`sha2` `0.10`、`image` `0.25`、
`futures-timer` `3`（UI 轮询用的跨平台异步定时器）、
`rfd` `0.15`（仅桌面，`default-features = false`，「打开图片」原生文件对话框）。

### 图像输入与区域选择（桌面）

页面顶栏提供三个入口，产物都会落入同一个预览 / ROI 流程：

- **截取屏幕**：`xcap` 抓取主显示器整屏。
- **框选区域**（仅 macOS）：调用系统 `screencapture -i`，用户可拖出任意位置 / 大小的
  矩形，像素精确；取消则保留原状态。需授予「屏幕录制」权限。若因**权限未授 / 跨屏
  框选**导致 `screencapture` 取不到图（`could not create image from rect`），会自动**降级为
  全屏抓取**，用户再用拖拽 / 数值框选区（区分于按 Esc 主动取消，后者不降级）。
- **打开图片**：`rfd` 原生文件对话框选本地图片（png/jpg/bmp/webp/…）。

截屏后在预览图上**拖拽框选**要翻译的区域；预览图下方另有 **X / Y / 宽 / 高（像素）**
数值框可精确设定位置与大小，以及「清除选区」（不选区即处理整图）。非 macOS 桌面
无系统框选按钮，走拖拽 + 数值框这条路。

> **不阻塞 UI**：三个入口（截屏 / 系统框选 / 打开图片）的阻塞操作（原生对话框、
> `screencapture`、图片解码）都放在**临时后台线程**上执行，结果通过一个共享
> `Arc<Mutex<VecDeque<CaptureOutcome>>>` 队列回传，由页面已有的 80ms 轮询循环排空
> 并写回 Signal（Signal 只能在 reactive 上下文写，worker 线程不直接碰）。rfd 在
> macOS 本就不应在主线程调用，此改造同时也避免了潜在死锁。

> **单元测试**：`mt.rs` 的提示词模板测试仅在 feature 开启时编入，运行
> `cargo test --features translate`。默认 `cargo test` 不含翻译测试。

### 打包带翻译功能的桌面安装包

⚠️ **默认打包不含本功能**。README「打包发布」里的 `dx bundle` 与
`scripts/package_macos.sh` 都按**默认 feature** 构建：`package_macos.sh` 内部调用
`dx bundle` 时**没有**传 `--features translate`，所以产出的 `.app` / 安装包**没有截图
翻译入口**（页面走「功能未编入」的降级分支）。这是刻意的——默认包不拉入
cmake / C++ 工具链与多 GB 依赖，也不影响 CI 与移动端。

要产出**带翻译**的 macOS 安装包，必须在与 `scripts/check_translate.sh` **相同的
工具链环境**下给 `dx bundle` 显式加 `--features translate`（否则会同样踩到
「PATH 上第一个 clang 是 Android NDK clang → 找不到 `stdio.h` / `functional`」等坑，
见第 8 节），再用打包脚本注入品牌图标：

```bash
# 1) 带 translate 打包（工具链环境与 check_translate.sh 一致）
SDK="$(xcrun --show-sdk-path)"
SDKROOT="$SDK" CC=/usr/bin/clang CXX=/usr/bin/clang++ \
CFLAGS="-isysroot $SDK" CXXFLAGS="-isysroot $SDK" \
BINDGEN_EXTRA_CLANG_ARGS="-isysroot $SDK" \
LIBCLANG_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib" \
CARGO_NET_GIT_FETCH_WITH_CLI=true \
dx bundle --release --features translate

# 2) 注入品牌图标并用打过补丁的 .app 重建 dmg（--no-build：跳过重复的 dx bundle）
bash scripts/package_macos.sh --no-build
```

说明：

- **模型不随包分发**：带翻译的包与默认包在 `[bundle] resources`（词典库 +
  第三方声明）上一致，仅二进制多编入了 feature；OCR / HY-MT 模型仍由用户首次
  使用时按需下载到应用数据目录（见第 5 节），安装包体积不受影响。
- **图标注入与 dmg 重建**：`dx bundle` 自产的 `Synstudy.icns` 并不可靠——除现代 PNG
  表示外还混入了**长度不合规范的 legacy 位图表示**（is32/il32/it32 + 1-bit 掩码对，
  实测 2026-10 dx 0.8.0-alpha.1）。IconServices 会扫描并合并 Resources 下**全部** .icns
  的表示，坏 legacy 条目会污染小尺寸渲染（16/24px 发糊、白环、右下角调色板垃圾色块，
  大尺寸走正常 PNG 表示故 Dock/启动台看不出问题）；且 dmg 是 bundle 时就把当时的
  .app 打进去了，晚于注入。故 `inject_macos_icon.py` 注入 iconutil 规范产的 `icon.icns`
  时会**顺手删除 bundle 里其它 .icns**；`package_macos.sh`（含 `--no-build`）随后用打过
  补丁的 .app **自动重建/生成 dmg**（hdiutil UDZO，含 /Applications 拖拽链，先写临时
  文件再原子覆盖），特例用 `--no-dmg` 跳过。注意别被 `iconutil -c iconset` 的像素比对
  骗到：它只解现代 PNG 条目、无视残缺 legacy 条目。另：系统图标缓存粘滞，装新包后
  Finder 仍显旧图标时执行 `sudo rm -rf /Library/Caches/com.apple.iconservices.store && killall Dock Finder`；
  圆角图标外围那圈浅色光晕是 macOS 标准投影（系统 App 同样如此），不是 bug。
- **Windows / Linux**：同理，把 `--features translate` 换成 `translate-vulkan` /
  `translate-cuda` 并满足第 3 节的原生依赖。**Windows 直接用打包模式**（需先装 dx CLI，
  脚本会复用与构建相同的 MSVC + Vulkan SDK 工具链环境）：

  ```powershell
  powershell -ExecutionPolicy Bypass -File scripts\check_translate.ps1 bundle -Release
  ```

  等价于 `dx bundle --release --features translate-vulkan`，产物落在
  `target\dx\synstudy\bundle\windows\`（若脚本触发了短 base 搬家，则在
  `C:\dst\dx\synstudy\bundle\windows\`，脚本会打印实际位置；dx 默认只产 NSIS
  `*-setup.exe`，`.msi` 未启用）。
  Linux 则手动跑 `dx bundle --release --features translate-vulkan` 产出 `.deb`。
  注意：图标注入脚本目前**仅 macOS**，Windows/Linux 的 dx 自产图标不额外处理。运行时
  需要 `vulkan-1.dll`（随显卡驱动装于 System32，一般无需随包分发）；若目标机无
  Vulkan 兼容 GPU，llama.cpp 会自动回退 CPU（`translate_gpu` 设置项控制是否卸载到 GPU）。
- **退出崩溃（macOS）**：带翻译的包关闭时若报 `GGML_ASSERT([rsets->data count] == 0)`
  + `Abort trap: 6`，是常驻的 llama/Metal 资源未在进程 `exit()` 前释放；已在
  `translate/mod.rs` 用有序注册的 `libc::atexit` 钩子（首次加载模型后 arm，确保在
  ggml 的 Metal device 析构前跑）释放常驻模型修复。

---

## 5. 模型：按需下载与管理

模型**绝不打包进安装包**（体积与 App Store 限制，且规避再分发合规风险），
首次使用时下载到 `platform::models_dir()`：

```
<data_dir>/models/
├── ocr/
│   ├── PP-OCRv5_mobile_det.mnn     # 文本检测（约 4.8 MB）
│   ├── PP-OCRv5_mobile_rec.mnn     # 文本识别（约 16.5 MB）
│   └── ppocr_keys_v5.txt           # 字符表（约 74 KB）
└── mt/
    └── hy-mt1.5-1.8b-q4_k_m.gguf   # 翻译模型 Q4_K_M（约 1.08 GB）
```

`<data_dir>` 与进度库同目录（macOS：`~/Library/Application Support/com.synvek.synstudy/`；
Linux：`~/.local/share/synstudy/`；Windows：`%APPDATA%\Synvek\Synstudy\data\`；
iOS：App 沙盒 Documents；Android：应用私有 files 目录）。

下载能力（`translate/models.rs`）：当前每个文件**单一 ModelScope 源**（我们上传的
`synvek/*` 仓库，即权威出处）；镜像回退列（`ModelFile.urls`）预留但暂未接入，
后续自行设计可信备选后再加。流式进度回调、**SHA-256 校验**（四个文件均已录入
真实摘要）、`.part` 临时文件下载完成后原子 `rename` 落地、失败清理。状态机：
`Missing` / `Ready` / `Invalid`。

> 请求必须带**非空 `User-Agent`**，否则 ModelScope 的 LFS CDN 在 `resolve` 302 跳转后
> 返回 `403`（`download_group` 已设置）。校验方式：`curl -A "" -L <URL>` 会 403，带任意
> 非空 UA 则 200/206。

### 发布前检查项（已到位）

`models.rs` 中各 `ModelFile` 状态：

1. ✅ OCR 三个文件（det / rec / keys）的 URL、文件名、SHA-256 已从 `synvek/PP-OCR`
   下载核实并录入；
2. ✅ HY-MT GGUF（`synvek/HY-MT1.5-1.8B-GGUF`，文件名小写 `hy-mt1.5-1.8b-q4_k_m.gguf`）
   的 URL 与 SHA-256（约 1.08 GB）已核实录入；
3. ✅ 各 `size_hint` 改为真实字节数（注意 keys 仅 74 KB，旧提示 250 KB 会使状态误判
   `Invalid`）；
4. 待办：`image` 版本与 `rust-paddle-ocr` 传递依赖对齐（当前 macOS 已跑通，`image` 解析 `0.25.8`）。

---

## 6. GPU 后端与运行时设置

后端选择：Apple → **Metal**（自动）；Win/Linux → **Vulkan**（`translate-vulkan`）
或 **CUDA**（`translate-cuda`）；未编译任何 GPU 后端或无可用 GPU → **CPU**
（NEON / AVX）兜底。翻译模型的 GPU 卸载由 `n_gpu_layers` 控制（开启时推到全部
层，关闭时为 `0`）。

设置页「翻译（截图 / 拍照）」新增（均持久化到进度库，`Settings` 带
`#[serde(default)]`，旧存档可解析）：

| 字段 | 默认 | 说明 |
| --- | --- | --- |
| `translate_target_lang` | `Chinese` | 目标语言（页面顶栏也可即时切换） |
| `translate_src_lang` | `auto` | 源语言；`auto` 让模型自行检测 |
| `translate_gpu` | `true` | 是否把模型层卸载到 GPU |
| `translate_keep_model_resident` | `true` | 翻译后是否常驻模型（关闭则空闲即卸载约 1 GB） |

上下文窗口 `n_ctx`：桌面 2048 / 移动端 1024（`pages/translate.rs` 的
`default_n_ctx()`）。翻译采用 **greedy 解码**（确定性，适合翻译），直接基于
logits 实现以最小化 FFI 面积。

---

## 7. 许可与合规

- **代码组件**：MNN（Apache-2.0）、llama.cpp（MIT）随程序静态链接，允许自由
  使用 / 修改 / 再分发（含商用），保留声明即可。
- **模型权重**：PaddleOCR（Apache-2.0）、HY-MT1.5-1.8B（以腾讯混元官方模型卡
  为准）**不随包分发**，仅由用户在运行时从官方源下载，规避再分发合规风险。
  HY-MT 的使用须遵守其官方许可条款（可能含商用规模阈值等限制）。
- 声明文件：`license/MNN-NOTICE.txt`、`license/PaddleOCR-NOTICE.txt`、
  `license/llama-cpp-NOTICE.txt`、`license/HY-MT-NOTICE.txt`，并在
  `license/README.md` 的「可选『翻译』功能组件」小节登记。
- 应用内「设置 → 关于 → 版权与许可」在启用 feature 时会额外列出上述组件。
- 第三方声明生成：`scripts/gen_notices.py` 仍以**入库数据源**为准；翻译组件
  可选地用 `python3 scripts/gen_notices.py --with-translate` 追加一段附录，该
  附录**不计入** `notices_version` 哈希，故默认（数据-only）产物逐字节不变。

---

## 8. 故障排查

| 现象 | 排查 |
| --- | --- |
| 构建报 `cmake: command not found` | 安装 cmake ≥ 3.22（macOS：`brew install cmake`） |
| C++ 编译失败 / 找不到编译器 | 装 Xcode CLT（macOS）/ MSVC Build Tools（Win）/ gcc·clang（Linux） |
| Windows：`linker cl.exe not found` / 报找不到 `cl` | 普通 PowerShell 无 MSVC 环境。用 `scripts\check_translate.ps1`（自动导入 vcvars），或从「Developer PowerShell for VS」运行 |
| Windows：CMake 报 `Could not create named generator Visual Studio 18 2026` | 装了 VS 2026(v18) 但本机 CMake < 4.2（该生成器 4.2 才加入），cmake crate 的 vswhere `-latest` 探测选中了 v18。`check_translate.ps1` 会把生成器锚定到本机 CMake 认识的 VS（如 2022），有 ninja 则优先 Ninja；也可 `-CMakeGenerator 'Visual Studio 17 2022'` 手动指定。同目录不能换生成器，切换后需删掉旧 build/fingerprint 目录重跑 |
| Windows Vulkan：build.rs panic `Please install Vulkan SDK and ensure that VULKAN_SDK env variable is set` | 未装 Vulkan SDK，或 `VULKAN_SDK` 未设。装 SDK 并勾选 Add to PATH；`check_translate.ps1` 会在 `VULKAN_SDK` 缺失时自动到 `C:\VulkanSDK\*` 与注册表里找最新版 |
| Windows Vulkan：`vulkan-shaders-gen` / `glslc` 找不到，或 SPIR-V 生成失败 | `%VULKAN_SDK%\Bin` 不在 PATH（`glslc` 在那里）。用 `check_translate.ps1`（会拼入 PATH），或手动 `$env:PATH="$env:VULKAN_SDK\Bin;$env:PATH"` |
| Windows：Ninja 高并发下 `fatal error C1041`（多个 cl.exe 写同一 vc140.pdb） | 需 `/FS`。注意 llama 的 `vulkan-shaders-gen` 是 ExternalProject 子项目，configure 时不继承父级 flags，只能从同名环境变量初始化缓存——`check_translate.ps1` 已向 `CMAKE_C/CXX_FLAGS` 追加 `/FS`；手动构建时记得同样设置 |
| Windows：子项目 configure 报 `loading 'CMakeFiles\rules.ninja': The system cannot find the file specified` 或 MSB8066，伴随 CMake warning「has NNN characters / maximum 250」 | 仓库路径偏深：llama Vulkan 子项目的 try-compile 中间路径超 CMake 250 安全上限 / Win32 260 限制，文件实际打不开。`check_translate.ps1` 检测到偏深会自动把构建目录搬到短 base（`CARGO_TARGET_DIR=C:\dst`，省 ~24 字符）；`-TargetDir` 可指定、`-KeepTargetDir` 可关闭。搬后首次会全量重编；安装包随 base 落到 `C:\dst\dx\synstudy\bundle\windows\` |
| Windows：`FileTracker : error FTK1011` / 路径过长（NMAKE / MSBuild） | llama.cpp build.rs 已对 MSVC 设 `TrackFileAccess=false` 并加 `/FS`；仍现则同上「rules.ninja 找不到」行（短 base 搬家），或把仓库放到盘符浅路径（如 `C:\work\synstudy`）以避开 PATH_MAX |
| Windows：运行时弹窗缺 `vulkan-1.dll` | 目标机未装 Vulkan 兼容显卡驱动。升级驱动即可（`vulkan-1.dll` 随驱动入 System32）；无 GPU 时 llama.cpp 自动回退 CPU |
| macOS 报 `'stdio.h' file not found`（ggml）或 `'functional' file not found`（MNN `Interpreter.hpp`） | 根因是 **PATH 上第一个 clang 是 Android NDK clang**（面向 Android，不会自动定位 macOS SDK 头文件），**且项目 `.cargo/config.toml` 用 `force=true` 清空了 `SDKROOT`/`C_INCLUDE_PATH`**，所以仅 `export SDKROOT` 无效。用 `bash scripts/check_translate.sh` 构建（强制 `CC/CXX=/usr/bin/clang(++)` + 命令行显式 `-isysroot $(xcrun --show-sdk-path)`）。手动等价：`SDK=$(xcrun --show-sdk-path); CC=/usr/bin/clang CXX=/usr/bin/clang++ CFLAGS="-isysroot $SDK" CXXFLAGS="-isysroot $SDK" BINDGEN_EXTRA_CLANG_ARGS="-isysroot $SDK" LIBCLANG_PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib" cargo check --features translate` |
| bindgen 报泛化的 `libclang error`（llama / MNN） | **别往 `BINDGEN_EXTRA_CLANG_ARGS` 里加 `-std=c++17`**：它会被无差别套用到 C 头文件（llama 的 `wrapper.h`），clang 报 `'-std=c++17' not allowed with 'C'`。全局参数只放语言中立的 `-isysroot`，各 crate 自带其 C++ 标准（MNN 内部 c++14）。并确认 `LIBCLANG_PATH` 指向 Xcode 或 CommandLineTools 的 `libclang.dylib` |
| `rust-paddle-ocr` 编译报 `E0106 missing lifetime specifier`（`mnn::Session`）或 `E0308`（`copy_from_host_tensor` 参数类型） | 传递依赖 `mnn`（`aftershootco/mnn-rs`）的上游 API 在漂移，`rust-paddle-ocr` v1.4.2 只兼容特定 rev。**`Cargo.lock` 已把 `mnn`/`mnn-sys` 固定到 `1a275e70`（v1.4.2 自身 lock 所指的 rev）**。别随手 `cargo update`（会把 mnn 拉回带 `Session<'i>` 生命周期的新版）；若不小心动了，执行 `cargo update -p mnn --precise 1a275e70336d2f21603a41b22b3b257bc3b49333` 复原。注：`[patch]` 同 URL 换 rev、或直接加 `mnn={rev=…}` 依赖都不行（分别报「patches must point to different sources」与 `links="mnn"` 冲突），只有 `cargo update --precise` 能原地改写单条 lock 项 |
| 拉取 `rust-paddle-ocr` 卡住 / 极慢 | 该依赖及其传递依赖 `mnn`（`aftershootco/mnn-rs`）均为 GitHub git 源，国内直连易超时：`export http_proxy=... https_proxy=...` 并设 `CARGO_NET_GIT_FETCH_WITH_CLI=true`（让 git CLI 走代理）后重试；或临时 `git config --global url."https://kkgithub.com/".insteadOf "https://github.com/"`（用完 `git config --global --unset` 撤销） |
| 非离线构建卡在网络（无 `Blocking` 也久无进展） | 除了 git 源，`rust-paddle-ocr` 的 `build.rs`（cbindgen）会**非离线**地调用 `cargo metadata` 重新解析 **crates.io**（即便整体 `--offline`，这一步也不受控）。因此首次 clean / 改依赖 / `cargo update` 后重跑，需要一个**能覆盖 `index.crates.io` + `static.crates.io` 的通用代理**（仅加速 GitHub 的域名级代理不够）。终端先 `export http_proxy=... https_proxy=...` 再 `bash scripts/check_translate.sh`。依赖已全部缓存后，日常 `--offline` 构建不依赖网络 |
| `no matching package named \`rust-paddle-ocr\` found` | **上游已把默认分支 `master` 上的 crate 改名为 `ocr-rs`（v2.x）**；本项目代码针对 `main` 分支的 `rust-paddle-ocr` v1.4.2，`Cargo.toml` 已显式 `branch = "main"`。若仍报错，清掉只缓存了 `master` 的旧库：`rm -rf ~/.cargo/git/db/rust-paddle-ocr-* ~/.cargo/git/checkouts/rust-paddle-ocr-*` 后重拉 |
| Linux 截屏链接错误 | 安装 X11 / Wayland 开发库（`libxcb` / `libxrandr` / `libdbus` 等） |
| macOS 截屏全黑 / 无权限 | 系统设置 → 隐私与安全性 → 屏幕录制，授权本应用后重启 |
| 「框选区域」报 `could not create image from rect`（`screencapture` 输出到终端） | 根因是**启动本程序的进程没有「屏幕录制」权限**。`screencapture` 的权限归属于 *responsible process*：从终端 / IDE 跑 `cargo run`（或 `dx serve`）时，需要授权的是**那个终端 / IDE**，而非调试二进制；且裸二进制不是 `.app` 包，TCC 常不弹窗。**解决**：①系统设置→隐私与安全性→屏幕录制，勾选启动它的终端 / IDE（如 Qoder / Terminal）后完全重启；或②用 `scripts/package_macos.sh` 打包成 `.app` 后运行并授权该 app。另：跨屏 / 副屏框选 macOS 可能返回空图，尽量在主屏内选区。（`capture_region` 已捕获 `screencapture` 的 stderr 并拼入错误提示） |
| 预览图上拖拽框选「不工作」（选区不出现） | 根因：`<img>` 默认可拖拽，webview 把 mousedown+move 当成**图片原生拖动手势**吞掉了鼠标事件。已在 `img` 上加 `draggable="false"` 与 CSS `-webkit-user-drag:none; user-select:none`。若个别平台 `element_coordinates()` 仍不稳，用预览图下方的 **X/Y/宽/高数值框**精确设定选区，或（macOS）直接用「框选区域」系统截取 |
| 翻译输出异常 / 乱码 | 确认 GGUF 为**主线 llama.cpp 兼容**量化；必要时核对 `llama-cpp-2` 对应的 llama.cpp 版本 |
| 模型「下载失败」 | 检查网络 / 代理；确认 `models.rs` 中 URL 与文件名有效；查看进度库同目录 `models/` 是否留有 `.part` |
| ModelScope 下载报 `403 Forbidden`（`cdn-lfs-cn-1.modelscope.cn/...auth_key=...`） | 根因：**ModelScope 的 LFS CDN 拒绝空 `User-Agent` 的请求**，而 `reqwest` 默认客户端**不发 UA**（`resolve` 会 302 到带 `auth_key` 的 CDN，那一步被 403）。已在 `models.rs` 的 `download_group` 给客户端加 `.user_agent(...)`。验证：`curl -A "" -L <resolve URL>` 403、带任意非空 UA 则 200/206 |
| 内存吃紧 | 关闭 `translate_keep_model_resident`，或在页面点「卸载模型」 |

---

## 9. 已知限制 / 待办

- 移动端（Phase 3）入口与打包链路已接通：iOS/Android 的图片输入走 `document::eval` +
  `FileReader.readAsDataURL` + `dioxus.send()`；导航入口：带 `translate` 的构建是底部 tab「翻译」，
  不带该 feature 的构建不占 tab（页面仅占位文案）；`capture.rs` 仅在非移动端编译。
  但原生依赖（llama.cpp + MNN）的 iOS 交叉编译仍需首次实机验证（见第 1 节 Phase 3 更新）。
- 模型 URL / SHA-256 为占位，须按第 5 节确认。
- `image` 版本需与 `rust-paddle-ocr` 传递依赖对齐后再锁定 `Cargo.lock`（当前已在
  Apple silicon macOS 上用 `scripts/check_translate.sh` 跑通 `cargo check --features
  translate`，`mnn`/`mnn-sys` 固定到 `1a275e70`，`image` 解析为 `0.25.8`）。
- Phase 1 编译已在真实 macOS（arm64）验证通过；**运行时端到端**（截屏 → 框选 →
  OCR → 中文译文流式输出，Metal 可用 / 无 GPU 时 CPU 兜底）仍需开发者实机点验。
- 迁移到上游新的 `ocr-rs` 2.x（替掉 `rust-paddle-ocr` v1.4.2 + 旧 `mnn` pin）是
  后续项：import 名、Det/Rec API、模型布局都会变，届时可去掉对 `1a275e70` 的锁定。
