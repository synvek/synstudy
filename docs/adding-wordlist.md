# 新增分级词库指南（以「四级核心词汇」为例）

本文说明如何准备一份自整理词表，并把它作为新的分级词库分类接入 synstudy。
适用于任何「不在 ECDICT `tag` 标签体系内、需要自定义清单」的场景，例如
CET-4 高频核心词、按教材单元整理的词表等。

## 管线总览

```
pro/words/<词库名>/vocabulary/*.md     ① 人工精选的 md 词表（词条行格式固定）
        │
        │ ② md → json 转换（噪音过滤）
        ▼
data/dicts/<key>.json                  ③ JSON 词表（qwerty-learner 兼容格式）
        │
        │ ④ 在 scripts/build_dict_db.py 的 CATEGORIES 注册 source="json" 分类
        ▼
assets/synstudy.db                     ⑤ 构建（全量重建 或 增量写入现有库）
   categories / category_words 表
        │
        │ ⑥ Rust 侧补磁贴与短码（可选，不补也能显示，走默认回退）
        ▼
词库页 / 学习页可见
```

两种分类来源的差异（见 `build_dict_db.py` 的 `CATEGORIES` 注释）：

| 来源 | 排序语义（`order_index`） | 文件缺失时 |
| --- | --- | --- |
| `"tag"` | 从 ECDICT `tag` 列取词，按字母序 A→Z | 不适用 |
| `"json"` | 按 JSON 文件的**原始条目顺序**（即词表正序） | 该分类自动跳过 |

应用的「正序 / 倒序」直接读取 `category_words.order_index`，随机顺序在运行期洗牌，
与构建无关。**词表在 JSON 里的排列顺序就是学习正序**，转 JSON 前请按期望的学习顺序排好。

## 第 1 步：准备 md 词表

在 `pro/words/<词库名>/vocabulary/` 下按字母段拆分 md 文件（参照现有
[CET-4 词库](../pro/words/CET-4/vocabulary/A-G.md)），词条行格式为：

```markdown
- **ability** /əˈbɪləti/ *n.* 能力；才能
- **access** /ˈækses/ *n.* 接近；机会；*v.* 访问；获取
```

规则要点：

- 行首为 `- **`，词头为纯 ASCII 单词（可含空格 / 撇号 / 连字符，如 `a lot of`、`don't`）；
- 词头后必须紧跟 `/音标/`，再接 `*词性*` 与中文释义；
- 词条下的子项（`  - 例 1：...`，两格缩进）不会被解析为词条行；
- 中文小结 bullet、写作句型模板等噪音行因「不含纯 ASCII 词头 + 音标」会被自然过滤；
- 同一词条的拼写变体（如 `distil / distill`）应拆成两条独立词条。

每个词库目录建议配一个 `index.md` 说明收录范围与词数，便于后续维护。

## 第 2 步：md → JSON 转换

输出到 `data/dicts/<key>.json`，格式为 qwerty-learner 词表风格（与现有
`cet4_high.json` 一致）：

```json
[
  {
    "name": "ability",
    "trans": ["n. 能力；才能"],
    "usphone": "əˈbɪləti",
    "ukphone": "əˈbɪləti"
  }
]
```

字段说明（决定构建行为的只有 `name`，其余为展示兜底）：

- `name`：词条拼写，构建时优先在 ECDICT `words` 表中精确匹配（大小写敏感），
  命中则复用其音标 / 释义 / 词频等全量信息；未命中才以 `trans` / `usphone` 兜底插入新词；
- `trans`：中文释义列表，多条以换行拼接；
- `usphone` / `ukphone`：美 / 英音标，不带斜杠（构建器会自动补 `/.../`）；
- 重复的 `name` 会被跳过（以首次出现为准），顺序即 `order_index`。

可直接使用的最小转换脚本范例（保存为临时脚本运行，或按喜好固化进 `scripts/`）：

```python
#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 pro/words/<词库名>/vocabulary/*.md 转成 data/dicts/<key>.json。"""
import json, os, re, sys

ENTRY = re.compile(
    r"^- \*\*([A-Za-z][A-Za-z' \-]*)\*\*"   # 纯 ASCII 词头
    r"\s*/([^/]+)/"                          # /音标/（必须存在，过滤噪音行）
    r"\s+\*(.+?)\*\s*(.+)$"                  # *词性* 释义（可含多词性）
)

def convert(md_paths, out_path):
    items, seen = [], set()
    for path in md_paths:
        with open(path, encoding="utf-8") as f:
            for line in f:
                m = ENTRY.match(line.rstrip())
                if not m:
                    continue
                word, phone, pos, gloss = m.groups()
                word = word.strip().lower()
                # 拼写变体（"distil / distill" 之类词头）拆成独立词
                for name in [w.strip().lower() for w in word.split("/") if w.strip()]:
                    if not name or name in seen or "..." in gloss:
                        continue
                    seen.add(name)
                    items.append({
                        "name": name,
                        "trans": ["{}. {}".format(pos.strip(), gloss.strip())],
                        "usphone": phone.strip(),
                        "ukphone": phone.strip(),
                    })
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(items, f, ensure_ascii=False, indent=2)
    print("wrote {} entries -> {}".format(len(items), out_path))

if __name__ == "__main__":
    # 用法：python3 convert_md.py data/dicts/cet4_core.json pro/words/CET-4/vocabulary/A-G.md ...
    out, mds = sys.argv[1], sys.argv[2:]
    convert(sorted(mds), out)
```

运行示例（以四级高频核心词为例，输入现有的 CET-4 md 目录）：

```bash
python3 convert_md.py data/dicts/cet4_high.json \
    pro/words/CET-4/vocabulary/A-G.md \
    pro/words/CET-4/vocabulary/H-N.md \
    pro/words/CET-4/vocabulary/O-T.md \
    pro/words/CET-4/vocabulary/U-Z.md
```

## 第 3 步：在 CATEGORIES 注册分类

编辑 `scripts/build_dict_db.py`，在 `CATEGORIES` 列表中新增一项（key 是小写
稳定标识，应用侧进度表以它为准）：

```python
CATEGORIES = [
    ...
    {"key": "cet4",     "name": "大学英语四级", "source": "tag", "tag": "cet4"},
    {"key": "cet4_high", "name": "四级高频精选", "source": "json",
     "globs": ["cet4_high.json"]},                          # ← 新增
    ...
]
```

要点：

- `globs` 是 `data/dicts/` 下的候选文件名模式，按顺序尝试、可配多个文件合并成一个分类；
- 对应 JSON 不存在时构建只打印 `[warn]` 并跳过该分类，不影响其余构建；
- 分类的展示名 `name` 直接写入 `categories.name`，词库卡片标题即为此值。

> **现状提示**：`data/dicts/` 下已生成 `cet4_high.json` / `cet6_high.json` /
> `ky1_high.json` / `ky2_high.json` 四份词表，但当前 `CATEGORIES` 尚未注册对应
> 条目，所以应用内不可见。接入「四级核心词汇」只需完成本步及之后的第 4、5 步。

## 第 4 步：构建入库

**方式 A — 全量重建**（改动多个分类、或首次跑通管线时用；约 8~10 分钟）：

```bash
python3 scripts/build_dict_db.py          # 输出 assets/synstudy.db
```

**方式 B — 增量写入现有库**（只加词库分类、不动 240MB 整库时用；幂等，可重复执行）：

```python
import json, sqlite3

DB, KEY, NAME = "assets/synstudy.db", "cet4_high", "四级高频精选"
conn = sqlite3.connect(DB)
cur = conn.cursor()
ids = dict(cur.execute("SELECT id, word FROM words"))

cur.execute("INSERT OR IGNORE INTO categories(key, name, word_count) VALUES (?,?,0)", (KEY, NAME))
cat_id = cur.execute("SELECT id FROM categories WHERE key=?", (KEY,)).fetchone()[0]
cur.execute("DELETE FROM category_words WHERE category_id=?", (cat_id,))  # 幂等重跑

order = 0
for it in json.load(open("data/dicts/{}.json".format(KEY), encoding="utf-8")):
    wid = ids.get(it["name"])
    if wid is None:
        print("MISS:", it["name"]); continue      # 自整理词表理论上应全部命中 ECDICT
    cur.execute("INSERT OR IGNORE INTO category_words(category_id, word_id, order_index) VALUES (?,?,?)",
                (cat_id, wid, order)); order += 1

cur.execute("UPDATE categories SET word_count=? WHERE id=?", (order, cat_id))
conn.commit()
print("applied {} words to {}".format(order, KEY))
```

增量方式要求词条**全部命中 `words` 表**（打印 `MISS` 即说明有词不在 ECDICT，
此时应改走全量重建，让 `ensure_word` 兜底插入）。已验证 4 份 `*_high.json`
均可全量命中，增量即可生效。

## 第 5 步：Rust 侧磁贴与短码

词库列表本身从数据库读取，新分类无需加页面代码；但若要融入现有分组视觉，
需在 `src/components/category_tile.rs` 补两处映射：

```rust
// category_visual：图标缩写 / 配色 / 分组标题
"cet4_high" => ("4H", "tone-rose", "高频精选"),
// category_code：学习页概览卡上的短码
"cet4_high" => "CET-4H",
```

不补也不会出错——未知 key 走默认回退 `("词", "tone-indigo", "")` / `"词库"`，
只是分组标题为空。若新增了 `tone-*` 配色类，记得在 `assets/main.css` 补对应变量。

## 第 6 步：验证

```bash
# 1. 数据库层面：分类与词数
python3 -c "import sqlite3;print(sqlite3.connect('assets/synstudy.db').execute(
  'select key,name,word_count from categories').fetchall())"

# 2. order_index 连续无重复（正序完整性）
python3 -c "import sqlite3;cur=sqlite3.connect('assets/synstudy.db').cursor();
cur.execute(\"select min(o),max(o),count(*),count(distinct o) from
 (select order_index o from category_words cw join categories c on c.id=cw.category_id
  where c.key='cet4_high')\");print(cur.fetchone())"

# 3. 编译与运行
cargo check --offline && dx serve
```

进入词库页确认新卡片出现、词数正确，开始学习后前几词应为 JSON 里的原始顺序。

## 许可与合规注意

- `pro/words/` 下的词表为**自整理精选**（基于公开真题统计规律与教辅常见解析重写的
  释义与例句），不引入第三方词表版权风险，无需登记进 `license/README.md` 与
  THIRD-PARTY-NOTICES；
- **禁止**直接导入 GPL 或无明确许可的第三方词表数据入库（项目早期已清理过此类数据，
  见 README「许可合规」章节）；如确需引入新的第三方数据源，须同步更新
  `license/README.md` 表格，`scripts/gen_notices.py` 会在构建末尾自动纳入声明；
- md 中真题例句若为「参考情境改写」而非原文摘录，保留现有 `⚠️` 标注约定。
