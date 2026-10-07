# synstudy 英语学习工具开发计划

## 摘要
使用 Rust + Dioxus 0.7 构建**纯客户端桌面应用**（macOS/Windows/Linux），**离线优先、暂不做后端**。词典数据经一次性构建脚本预处理为**只读 SQLite** 内嵌打包；用户学习进度写入本地**可写 SQLite**（应用数据目录）。MVP 覆盖：英汉词典查询、分级词库浏览、学习模式、背诵/复习模式(SRS)、进度统计。后期扩展（倒序学习、词根背单词、发音、Web/移动端）作为路线图预留。

## 数据来源与授权
- **ECDICT**（`github.com/skywind3000/ECDICT`，MIT）：主词典。`ecdict.csv`（76万词条，仓库内）或 release 的 `stardict.7z`（完整版）。字段：`word/phonetic/definition/translation/pos/collins/oxford/tag/bnc/frq/exchange`。其中 `tag` 提供考试分级：`zk`(中考)、`gk`(高考)、`cet4`、`cet6`、`ky`(考研)、`toefl`、`ielts`、`gre`。附带 `wordroot.txt`(词根词缀 JSON，用于后期功能)。
- **qwerty-learner**（`github.com/RealKai42/qwerty-learner`，`public/dicts/*.json`）：补齐 ECDICT **未覆盖**的分级——**专四(TEM4)、专八(TEM8)、小学/初中/高中(人教版 PEP)**。JSON 格式：`{name, trans, usphone, ukphone}`。
  - 预期文件：`TEM4_T.json`/`TEM8_T.json`、`PEPXiaoXue*.json`、`PEPChuZhong*.json`、`PEPGaoZhong*.json`、`GaoKao_3500.json`（实施时以仓库实际文件名为准）。
  - **待办确认**：实施前核对 qwerty-learner 的 LICENSE 是否允许再分发其词表数据。若受限，回退方案：CET4/CET6/TOEFL/GRE/中考/高考直接用 ECDICT `tag` 生成；仅 TEM4/TEM8/小学需要另找开放词表或用大纲词表生成。

## 分级词库映射（category）
统一 9+ 类，每类为**有序词表**（保留原词库顺序，支持后期倒序）：
小学 / 初中 / 高中(含高考3500) / 四级(cet4) / 六级(cet6) / 专四(tem4) / 专八(tem8) / 托福(toefl) / GRE。
每个词通过 word 关联 ECDICT 主词典获取音标、双解释义、词性、词形变化、词频。

## 技术选型
- Dioxus `0.7`（`features = ["desktop"]`），`dx` CLI 已装(0.7.3)，Rust 1.93.1 已装。
- `rusqlite { features = ["bundled"] }`：静态编译 SQLite，跨平台免依赖。
- `serde` + `serde_json`：解析源数据与本地配置。
- `directories`：定位可写的用户数据目录（进度库）。
- `chrono`：SRS 复习排期时间计算。
- 导航：基于信号的枚举式页面切换（`use_signal<Page>`），避免桌面端路由复杂度；如需可后续换 `dioxus-router`。
- 样式：`assets/main.css`（Dioxus 0.7 自动 CSS 注入）。

## 项目结构
```
synstudy/
├── Cargo.toml
├── Dioxus.toml
├── assets/
│   ├── main.css
│   └── synstudy.db          # 构建产物：只读词典库(打包进应用)
├── data/                    # 源数据(git 忽略大文件)：ecdict.csv / qwerty json
├── scripts/
│   └── build_dict_db.py     # 一次性数据管线：CSV+JSON -> assets/synstudy.db
├── .gitignore
└── src/
    ├── main.rs              # dioxus::launch(App)
    ├── app.rs               # 根组件 + 导航壳 + 全局 Provider
    ├── db/
    │   ├── mod.rs
    │   ├── dict_repo.rs     # 只读词典库查询(查词/分类词表)
    │   └── progress_repo.rs # 可写进度库(学习记录/SRS/建表迁移)
    ├── models.rs            # Word, Category, WordProgress, ReviewState
    ├── state.rs             # AppSettings/Session 全局信号
    ├── srs.rs               # 间隔重复算法(SM-2 lite / Leitner)
    ├── components/          # SearchBar, WordCard, WordDetail, NavBar, ProgressBar, CategoryTile
    └── pages/
        ├── dictionary.rs    # 词典查询
        ├── categories.rs    # 词库浏览/选择
        ├── study.rs         # 学习模式
        ├── review.rs        # 背诵/复习模式
        ├── stats.rs         # 进度统计
        └── settings.rs      # 设置
```

## 数据库设计
**只读词典库 `assets/synstudy.db`**
- `words(id INTEGER PK, word TEXT UNIQUE, phonetic, translation, definition, pos, tag, bnc INT, frq INT, exchange)`
  - 建索引：`word`；可选 FTS5 虚表 `words_fts` 支持模糊/前缀查询。
- `categories(id INTEGER PK, key TEXT UNIQUE, name TEXT, word_count INT)`
- `category_words(category_id, word_id, order_index)` — 保留原词库顺序，主键 `(category_id, word_id)`。

**可写进度库 `<appdata>/synstudy/progress.db`**（首启建表 + 版本迁移）
- `word_progress(word_id, category_id, status, box_level INT, ease REAL, reps INT, lapses INT, last_review, next_review, PRIMARY KEY(word_id, category_id))`
- `settings(key, value)` — 学习偏好（每轮词数、正序/倒序、发音开关等）。
- `review_log(word_id, category_id, reviewed_at, rating)`（可选，用于统计）。

## 数据构建管线 (`scripts/build_dict_db.py`)
1. 读取 `data/ecdict.csv`（UTF-8），写入 `words` 表（丢弃空的 `detail/audio` 字段控制体积）。
2. 读取分级来源：
   - ECDICT `tag` 生成 cet4/cet6/toefl/gre/zk(初中)/gk(高中) 词表（按 `frq`/`bnc` 词频排序）。
   - qwerty-learner JSON 生成 小学/初中/高中/tem4/tem8 词表（保留原顺序 `order_index`）。
3. 每个词表去重、按 word 关联 `words.id`，写入 `categories` + `category_words`。
4. 输出到 `assets/synstudy.db`，打印各分类词条数校验。
5. **体积权衡**：76万词条全量约 150–250MB。提供开关：`--mini` 用 `ecdict.mini.csv` 或仅导入 9 类词表涉及的词 + 常用词子集，显著减小体积（默认先全量，按需切换）。

## 实施阶段

### 阶段 0：脚手架与数据
- `dx new` 初始化项目（desktop 模板），改名为 `synstudy`，清理模板代码。
- 配置 `Cargo.toml`（dioxus desktop / rusqlite bundled / serde / directories / chrono）。
- 下载 ECDICT + qwerty-learner 数据到 `data/`（git 忽略），编写并跑通 `build_dict_db.py`，产出 `assets/synstudy.db`。
- 验收：脚本输出各分类词条数，SQLite 可查询到示例词（含音标/双解/tag）。

### 阶段 1：数据访问层
- 实现 `db/dict_repo.rs`：按 word 精确/前缀查询、按 category 取有序词表（含分页）、分类列表与计数。
- 实现 `db/progress_repo.rs`：首启创建/迁移进度库，读写 `word_progress`、`settings`。
- 实现 `models.rs`、`state.rs`（全局只读词典句柄 + 可写进度句柄通过 Dioxus Context 注入）。
- 验收：单元测试查询与进度读写通过。

### 阶段 2：词典查询页 (MVP 核心)
- `pages/dictionary.rs` + `components/SearchBar`、`WordDetail`：输入即时检索，展示音标、中文释义、英文释义、词性、词形变化(exchange)、考试标签、词频。
- 顶部 `NavBar` + `app.rs` 枚举导航壳，`assets/main.css` 基础样式。
- 验收：桌面运行可查词并展示完整词条。

### 阶段 3：词库浏览 + 学习模式
- `pages/categories.rs` + `CategoryTile`：展示 9 类词库、词条数、学习进度百分比。
- `pages/study.rs` + `WordCard`：按 `order_index` 逐词卡片学习，点击/空格翻面显示释义，标记「认识/不认识」，写入进度；支持设置每轮词数。
- 验收：选分类→学习→进度持久化，重启后进度保留。

### 阶段 4：背诵/复习模式 (SRS)
- `srs.rs`：SM-2 lite（Again/Hard/Good/Easy 四档自评，更新 `box_level/ease/next_review`）。
- `pages/review.rs`：拉取到期(`next_review <= now`)词组成复习队列，自评后排期，展示今日待复习/已掌握数量。
- `pages/stats.rs` + `ProgressBar`：各分类新学/学习中/已掌握统计。
- 验收：复习队列按 SRS 排期正确，重进只出现到期词。

### 阶段 5：打磨与打包
- `pages/settings.rs`：每轮词数、正序/倒序开关、发音占位、数据导出/重置。
- 三平台 `dx build --release` 打包验证（macOS 优先）。
- 补充 `README`（数据构建步骤、运行方式、授权说明）。

## 路线图（MVP 之后，预留设计不实现）
- **倒序学习**：`category_words.order_index` 已存序，仅需 study/review 读取顺序开关（settings 已预留）。
- **词根背单词**：接入 ECDICT `wordroot.txt`，新增 `word_roots` 表与按词根聚合的学习视图。
- **发音/TTS**：接入系统 TTS 或在线音频（ECDICT `audio` 字段预留）；桌面端调用平台语音 API。
- **拼写/听写测验**、**生词本**、**错词本强化复习**。
- **Web / 移动端目标**：数据层与 UI 已平台无关，后续加 `web`/`mobile` feature，词典库改为静态资源加载，进度库改用平台存储（Web 用 IndexedDB 封装）。

## 假设
1. MVP 仅桌面端；三平台以 macOS 为首要验证目标。
2. 词典数据一次性构建、随应用打包，运行时只读；不做在线更新。
3. 背诵采用自评式 SRS（SM-2 lite），非打字测验。
4. 默认全量 ECDICT；若打包体积敏感，切换 `--mini` 构建。
5. qwerty-learner 词表授权在阶段 0 核实，受限时按回退方案处理。

## 测试计划
- 数据管线：校验各分类词条数、抽样词条字段完整性（音标/双解/tag）。
- 数据层：`cargo test` 覆盖 dict_repo 查询与 progress_repo 读写/迁移。
- SRS：`srs.rs` 单元测试验证四档评分后的 `next_review` 排期。
- 端到端手动验证：查词、学习一轮、复习到期词、重启后进度保留、三平台 release 打包启动。