# synstudy 下一阶段改进增强计划

## 摘要
在现有功能完整的 MVP（英汉/汉英词典、9 大分级词库、学习模式、SM-2 lite 背诵、统计、设置）之上，按四个**可独立发布**的阶段增强，仍**仅面向桌面端**（wry webview）。四阶段按依赖与风险排序：先补齐底层「progress.db 迁移框架 + 检索/体验」，再叠加学习功能、发音、SRS/统计。每阶段自成闭环，可单独合并交付。

## 现状分析（关键约束）
- `data/wordroot.txt` 是 JSON（键=词根/词缀，字段 `meaning/class/root/example[]/origin/synonyms/antonyms`，14486 行），**尚未入库**，`README` 路线图已预留。
- `progress_repo.rs` 仅用 `CREATE TABLE IF NOT EXISTS`，**无版本迁移机制**；任何新增列/表都需先补迁移框架。
- `review_log(word_id, category_id, reviewed_at, rating)` 已在写，可直接支撑热力图/连续打卡/记忆保持率。
- `dict_repo.rs` 每次调用新开只读连接；`search()` 走 `word LIKE 'q%'` 前缀匹配，无模糊/容错、无全文检索。
- 桌面端为 wry webview，可用 JS `speechSynthesis` 实现**离线** TTS，无需原生依赖。
- 词典库 `schema_version=4`；进度库无 `user_version`。

---

## 阶段 A：检索与体验打磨（底层基础，先行）

### A1. progress.db 迁移框架（后续阶段的前置依赖）
- `src/db/progress_repo.rs`：`ensure_schema()` 改为基于 `PRAGMA user_version` 的顺序迁移：读取当前版本 → 逐条执行 `MIGRATIONS: &[&str]`（版本 1、2…）→ 每步 `PRAGMA user_version = N`。现有建表作为版本 1，后续阶段的新表/新列追加为版本 2、3…。
- 保持向后兼容：老库 `user_version=0` 时从头执行全部 `IF NOT EXISTS` 语句再打版本号。
- 验收：对已存在的老 `progress.db` 启动后数据不丢、版本号正确递增；单测覆盖「空库首建」与「模拟旧库升级」。

### A2. FTS5 全文检索 + 拼写容错
- 先做**能力探测 spike**：确认 `rusqlite { bundled }` 运行时与 `scripts/build_dict_db.py` 使用的系统 `sqlite3` 均支持 FTS5（`CREATE VIRTUAL TABLE ... USING fts5`）。
- `scripts/build_dict_db.py`：若可用，新增 `words_fts` FTS5 虚表（`word` + `translation`，`content='words'`），`schema_version` 升 5；否则跳过并打印告警。
- `src/db/dict_repo.rs`：`search()` 优先走 FTS5（`MATCH` + `bm25` 排序，前缀词自动补 `*`）；**无 FTS5 或查询失败时回退**到现有 `LIKE 'q%'`，再叠加 Rust 侧轻量容错：对前缀候选集按编辑距离 ≤1 兜底匹配（限制候选规模，避免全表 770k 扫描）。
- 验收：查 `achieve`/`acheive`（typo）/`苹果`（释义全文）均有合理结果；无 FTS5 环境下功能不崩溃、回退可用。

### A3. 键盘快捷键
- `src/app.rs`（`AppShell`）：挂全局 `onkeydown`，用 `Signal<Page>` 做页面切换（如 `Ctrl/Cmd+1..6` 对应词典/词库/学习/背诵/统计/设置）。
- `pages/study.rs`、`pages/review.rs`：卡片翻面=空格，评分=`1/2/3/4`（复用 `Rating::key_hint()`，目前 `#[allow(dead_code)]`），学习「认识/不认识」=`Enter`/`Backspace`。
- `pages/dictionary.rs`：`/` 或 `Ctrl/Cmd+K` 聚焦搜索框，`↑/↓` 选结果，`Enter` 打开详情。
- 设置页新增「快捷键」说明区块（只读展示）。

### A4. 暗色主题 + 卡片动画
- `assets/main.css`：抽出颜色为 CSS 变量，新增 `[data-theme="dark"]` 覆盖；`Settings` 增 `theme: Theme`（`System/Light/Dark`，默认 `System`），持久化到进度库；`app.rs` 根节点按设置写 `data-theme`，`System` 时读 `prefers-color-scheme`。
- `WordCard` 翻面加 CSS `transition`/`transform`（纯样式，无需 JS）。

---

## 阶段 B：学习功能增强

### B1. 生词本（收藏）
- 迁移新增 `starred(word_id INTEGER PRIMARY KEY, added_at INTEGER, note TEXT DEFAULT '')`（进度库迁移框架 A1 的版本 2）。
- `progress_repo.rs`：`toggle_star/is_starred/list_starred/remove_star`。
- `components/word_detail.rs`：标题旁加星标按钮（★/☆）。
- `pages/categories.rs` 或新增 `pages/wordbook.rs` + 导航项：生词本列表，可进入「只学生词本」的学习/背诵会话（复用 study/review 的队列逻辑，队列源改为 starred ids）。

### B2. 错词本（强化复习）
- 无需新表：从 `word_progress`（`lapses>0`）+ `review_log`（近期 `rating=Again` 频次）聚合「易错词」。
- `progress_repo.rs`：`weak_word_ids(category_id, now, limit)`。
- 新增「错词强化」入口（词库页或背诵页按钮）：以错词组队列进入 review 会话，评分后正常回写 SRS。

### B3. 拼写 / 听写打字测验
- 新增 `src/pages/quiz.rs` + 导航项「测验」，`state.rs` 的 `Page` 增 `Quiz`，`nav_items()` 扩展。
- 交互：展示 `translation`（+音标，可选自动 TTS 发音，联动阶段 C）→ 用户键入单词 → 即时校验（大小写不敏感、trim）→ 正确/错误反馈；错误计入错词本并可回写一次 `Rating::Again`（可配置）。
- 测验范围：当前选中词库 / 生词本 / 错词本；每轮题数复用 `Settings.words_per_session`。
- `Settings` 增 `quiz_mode`（英译中拼写 / 听音拼写）与 `quiz_auto_tts: bool`。

### B4. 词根背单词
- `scripts/build_dict_db.py`：解析 `data/wordroot.txt` JSON，新增
  - `word_roots(root_key TEXT PK, class TEXT, meaning TEXT, origin TEXT, synonyms TEXT, antonyms TEXT)`
  - `root_words(root_key TEXT, word_id INTEGER, order_index INTEGER, PRIMARY KEY(root_key, word_id))`
  - 仅关联 `words` 表中已存在的 `example` 词；`schema_version` 升 6；`meta` 增 `root_total`。
- `dict_repo.rs`：`roots()/root_words(root_key)/roots_for_word(word_id)`。
- `models.rs`：`WordRoot { root_key, class, meaning, origin, synonyms, antonyms }`。
- 新增 `pages/roots.rs` + 导航「词根」：左侧词根列表（含 `meaning/class`），右侧该词根下聚合词（复用 `WordDetail`）；`WordDetail` 底部展示所属词根（可点击跳转）。

---

## 阶段 C：发音 / TTS（离线，webview SpeechSynthesis）

### C1. TTS 封装
- 新增 `src/tts.rs`：用 Dioxus `document::eval`（桌面 webview）执行 JS `speechSynthesis.speak(new SpeechSynthesisUtterance(text))`，参数含 `lang='en-US'`、`rate`；提供 `speak(text, rate)` 与 `stop()`。
- 首次注入一段 JS 助手（缓存 utterance/voice），避免每次拼字符串；对非桌面平台 no-op（当前仅桌面，保留 `#[cfg]` 兜底）。

### C2. UI 接入
- `components/word_card.rs`、`word_detail.rs`：单词与每条例句旁加「🔊」按钮，点击 `speak`。
- `Settings` 增 `tts_enabled: bool`、`tts_rate: f32`、`tts_autoplay: bool`（翻面/进入测验自动读单词）；设置页加对应控件。
- 验收：点击按钮可离线朗读单词与例句；关闭开关后不触发；语速可调。**不引入网络音频**，保持离线优先。

---

## 阶段 D：SRS 与统计增强

### D1. FSRS 调度（可选，与 SM-2 lite 并存）
- `Cargo.toml` 引入 `fsrs`（open-spaced-repetition 官方 Rust 实现）。
- 迁移新增列（版本 3）：`word_progress` 增 `stability REAL`、`difficulty REAL`、`scheduler INTEGER DEFAULT 0`（0=SM-2,1=FSRS）。
- `src/srs.rs`：抽象 `schedule(progress, rating, now, algo)`；`Settings` 增 `scheduler: Scheduler`（默认保持 SM-2 lite，用户可切 FSRS）。FSRS 分支读写 stability/difficulty，SM-2 分支保持现有逻辑不变。
- 保留并扩展 `srs.rs` 单测：FSRS 首评/复评后 `next_review` 单调性、状态回写、切换算法不崩。

### D2. 统计可视化（全部基于已有 review_log，纯 CSS 无图表库）
- `progress_repo.rs` 新增聚合查询：`review_counts_by_day(range)`、`current_streak()/longest_streak()`、`retention_rate(category_id, now)`（Good+Easy / 总评次）、`due_forecast(next_n_days)`。
- `pages/stats.rs` 扩展：
  - 复习热力图（近 ~26 周日历格，颜色深浅=当日复习数）。
  - 连续打卡（当前/最长 streak）。
  - 记忆保持率（总体 + 分词库）。
  - 未来 7 天到期预测条形图。
- 新增 `components/heatmap.rs`、`components/streak_card.rs` 等展示组件。

---

## 数据库与迁移变更汇总
- 只读词典库：`schema_version` 4 → 6；新增 `words_fts`(FTS5,可选)、`word_roots`、`root_words`；`meta` 增 `root_total`。旧库缺表时 `dict_repo` 需**优雅降级**（参照现有 `get_examples` 对缺表返回空的做法）。
- 可写进度库：引入 `PRAGMA user_version` 迁移；版本 2 = `starred`；版本 3 = FSRS 列。所有新增用 `IF NOT EXISTS`/`ALTER TABLE ADD COLUMN` 保证幂等。

## 依赖变更
- 新增：`fsrs`（阶段 D，可选）。
- 不新增：TTS 走 webview JS；热力图/图表走纯 CSS；模糊搜索走 FTS5 + Rust 编辑距离，不引第三方搜索库。

## 测试计划
- 阶段 A：迁移框架单测（空库/旧库升级）；`search` FTS5 与回退路径单测（含 typo、释义全文、无 FTS5 模拟）；快捷键/主题手动验证。
- 阶段 B：`starred`/错词聚合/词根查询的 `dict_repo`/`progress_repo` 单测；测验判分逻辑单测；词根入库后抽样校验（`build_dict_db.py` 打印 `root_total`）。
- 阶段 C：TTS 手动验证（朗读/关闭/语速）；非桌面 no-op 编译校验。
- 阶段 D：FSRS 调度单测（与 SM-2 并存、切换不崩）；统计聚合查询单测（streak/retention/热力图分桶边界）。
- 全程：`cargo test` 全绿、`cargo build --release` 零警告、对 134MB 真实库运行无 panic。

## 假设
1. 仍仅桌面端；Web/移动端不在本计划范围（保留 `web` feature 现状不动）。
2. TTS 采用 webview `speechSynthesis`，离线、不引网络音频、不加原生依赖。
3. FSRS 作为**可选**调度器，默认仍为 SM-2 lite，避免打断现有用户记忆曲线。
4. FTS5 以运行时/构建时探测为准，任一不可用则自动回退到 LIKE + 编辑距离，功能不缺失。
5. 阶段可独立交付；若需调整优先级，A（尤其 A1 迁移框架）建议最先落地。
6. `data/wordroot.txt` 遵循 ECDICT MIT 授权，可随构建入库与分发。