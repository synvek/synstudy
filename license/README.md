# 外部词典数据许可证

本项目**代码**独立许可，但打包的词典数据库 `assets/synstudy.db` 收录了以下**外部词典数据**。
各数据源遵循其原始许可证，完整条款文本保存在本目录下。

| 数据源 | 本项目使用的内容 | 许可证 | 条款文件 |
| --- | --- | --- | --- |
| [ECDICT](https://github.com/skywind3000/ECDICT) | `words` 表：英汉释义、音标、词形变化、词频、考试标签 | MIT | [`ECDICT-LICENSE.txt`](./ECDICT-LICENSE.txt) |
| [CC-CEDICT](https://www.mdbg.net/chinese/dictionary?page=cc-cedict) | `zh_entries` 表：汉英词典（中文主键 / 拼音 / 繁体 / 英文释义） | CC-BY-SA 4.0 | [`CC-CEDICT-LICENSE.txt`](./CC-CEDICT-LICENSE.txt) |
| [Wiktionary / kaikki.org](https://kaikki.org/dictionary/English/) | `examples` 表：分级词库词汇的自然例句 | CC BY-SA 4.0 | [`Wiktionary-kaikki-LICENSE.txt`](./Wiktionary-kaikki-LICENSE.txt) |
| [Tatoeba](https://tatoeba.org/) | `sentence_bank` / `sentence_words` 表：按考纲分级的例句（四/六/专四/专八/托福/GRE） | CC-BY 2.0 FR | [`Tatoeba-LICENSE.txt`](./Tatoeba-LICENSE.txt) |
| [CEFR-J](https://github.com/openlanguageprofiles/olp-en-cefrj) | `words.cefr` 列：A1–C2 难度等级标注 | 免费商用**须引用**（C1/C2 部分 CC BY-SA 4.0） | [`CEFR-J-NOTICE.txt`](./CEFR-J-NOTICE.txt) |
| [qwerty-learner](https://github.com/RealKai42/qwerty-learner) | 小学（人教版 PEP）分级词表 | GPL-3.0 | [`qwerty-learner-LICENSE.txt`](./qwerty-learner-LICENSE.txt) |
| [kajweb/dict](https://github.com/kajweb/dict) | 专四 / 专八词表（**仅英文单词**） | 无开源许可证（见下方说明） | 无 |

## 合规说明

### ECDICT（MIT）
Copyright (c) 2025 Linwei。MIT 许可，允许自由使用、修改与再分发，仅需保留版权与许可声明。

### CC-CEDICT（CC-BY-SA 4.0）
- **署名（BY）**：词典详情面板与本文档均标注来源为 CC-CEDICT。
- **相同方式共享（SA）**：`assets/synstudy.db` 中 `zh_entries` 表属于 CC-CEDICT 的演绎数据，
  若对外分发该数据库，须以 **CC-BY-SA 4.0** 相同许可共享。
- ⚠️ SA 条款仅约束**数据**，本项目**源代码**不受其传染性影响，仍独立许可。

### qwerty-learner（GPL-3.0）
小学 PEP 词表来自 GPL-3.0 项目。本项目仅将其用于**本地构建**个人词典库；
若要再分发应用，请自行核实 GPL-3.0 的合规义务。

### Wiktionary / kaikki.org（CC BY-SA 4.0）
`examples` 表的例句由 kaikki.org 从 Wiktionary 提取，属 CC BY-SA 4.0。
- **署名（BY）**：词典详情面板例句处标注「例句来自 Wiktionary（CC BY-SA 4.0）」。
- **相同方式共享（SA）**：若对外分发含例句的 `assets/synstudy.db`，该演绎数据须以 CC BY-SA 4.0 共享；SA 仅约束**数据**，不影响本项目源码。

### Tatoeba（CC-BY 2.0 FR）
`sentence_bank` / `sentence_words` 表的分级例句来自 Tatoeba 众包语料库，以及其提供的跨语言链接（英文↔中文）。句子版权归各自贡献者，以 CC-BY 2.0 FR 发布。
- **署名（BY）**：词典详情面板与测验卡片均标注「句子来自 Tatoeba（CC-BY 2.0 FR）」。
- **修改声明**：synstudy 对 Tatoeba 句子做了长度/词库覆盖/考纲词命中的筛选，并通过 Tatoeba 自己的 links 表拼接中文译文。
- **与真题无关**：`exam_levels` 标签完全基于开放 ECDICT 考纲元数据自行推导，**不复制**任何受版权保护的 CET-4/6、TEM-4/8、TOEFL、GRE 真题内容。

### CEFR-J（须引用署名；C1/C2 为 CC BY-SA 4.0）
`words.cefr` 等级来自 CEFR-J 词库档案，版权归属东京外国语大学户田研。
- 可免费用于研究与**商用**，前提是**正确引用**数据集（详见 `CEFR-J-NOTICE.txt`）。
- 其中 C1/C2 补充（Octanove）以 CC BY-SA 4.0 发布，再分发时同样适用数据层面的 SA 义务。

### kajweb/dict（无许可证）
该仓库**未提供开源许可证**，且数据源自有道（专有）。因此本项目：
- **仅提取其中的英文单词（`headWord`）** 作为专四 / 专八的选词与排序依据；
- **不复制**其释义、例句、音标等专有内容——这些一律改由 MIT 许可的 ECDICT 提供。

单词本身属事实性数据，不构成受版权保护的表达。若需正式再分发，建议改用官方
专四 / 专八大纲词表或其他明确开放的词表，替换 `data/dicts/tem4.json`、`tem8.json`。
