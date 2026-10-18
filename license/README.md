# 外部词典数据许可证

本项目**代码**独立许可，但打包的词典数据库 `assets/synstudy.db` 收录了以下**外部词典数据**。
各数据源遵循其原始许可证，完整条款文本保存在本目录下。

| 数据源 | 本项目使用的内容 | 许可证 | 条款文件 |
| --- | --- | --- | --- |
| [ECDICT](https://github.com/skywind3000/ECDICT) | `words` 表：英汉释义、音标、词形变化、词频、考试标签 | MIT | [`ECDICT-LICENSE.txt`](./ECDICT-LICENSE.txt) |
| [CC-CEDICT](https://www.mdbg.net/chinese/dictionary?page=cc-cedict) | `zh_entries` 表：汉英词典（中文主键 / 拼音 / 繁体 / 英文释义） | CC-BY-SA 4.0 | [`CC-CEDICT-LICENSE.txt`](./CC-CEDICT-LICENSE.txt) |
| [Wiktionary / kaikki.org](https://kaikki.org/dictionary/English/) | `examples` 表：分级词库词汇的自然例句；`word_relations` 表：近义词/反义词 | CC BY-SA 4.0 | [`Wiktionary-kaikki-LICENSE.txt`](./Wiktionary-kaikki-LICENSE.txt) |
| [WordNet](https://wordnet.princeton.edu/) | `word_relations` 表：近义/反义词补充（synset 同集成员与 `!` 反义指针） | Princeton WordNet 许可（BSD 风格，可自由商用，保留声明即可） | [`WORDNET-LICENSE.txt`](./WORDNET-LICENSE.txt) |
| [Tatoeba](https://tatoeba.org/) | `sentence_bank` / `sentence_words` 表：按考纲分级的例句（四/六/考研/托福/雅思/GRE） | CC-BY 2.0 FR | [`Tatoeba-LICENSE.txt`](./Tatoeba-LICENSE.txt) |
| [OpenCC](https://github.com/BYVoid/OpenCC) | 繁→简转换表：仅用于构建阶段将 `sentence_bank` 中文译文转为简体 | Apache-2.0 | [`OpenCC-LICENSE.txt`](./OpenCC-LICENSE.txt) |
| [CEFR-J](https://github.com/openlanguageprofiles/olp-en-cefrj) | `words.cefr` 列：A1–C2 难度等级标注 | 免费商用**须引用**（C1/C2 部分 CC BY-SA 4.0） | [`CEFR-J-NOTICE.txt`](./CEFR-J-NOTICE.txt) |

## 合规说明

### ECDICT（MIT）
Copyright (c) 2025 Linwei。MIT 许可，允许自由使用、修改与再分发，仅需保留版权与许可声明。

### CC-CEDICT（CC-BY-SA 4.0）
- **署名（BY）**：词典详情面板与本文档均标注来源为 CC-CEDICT。
- **相同方式共享（SA）**：`assets/synstudy.db` 中 `zh_entries` 表属于 CC-CEDICT 的演绎数据，
  若对外分发该数据库，须以 **CC-BY-SA 4.0** 相同许可共享。
- ⚠️ SA 条款仅约束**数据**，本项目**源代码**不受其传染性影响，仍独立许可。

### Wiktionary / kaikki.org（CC BY-SA 4.0）
`examples` 表的例句由 kaikki.org 从 Wiktionary 提取，属 CC BY-SA 4.0；`word_relations` 表中
`source='Wiktionary'` 的近义/反义词同样出自该数据（senses 级 synonyms/antonyms 字段），适用相同的 BY/SA 义务。
- **署名（BY）**：词典详情面板例句处标注「例句来自 Wiktionary（CC BY-SA 4.0）」，近义/反义词处标注来源。
- **相同方式共享（SA）**：若对外分发含例句的 `assets/synstudy.db`，该演绎数据须以 CC BY-SA 4.0 共享；SA 仅约束**数据**，不影响本项目源码。

### WordNet 3.1（Princeton license，BSD 风格）
`word_relations` 表中 `source='WordNet'` 的行由构建脚本解析 WordNet 3.1 词典文件（dict 包，经 NLTK wordnet31 镜像获取）生成：synset 成员作为近义词，形容词 `!` 指针目标作为反义词。
- 许可允许**自由使用、修改与再分发（含商用）**，只需在所有副本（含演绎数据）中保留 Princeton 版权与许可声明（见 `WORDNET-LICENSE.txt`）。
- 声明体现：本文件与 THIRD-PARTY-NOTICES.txt 均列出 WordNet 条目；应用内近义/反义词处标注「来自 Wiktionary 与 WordNet」。
- 与 CC 许可不同，WordNet 许可**无 SA 传染性**，仅要求保留声明。

### Tatoeba（CC-BY 2.0 FR）
`sentence_bank` / `sentence_words` 表的分级例句来自 Tatoeba 众包语料库，以及其提供的跨语言链接（英文↔中文）。句子版权归各自贡献者，以 CC-BY 2.0 FR 发布。
- **署名（BY）**：词典详情面板与测验卡片均标注「句子来自 Tatoeba（CC-BY 2.0 FR）」。
- **修改声明**：synstudy 对 Tatoeba 句子做了长度/词库覆盖/考纲词命中的筛选，并通过 Tatoeba 自己的 links 表拼接中文译文；中文译文另经 OpenCC 表做了繁体→简体的字形转换。
- **与真题无关**：`exam_levels` 标签完全基于开放 ECDICT 考纲元数据自行推导，**不复制**任何受版权保护的 CET-4/6、TEM-4/8、TOEFL、IELTS、GRE 真题内容。

### OpenCC（Apache-2.0）
Tatoeba 的中文译文以繁体为主，构建时使用 OpenCC 的 `TSCharacters` / `TSPhrases` 映射表转为简体。
- 转换表仅在**构建阶段**使用，映射数据本身不会打包进 `assets/synstudy.db`（库中只存转换后的句子文本）。
- Apache-2.0 允许自由使用与再分发，保留许可声明即可（见 `OpenCC-LICENSE.txt`）。

### CEFR-J（须引用署名；C1/C2 为 CC BY-SA 4.0）
`words.cefr` 等级来自 CEFR-J 词库档案，版权归属东京外国语大学户田研。
- 可免费用于研究与**商用**，前提是**正确引用**数据集（详见 `CEFR-J-NOTICE.txt`）。
- 其中 C1/C2 补充（Octanove）以 CC BY-SA 4.0 发布，再分发时同样适用数据层面的 SA 义务。

## 可选「翻译」功能组件（`translate` feature）

以下组件与上面的词典**数据**许可相互独立：它们**仅在以 `--features translate`
构建时**静态链接进程序（推理引擎），或其模型权重在**运行时由用户从官方源
自行下载**（synstudy 不随安装包再分发任何模型文件）。因此它们**不参与**
`THIRD-PARTY-NOTICES.txt` —— 该文件只声明实际入库的词典数据源（见
`scripts/gen_notices.py` 的说明）；翻译构建可选地以 `--with-translate` 追加一段
组件声明，且该附录不计入 `notices_version` 哈希。

| 组件 | 用途 | 许可证 | 条款文件 |
| --- | --- | --- | --- |
| [MNN](https://github.com/alibaba/MNN) | OCR 推理引擎（经 `rust-paddle-ocr` 静态链接） | Apache-2.0 | [`MNN-NOTICE.txt`](./MNN-NOTICE.txt) |
| [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | PP-OCRv5 mobile 检测 / 识别模型（运行时下载，约 28 MB） | Apache-2.0 | [`PaddleOCR-NOTICE.txt`](./PaddleOCR-NOTICE.txt) |
| [llama.cpp](https://github.com/ggerganov/llama.cpp) | 翻译模型推理引擎（经 `llama-cpp-2` 静态链接） | MIT | [`llama-cpp-NOTICE.txt`](./llama-cpp-NOTICE.txt) |
| [HY-MT1.5-1.8B](https://github.com/Tencent-Hunyuan/HY-MT) | 翻译模型权重（运行时由用户从官方源下载，约 980 MB，不再分发） | 以官方模型卡为准（腾讯混元社区许可） | [`HY-MT-NOTICE.txt`](./HY-MT-NOTICE.txt) |

- **代码组件（MNN / llama.cpp）**：Apache-2.0 与 MIT 均允许自由使用、修改与再
  分发（含商用），保留版权与许可声明即可；完整条款见根目录
  `LICENSE-APACHE.txt` / `LICENSE-MIT.txt`。
- **模型权重（PaddleOCR / HY-MT）**：**不随包分发**，仅由用户在运行时从官方源
  下载，规避再分发合规风险。HY-MT 的使用须遵守腾讯混元官方许可条款（可能含
  商用规模阈值等限制），下载前请在官方模型卡阅读并同意。
- 应用内「设置 → 关于 → 版权与许可」在启用翻译功能时会额外列出上述组件及其
  官方链接。
