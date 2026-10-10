# synstudy UI 桌面改版与移动端响应式

## 摘要
- 目标：把 `assets/main.css` 的设计令牌与组件类改造为设计稿的浅色 indigo 风格；重构桌面三大核心页（词典 / 词库 / 学习配置）与共享组件的版式；用 CSS 媒体查询实现窄屏（移动端）响应式布局，新增底部 Tab 栏。
- 不改动：数据层、SRS 算法、构建脚本、依赖（不引入 Tailwind / FontAwesome / 远程字体）。
- 交付形态：单一 Dioxus 桌面应用，宽屏显示桌面版式，窄屏（`max-width: 768px`）自动切换移动版式。

## 设计令牌与主题（`assets/main.css`, `src/state.rs`）
- 重写 `:root` 为浅色令牌，对齐设计稿：`--bg:#f8fafc`(slate-50)、`--panel:#fff`、`--border:#e2e8f0`(slate-200)、`--text:#1e293b`(slate-800)、`--muted:#64748b`(slate-500)、`--primary:#6366f1`(indigo-500)、`--primary-soft:#eef2ff`(indigo-50)、`--radius:12px`、`--shadow:0 1px 3px rgba(0,0,0,.06)`；新增 `--success:#10b981`、`--warn:#f59e0b`、`--danger:#f43f5e`、`--teal:#14b8a6`、`--amber:#f59e0b` 用于分组配色。
- 保留 `:root[data-theme="dark"]` 深色令牌（沿用现有深色值）与 `@media (prefers-color-scheme)` 的 system 分支。
- `src/state.rs`：将 `Theme::default()` 由 `System` 改为 `Light`（`#[default]` 移到 `Light`），使默认浅色、仍可切换深色/跟随系统。
- 字体：保持现有系统字体栈（`-apple-system, PingFang SC, ...`），不引入 Google Fonts。

## 应用外壳与响应式导航（`src/app.rs`, `src/components/nav_bar.rs`, `src/components/mod.rs`）
- `NavBar`：桌面顶部栏改版为设计稿样式（左品牌 `s` 渐变方块 + `synstudy.`，右导航按钮，当前页高亮为 indigo 实心 pill，末尾分隔线 + 设置）。给顶部栏加 `class="navbar top"`。
- 新增 `MobileTabBar` 组件（放 `src/components/nav_bar.rs` 或新建 `mobile_nav.rs` 并在 `mod.rs` 注册）：固定底部 5 个 Tab — 词典 / 词库 / 学习 / 生词本 / 我的，图标用内联 SVG，当前页 indigo 高亮，含 `pb-safe` 安全区内边距。
- “我的”行为：在 `AppShell` 内新增 `Signal<bool>` 控制的底部弹出菜单（sheet），列出次级入口 背诵 / 测验 / 词根 / 统计 / 设置，点击后 `page.set(...)` 并关闭 sheet。不新增 `Page` 枚举值。
- `AppShell` 结构：`.app` 内同时渲染 `NavBar`(top) + `main.content` + `MobileTabBar` + sheet；由 CSS 媒体查询决定顶栏/底栏显隐。保留现有全局快捷键 `onkeydown`（Cmd/Ctrl+1..9、Cmd/Ctrl+K）。
- 断点：`@media (max-width: 768px)` 隐藏 `.navbar.top`、显示 `.mobile-tabbar`，并给 `.content` 增加 `padding-bottom` 避让底栏；`@media (min-width: 769px)` 反之。

## 共享组件类改造（`assets/main.css`）
- `.panel`（新增，对应设计稿白卡）：`rounded-xl + border + bg + shadow`；`.btn` 主/次按钮改为 indigo 实心 + slate 描边，圆角 8–10px；`.seg`/`.seg-btn` 分段控件对齐设计稿（灰底 + 选中白底 indigo 文字/或 indigo 实心）。
- `.chip` 考试标签、`.progress`/`.progress-fill`（细轨道 + indigo→violet 渐变填充）、`.card`（学习卡）统一到新令牌。
- 新增分组色调工具类：`.tone-teal/.tone-indigo/.tone-amber`（词库卡图标底色）与 `.bookicon`。

## 词典页（`src/pages/dictionary.rs`）
- 页头改为设计稿版式：eyebrow 小标（`YOUR EVERYDAY WORD COMPANION`）+ `英汉词典` 标题 + 收录数说明；右侧 英汉/汉英 圆角 pill 切换（复用 `SearchMode`）。
- 搜索栏：加高（桌面 ~68px）、左侧放大镜图标、右侧 `Enter ↵` kbd 提示 + 查询按钮；保留现有 `oninput`/`onkeydown`（方向键选择、Enter 打开）与清空按钮逻辑。
- 主体 `.dict-body` 保持左结果列表 + 右详情两栏；空态改为设计稿插画式占位（居中图标 + “输入单词开始查询” + 中英释义/地道发音/实用例句 勾选行）。
- 结果项样式对齐（选中项 indigo 浅底 + 左侧高亮）。
- 响应式：窄屏 `.dict-body` 改单列，结果列表与详情上下堆叠。

## 单词详情（`src/components/word_detail.rs`, `src/components/mod.rs`, `src/components/zh_detail.rs`）
- 重构版式为设计稿分区：标题行（词 + CEFR 徽标 + 音标 + 发音按钮 + 加入生词本按钮）；`释义`；`英文释义` 浅灰盒；`词形变化` 网格（桌面 5 列 / 移动 3 列）；`考试标签` chips + `词频 BNC · COCA` 行；`例句`（每条带发音按钮，目标词 indigo 高亮）；保留 `考试句库` 与 `词根词缀` 区块（同样上令牌）。
- `src/components/mod.rs`：新增 `exchange_pairs(exchange:&str) -> Vec<(&'static str,String)>`（在现有 `format_exchange` 基础上返回结构化 label/value 对，供网格渲染）；`WordDetail` 用其渲染网格。
- 目标词高亮：例句中将当前 `word.word` 词干包一层 `<span class="hl">`（简单字符串包含匹配，找不到则整句原样）。
- `ZhDetail`（汉英）同步套用新令牌与分区样式，保持现有交叉跳转逻辑。
- 移动端单词详情对应设计稿：靠共享 CSS + 单列即可呈现，无需单独组件。

## 词库列表页（`src/pages/categories.rs`, `src/components/category_tile.rs`）
- 页头：eyebrow（`BUILD YOUR VOCABULARY`）+ `词库` 标题 + 说明；右侧 `N 个精选词库` 胶囊；下方图例行（基础教育/大学与专业/留学考试 三色点）。
- `CategoryTile` 改为设计稿卡片：左 `bookicon`（缩写 + 分组色）、标题 + `X 词 · 分组`、状态行（`已学 n / total` 或 `尚未开始` + `待复习 n`）、进度轨道、`学习`/`背诵` 按钮；当前选中且已开始的词库加 `学习中` 徽标 + indigo 描边高亮。
- `category_tile.rs` 或 `categories.rs` 新增 `category_visual(key:&str) -> (&'static str /*abbr*/, &'static str /*tone class*/, &'static str /*group label*/)`，映射：`xiaoxue→("Aa",teal,基础教育)`、`chuzhong→("Ab",...)`、`gaozhong→("Ac",...)`、`cet4→("C4",indigo,大学与专业)`、`cet6→("C6",...)`、`tem4→("T4",...)`、`tem8→("T8",...)`、`toefl→("🌐",amber,留学考试)`、`gre→("G",amber,...)`。
- `Categories` 传入当前 `selected` 词库 id 以便高亮“学习中”卡片。
- 响应式：`.cat-grid` 窄屏改单列。

## 学习配置页（`src/pages/study.rs` — Idle 阶段）
- 顶部加面包屑（词库 › 当前词库名）。
- 概览卡：左大图标（同 `category_visual`）+ 词库名 + `学习中` 徽标 + `CET-x / 分组` 副标；右三列统计（已学 x/total、未学、待复习）；下方进度条 + 百分比。
- 双列区：左 `学习配置` 面板（学习顺序分段、每轮数量分段 20/30/40/50、信息说明块、`继续第 N 轮` + `查看全部单词` 按钮，沿用现有 `persist`/`begin`/`locked` 逻辑）；右 `我的学习计划` 侧栏（已完成/总轮数大数字 + 进度条 + 激励语）。
- 底部 `轮次记录` 区：网格化每轮状态（进行中/未开始/已完成），保留 `查看` 与 `清除轮次记录`（含二次确认）逻辑。
- `Phase::Running`/`Done`/`RoundReview`/`WordList`：套新令牌 + 响应式（窄屏 `.dict-body` 单列、卡片全宽），不改交互逻辑与快捷键。
- 响应式：窄屏双列改单列堆叠。

## 其余页面令牌统一（`src/pages/review.rs`, `quiz.rs`, `roots.rs`, `stats.rs`, `settings.rs`, `wordbook.rs`, `sentence_quiz.rs`）
- 这些页面复用 `.btn/.panel/.seg/.chip/.card/.progress/.stat-*` 等共享类，主要通过 `main.css` 令牌更新自动获得新风格；仅补充：`.stat-cards`/`.stats-row`/热力图/预测图在浅色下的对比度校准，以及窄屏响应式（统计网格降列、句子题选项单列、设置行换行）。
- `word_card.rs`、`streak_card.rs`、`heatmap.rs`：按新令牌微调（卡片留白、发音按钮、连续学习卡配色）。

## 构建与验证
- `cargo clippy --all-targets` 零警告；`cargo build`（默认 desktop feature）通过。
- `dx serve`（或 `cargo run`）手动核对：宽窗（≥1180）呈现桌面版式，窄窗（≤768）呈现移动版式（顶栏隐藏、底栏出现、单列布局）；浅色为默认，设置页可切深色/跟随系统。
- 逐页走查 9 个页面 + 词典空态/结果态 + 学习 Idle/Running/Done/轮次回顾，确认无溢色、无布局错位。

## 假设
- “移动端”= 同一桌面应用内的响应式窄屏布局（非原生 iOS/Android 打包）。
- “我的”Tab 以底部弹出菜单承载次级入口（背诵/测验/词根/统计/设置），不新增 `Page` 枚举。
- 图标使用内联 SVG / 现有 Unicode 字形，字体使用系统字体栈；不引入任何 CDN 资源。
- 断点取 `768px`；如需其它阈值可后续调整。