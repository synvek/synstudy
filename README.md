# synstudy

一个使用 **Rust + Dioxus** 构建的**纯客户端、离线优先**桌面英语学习工具（macOS / Windows / Linux）。

内置**英汉 / 汉英双向词典**与**分级词库**（初中 / 高中 / 四级 / 六级 / 托福 / GRE 开箱即用；小学 / 专四 / 专八 需自备官方大纲词表选装构建），支持**学习模式**与基于**间隔重复（SRS）**的**背诵 / 复习模式**，并记录本地学习进度与统计。

> 词典数据经一次性构建脚本预处理为**只读 SQLite**，随应用打包；用户学习进度写入本地**可写 SQLite**（应用数据目录）。**无后端、无网络依赖**。

---

## 功能特性

- **英汉词典查询**：前缀 / 精确检索，展示音标、中文释义、英文释义、词性、词形变化（exchange）、考试标签、词频（BNC / 柯林斯）；并展示 **CEFR 难度等级**（来自 CEFR-J）与 **自然例句**（来自 Wiktionary，仅分级词库词汇）。
- **CEFR 难度标注**：基于 **CEFR-J** 词库档案为单词标注 A1–C2 等级，详情面板以徽章展示，可用于按难度筛选 / 分级学习。
- ** Wiktionary 自然例句**：由 **kaikki.org** 从 Wiktionary 提取的真实例句，针对分级词库词汇入库（每词最多 3 句），在详情面板展示并附来源出处。
- **考试分级句库**（四/六/专四/专八/托福/GRE）：从 **Tatoeba** 开放句库（CC-BY 2.0 FR）提取自然句，按 ECDICT 考纲标签自动打上 `exam_levels`，可选拼接 Tatoeba 自己的中↔英链接作为参考译文（并经 **OpenCC** 将繁体转为简体）。支持三种测验题型：**选词填空**（Cloze）、**句意选择**（中文释义四选一）、**听音辨词**（TTS + 四选一）。词详情面板同步展示考试句库例句。
  - ⚠️ **不包含真题**：CET-4/6、TEM-4/8、TOEFL、GRE 真题受版权保护，本项目**不分发也不爬取**任何真题内容；句库完全由开放语料构成，仅按考纲词汇标记级别。
- **汉英词典查询**：基于 **CC-CEDICT**（真正以中文词为主键的汉英词典），支持**中文 / 拼音 / 繁体**检索，按「精确词条 → 前缀 → 子串」排序，覆盖成语、复合词与专有名词；详情面板可将英文释义**回链到英汉词典**（点击查看音标 / 词频 / 考试标签）。词典页顶部一键切换英汉 / 汉英方向。
- **分级词库浏览**：默认 6 大分类（提供官方大纲词表后最多 9 类），显示词条数与学习进度百分比。
- **学习模式**：按词库原始顺序逐词卡片学习，翻面查看释义，标记「认识 / 不认识」，进度实时持久化。
- **背诵 / 复习模式（SRS）**：SM-2 lite 算法，四档自评（重来 / 困难 / 良好 / 轻松）自动排期，仅拉取到期词组成复习队列。
- **进度统计**：各词库未学 / 学习中 / 复习中 / 已掌握分布。
- **设置**：每轮学习词数、正序 / 倒序开关、进度重置、数据路径查看。

---

## 技术栈

| 组件 | 说明 |
| --- | --- |
| [Dioxus](https://dioxuslabs.com/) `0.7` | 声明式跨平台 UI（`desktop` feature） |
| [rusqlite](https://crates.io/crates/rusqlite) `0.32` (`bundled`) | 静态编译 SQLite，跨平台免系统依赖 |
| [serde](https://crates.io/crates/serde) / serde_json | 源数据解析与本地配置序列化 |
| [directories](https://crates.io/crates/directories) | 定位可写的用户数据目录（进度库） |
| [chrono](https://crates.io/crates/chrono) | SRS 复习排期时间计算 |

---

## 项目结构

```
synstudy/
├── Cargo.toml
├── Dioxus.toml               # 桌面窗口配置 + assets 资源目录
├── assets/
│   ├── main.css              # 全局样式（构建时内联注入）
│   └── synstudy.db           # 构建产物：只读词典库（打包进应用）
├── data/                     # 源数据（git 忽略大文件）
│   ├── ecdict.csv            # ECDICT 主词典（英汉）
│   ├── cedict.txt            # CC-CEDICT 汉英词典（中文主键）
│   ├── kaikki.en.jsonl       # Wiktionary/kaikki.org 提取（例句，~3.3GB，仅本地构建用）
│   ├── cefrj/*.csv           # CEFR-J 词库档案（A1-C2 等级标注）
│   ├── wordroot.txt          # ECDICT 词根词缀（后期词根功能预留）
│   └── dicts/*.json          # 分级词表（自备官方大纲词表：小学 / 专四 / 专八）
├── license/                  # 外部词典数据的许可证条款文本 + 合规说明
├── scripts/
│   ├── fetch_data.sh         # 一键下载全部源数据（遵循 http(s)_proxy）
│   ├── gen_sample_data.py    # 生成样例源数据（完全离线时的回退方案）
│   └── build_dict_db.py      # 数据管线：CSV + JSON -> assets/synstudy.db
└── src/
    ├── main.rs               # dioxus::launch(App)
    ├── app.rs                # 根组件 + 导航壳 + 全局 Context Provider
    ├── models.rs             # Word / ZhEntry / Category / WordProgress / ProgressStatus
    ├── state.rs              # Page / Settings / 时间工具
    ├── srs.rs                # SM-2 lite 间隔重复算法（含单元测试）
    ├── db/
    │   ├── mod.rs            # 词典库路径解析 + 进度库路径
    │   ├── dict_repo.rs      # 只读词典库查询
    │   └── progress_repo.rs  # 可写进度库（建表 / 迁移 / 读写）
    ├── components/           # NavBar / WordCard / WordDetail / ProgressBar / CategoryTile
    └── pages/                # dictionary / categories / study / review / stats / settings
```

---

## 快速开始

### 环境要求

- Rust 1.80+（`rustup` 安装）
- Python 3.8+（用于数据构建脚本）
- 可选：[`dx` CLI](https://dioxuslabs.com/learn/0.7/getting_started)（`cargo install dioxus-cli`）用于打包

### 1. 准备词典数据

**方式 A — 一键抓取真实数据（推荐）**

仓库提供了 `scripts/fetch_data.sh`，自动下载下列数据源到 `data/`。若处于需要代理的网络环境，先导出 `http_proxy` / `https_proxy`（curl 会自动遵循）：

```bash
export http_proxy=http://127.0.0.1:7890
export https_proxy=http://127.0.0.1:7890
bash scripts/fetch_data.sh
```

抓取内容：

| 数据 | 来源 | 用途 |
| --- | --- | --- |
| `ecdict.csv`（~63MB，77 万词条）+ `wordroot.txt` | [ECDICT](https://github.com/skywind3000/ECDICT)（MIT） | 主词典：音标 / 双解释义 / 词形变化 / 词频 / 考试标签 |
| `cedict.txt`（~10MB，12.5 万词条） | [CC-CEDICT](https://www.mdbg.net/chinese/dictionary?page=cc-cedict)（CC-BY-SA 4.0） | **汉英词典**：中文主键 + 拼音 + 繁体 + 英文释义 |
| `kaikki.en.jsonl`（~3.3GB，构建时仅过滤分级词库） | [Wiktionary / kaikki.org](https://kaikki.org/dictionary/English/)（CC BY-SA 4.0） | **自然例句**：提取分级词库词汇的例句入 `examples` 表 |
| `tatoeba/en_sentences.tsv.bz2`（~50MB）+ `cmn_sentences.tsv.bz2`（~10MB）+ `links.tsv`（~200MB） | [Tatoeba](https://tatoeba.org/)（CC-BY 2.0 FR） | **考试分级句库**：`sentence_bank` / `sentence_words` 表，按考纲词自动标记四/六/专四/专八/托福/GRE |
| `opencc/TSCharacters.txt`、`TSPhrases.txt`（~250KB） | [OpenCC](https://github.com/BYVoid/OpenCC)（Apache-2.0） | 构建阶段将 Tatoeba 中文译文由**繁体转为简体**（仅存转换后文本，不入库映射表） |
| `cefrj/*.csv`（~280KB） | [CEFR-J](https://github.com/openlanguageprofiles/olp-en-cefrj)（**须引用署名**，C1/C2 为 CC BY-SA 4.0） | **难度等级**：为 `words.cefr` 标注 A1–C2 |

> 初中 / 高中 / 四级 / 六级 / 托福 / GRE 直接由 ECDICT 的 `tag` 字段（`zk/gk/cet4/cet6/toefl/gre`）生成，无需额外下载。
>
> ⚠️ **小学 / 专四 / 专八词表不再自动下载**：原数据源 qwerty-learner（GPL-3.0 传染风险）与 kajweb/dict（无开源许可证）已因合规原因移除。如需这三个分类，请自行整理**官方大纲词表**为 `data/dicts/xiaoxue.json` / `tem4.json` / `tem8.json`（格式 `[{"name": "word", ...}]`，仅需 `name` 字段选词，释义一律取自 ECDICT）；未提供时对应分类会被构建脚本自动跳过。
>
> 若不想下载 ~200MB 的 Tatoeba links 文件，可设 `SKIP_TATOEBA_LINKS=1` 后再跑 `fetch_data.sh`；句库仍会构建，仅不包含中文参考译文（不影响选词填空 / 听音辨词）。中文译文依赖 `data/opencc/` 的繁→简转换表；若缺失则译文保留原始字形（多为繁体）。

**方式 B — 生成样例数据（完全离线 / 快速试用）**

若无法访问上述数据源，可生成一份小体量样例词库（9 分类各 20 词）：

```bash
python3 scripts/gen_sample_data.py
```

### 2. 构建词典数据库

```bash
python3 scripts/build_dict_db.py
```

脚本会读取 `data/` 下的源数据，产出只读词典库 `assets/synstudy.db`，并打印各分类词条数校验。

使用完整 ECDICT + CC-CEDICT + CEFR-J + Wiktionary 构建的典型结果（约 134MB）：

```
  inserted 770611 word rows
  chuzhong   初中         1603 words
  gaozhong   高中         3677 words
  cet4       大学英语四级     3849 words
  cet6       大学英语六级     5407 words
  toefl      托福         6974 words
  gre        GRE        7504 words
  xiaoxue    小学          (skipped: no word list)   # 提供官方大纲词表后自动构建
  tem4       英语专业四级     (skipped: no word list)
  tem8       英语专业八级     (skipped: no word list)
  inserted 125215 Chinese-English entries   # 汉英 (zh_entries)
  8845 annotations, 8804 words tagged        # CEFR-J 等级 (A1-C2)
  inserted 43948 example sentences           # Wiktionary 例句（覆盖 16229 词）
```

常用参数：

| 参数 | 说明 |
| --- | --- |
| `--data-dir <路径>` | 源数据目录（默认 `data`） |
| `--out <路径>` | 输出数据库路径（默认 `assets/synstudy.db`） |
| `--limit-words <N>` | 限制导入 `words` 表词条数（控制体积） |

> **体积权衡**：76 万词条全量约 150–250MB。如打包体积敏感，用 `--limit-words` 或仅保留分级词表涉及的词。

### 3. 运行

```bash
# 使用 cargo
cargo run --release

# 或使用 dx CLI
dx serve --platform desktop   # 开发热重载
```

首次启动会在系统应用数据目录创建可写进度库：

- **macOS**：`~/Library/Application Support/com.synstudy.synstudy/progress.db`
- **Linux**：`~/.local/share/com.synstudy.synstudy/progress.db`
- **Windows**：`%APPDATA%\com.synstudy.synstudy\progress.db`

> 词典库查找顺序：环境变量 `SYNSTUDY_DICT_DB` → 可执行文件同目录 → `./assets/synstudy.db` → `./synstudy.db`。

### 4. 打包发布

```bash
dx build --release          # 编译 release 二进制
dx bundle --release         # 生成平台安装包（.app / .msi / .deb 等）
```

---

## 测试

```bash
cargo test                  # SRS 排期算法单元测试
```

覆盖：四档评分后的间隔增长、`Again` 重置、掌握阈值、ease 上下限、每轮词数上限等。

---

## 数据库设计

**只读词典库 `assets/synstudy.db`**

- `words(id, word UNIQUE, phonetic, translation, definition, pos, tag, bnc, frq, exchange, cefr)` — `cefr` 为 CEFR-J 标注的 A1–C2 等级
- `categories(id, key UNIQUE, name, word_count)`
- `category_words(category_id, word_id, order_index)` — 保留原词库顺序，主键 `(category_id, word_id)`
- `zh_entries(id, traditional, simplified, pinyin, pinyin_plain, glosses)` — CC-CEDICT 汉英（中文主键），simplified/traditional/pinyin_plain 建索引
- `examples(id, word_id, text, source, order_index)` — Wiktionary（kaikki.org）自然例句，仅针对分级词库词汇，按 `word_id` 建索引
- `sentence_bank(id, text, translation, source, source_id, cefr_min, exam_levels, difficulty, word_count)` — Tatoeba 分级句库；`exam_levels` 为逗号分隔的考纲标签（`cet4,cet6,tem4,tem8,toefl,gre`），`difficulty` 为 1..=5 的推导难度层。建索引：`exam_levels` / `difficulty` / `cefr_min`。
- `sentence_words(sentence_id, word_id, token_index, token_text, is_target, PRIMARY KEY(sentence_id, token_index))` — 句子↔词反向链接；`is_target = 1` 标记考纲词，供选词填空/听音辨词抽题。

**可写进度库 `<appdata>/progress.db`**（首启建表 + 版本迁移）

- `word_progress(word_id, category_id, status, reps, lapses, ease, interval_days, last_review, next_review, PRIMARY KEY(word_id, category_id))`
- `settings(key, value)` — 学习偏好（JSON）
- `review_log(word_id, category_id, reviewed_at, rating)` — 复习流水（统计用）

---

## 数据来源与授权

> 各外部词典的**完整许可证条款文本**已收录于 [`license/`](./license/) 目录，详见 [`license/README.md`](./license/README.md)。

- **ECDICT**（`github.com/skywind3000/ECDICT`，**MIT**）：主词典，提供音标 / 中英双解 / 词形变化 / 词频 / 考试标签（`zk/gk/cet4/cet6/ky/toefl/ielts/gre`）。本项目的英汉释义与音标内容均来源于此。
- **CC-CEDICT**（`www.mdbg.net`，**CC-BY-SA 4.0**）：汉英词典数据源，以中文词为主键，提供简体 / 繁体 / 拼音 / 英文释义（约 12.5 万条）。
  - ⚠️ CC-BY-SA 4.0 要求**署名**并对**数据演绎作品采用相同许可共享**。本项目在词典详情中标注来源；打包的 `assets/synstudy.db` 中 `zh_entries` 表数据须以 CC-BY-SA 4.0 共享（应用**代码**不受此约束，仍独立许可）。
- **Wiktionary / kaikki.org**（`kaikki.org`，**CC BY-SA 4.0**）：`examples` 表的自然例句由 kaikki.org 从 Wiktionary 提取（仅保留分级词库涉及词的例句）。详情面板标注来源；若对外分发含例句的数据库，该演绎数据须以 CC BY-SA 4.0 共享（同样仅约束数据）。
- **Tatoeba**（`tatoeba.org`，**CC-BY 2.0 FR**）：`sentence_bank` / `sentence_words` 表的分级例句与英中译文。句子版权归各自贡献者，使用时需署名并声明修改；本项目已在词详情与测验卡片标注来源，详见 [`license/Tatoeba-LICENSE.txt`](./license/Tatoeba-LICENSE.txt)。**不复制任何受版权保护的真题内容**，`exam_levels` 标签仅基于开放 ECDICT 考纲元数据自行推导。
- **OpenCC**（`github.com/BYVoid/OpenCC`，**Apache-2.0**）：中文译文以繁体为主，构建时用其 `TSCharacters` / `TSPhrases` 映射表转为简体。映射表仅在构建阶段使用，不打包进数据库；详见 [`license/OpenCC-LICENSE.txt`](./license/OpenCC-LICENSE.txt)。
- **CEFR-J**（`github.com/openlanguageprofiles/olp-en-cefrj`，**免费商用须引用**；C1/C2 部分 **CC BY-SA 4.0**）：`words.cefr` 的 A1–C2 等级来源，版权归东京外国语大学户田研；使用时须正确引用数据集（详见 `license/CEFR-J-NOTICE.txt`）。
- **小学 / 专四 / 专八词表**：原数据源 **qwerty-learner**（GPL-3.0 传染风险）与 **kajweb/dict**（无开源许可证）相关数据已**全部移除**，项目不再下载、构建或分发其内容。如需这三个分类，自行整理官方大纲词表放入 `data/dicts/`；释义与音标一律由 MIT 许可的 ECDICT 提供。
- 本仓库默认生成的 `data/` 样例数据仅用于离线试用，非官方词表。

---

## 路线图（MVP 之后）

- **倒序学习**：已实现（设置页「倒序学习」开关，学习模式按 `order_index` 反向取词）；后续可扩展到复习队列排序。
- **词根背单词**：接入 ECDICT `wordroot.txt`，新增 `word_roots` 表与按词根聚合的学习视图。
- **发音 / TTS**：接入系统 TTS 或在线音频（ECDICT `audio` 字段预留）。
- **拼写 / 听写测验**、**生词本**、**错词本强化复习**。
- **Web / 移动端目标**：数据层与 UI 已平台无关，后续加 `web`/`mobile` feature，词典库改为静态资源加载，进度库改用平台存储（Web 用 IndexedDB 封装）。

---

## 许可

代码部分可自由使用。词典数据遵循其各自来源的许可（英汉 ECDICT 为 MIT；汉英 CC-CEDICT 为 CC-BY-SA 4.0，其数据演绎须相同方式共享；Wiktionary/kaikki.org 例句为 CC BY-SA 4.0；Tatoeba 句库为 CC-BY 2.0 FR；繁→简转换表 OpenCC 为 Apache-2.0；CEFR-J 等级免费商用但须引用署名，其 C1/C2 部分为 CC BY-SA 4.0）。原 qwerty-learner（GPL-3.0）与 kajweb/dict（无许可）词表已因合规原因移除，不再随项目分发。完整条款文本见 [`license/`](./license/) 目录。
