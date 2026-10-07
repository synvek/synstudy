# synstudy

一个使用 **Rust + Dioxus** 构建的**纯客户端、离线优先**桌面英语学习工具（macOS / Windows / Linux）。

内置**英汉词典**与**分级词库**（小学 / 初中 / 高中 / 四级 / 六级 / 专四 / 专八 / 托福 / GRE），支持**学习模式**与基于**间隔重复（SRS）**的**背诵 / 复习模式**，并记录本地学习进度与统计。

> 词典数据经一次性构建脚本预处理为**只读 SQLite**，随应用打包；用户学习进度写入本地**可写 SQLite**（应用数据目录）。**无后端、无网络依赖**。

---

## 功能特性

- **英汉词典查询**：前缀 / 精确检索，展示音标、中文释义、英文释义、词性、词形变化（exchange）、考试标签、词频（BNC / 柯林斯）。
- **分级词库浏览**：9 大分类，显示词条数与学习进度百分比。
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
│   ├── ecdict.csv            # ECDICT 主词典
│   ├── wordroot.txt          # ECDICT 词根词缀（后期词根功能预留）
│   └── dicts/*.json          # 分级词库（小学 PEP / 专四 / 专八 等）
├── scripts/
│   ├── fetch_data.sh         # 一键下载全部源数据（遵循 http(s)_proxy）
│   ├── gen_sample_data.py    # 生成样例源数据（完全离线时的回退方案）
│   └── build_dict_db.py      # 数据管线：CSV + JSON -> assets/synstudy.db
└── src/
    ├── main.rs               # dioxus::launch(App)
    ├── app.rs                # 根组件 + 导航壳 + 全局 Context Provider
    ├── models.rs             # Word / Category / WordProgress / ProgressStatus
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
| `PEPXiaoXue*.json`、`GaoKao_3500.json` | [qwerty-learner](https://github.com/RealKai42/qwerty-learner)（GPL-3.0） | 小学（人教版）分级词表 |
| 专四 / 专八词表 | [kajweb/dict](https://github.com/kajweb/dict)（**仅提取英文单词**） | 专四 / 专八选词与词序；释义、音标一律取自 MIT 的 ECDICT |

> 初中 / 高中 / 四级 / 六级 / 托福 / GRE 直接由 ECDICT 的 `tag` 字段（`zk/gk/cet4/cet6/toefl/gre`）生成，无需额外下载。

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

使用完整 ECDICT 构建的典型结果（约 104MB）：

```
  inserted 770611 word rows
  xiaoxue    小学          819 words
  chuzhong   初中         1603 words
  gaozhong   高中         3677 words
  cet4       大学英语四级     3849 words
  cet6       大学英语六级     5407 words
  tem4       英语专业四级     4025 words
  tem8       英语专业八级    12197 words
  toefl      托福         6974 words
  gre        GRE        7504 words
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

- `words(id, word UNIQUE, phonetic, translation, definition, pos, tag, bnc, frq, exchange)`
- `categories(id, key UNIQUE, name, word_count)`
- `category_words(category_id, word_id, order_index)` — 保留原词库顺序，主键 `(category_id, word_id)`

**可写进度库 `<appdata>/progress.db`**（首启建表 + 版本迁移）

- `word_progress(word_id, category_id, status, reps, lapses, ease, interval_days, last_review, next_review, PRIMARY KEY(word_id, category_id))`
- `settings(key, value)` — 学习偏好（JSON）
- `review_log(word_id, category_id, reviewed_at, rating)` — 复习流水（统计用）

---

## 数据来源与授权

- **ECDICT**（`github.com/skywind3000/ECDICT`，**MIT**）：主词典，提供音标 / 中英双解 / 词形变化 / 词频 / 考试标签（`zk/gk/cet4/cet6/ky/toefl/ielts/gre`）。本项目的释义与音标内容均来源于此。
- **qwerty-learner**（`github.com/RealKai42/qwerty-learner`，**GPL-3.0**）：小学（人教版 PEP）分级词表。JSON 格式 `{name, trans, usphone, ukphone}`。
  - ⚠️ GPL-3.0 为传染性许可。本项目仅将其词表用于**本地构建**个人词典库；若要再分发应用，请自行核实 GPL 合规义务。
- **专四 / 专八词表**（`github.com/kajweb/dict`，数据源自有道）：该仓库**无开源许可证**，故本项目**仅提取其中的英文单词（`headWord`）作为选词与排序依据**，不复制其释义 / 例句 / 音标等专有内容——这些一律改由 MIT 的 ECDICT 提供。单词本身属事实性数据。
  - ⚠️ 若需正式再分发，建议改用官方专四 / 专八大纲词表或其他明确开放的词表替换 `data/dicts/tem4.json`、`tem8.json`。
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

代码部分可自由使用。词典数据遵循其各自来源的许可（ECDICT 为 MIT；qwerty-learner 以其仓库 LICENSE 为准）。
