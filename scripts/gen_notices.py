#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Generate assets/THIRD-PARTY-NOTICES.txt from license/README.md.

Only the data sources that actually landed in the built database are declared:
presence is decided by the `meta` table that build_dict_db.py writes
(word_total / zh_total / example_total / sentence_total / sentence_trans_total /
cefr_tagged / root_total / wordnet_total). The sha256 prefix of the rendered body is returned
as `notices_version` and stored back into the same meta table, so a released
bundle can always prove its notices match the data it ships:

  * add a new source to the build but forget license/README.md  -> gen_notices
    prints a "[warn] undeclared source" and the version changes;
  * edit license/README.md (attribution wording, licence swap)  -> the hash in
    meta no longer matches the shipped DB until a rebuild/regeneration.

build_dict_db.py calls generate_from_conn() at the end of every build; this
script can also be run standalone against an existing DB:

  python3 scripts/gen_notices.py [--db assets/synstudy.db]
                                 [--readme license/README.md]
                                 [--out assets/THIRD-PARTY-NOTICES.txt]
"""
import argparse
import datetime
import hashlib
import os
import re
import sqlite3
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

LINE = "=" * 72

# (source key, match terms against normalized license/README.md names,
#  meta keys whose values prove the source was actually built in)
SOURCES = [
    ("ECDICT",     ["ecdict"],                  ["word_total", "root_total"]),
    ("CC-CEDICT",  ["cc-cedict"],               ["zh_total"]),
    ("Wiktionary", ["wiktionary"],              ["example_total"]),
    ("WordNet",    ["wordnet"],                 ["wordnet_total"]),
    ("Tatoeba",    ["tatoeba"],                 ["sentence_total"]),
    ("OpenCC",     ["opencc"],                  ["sentence_trans_total"]),
    ("CEFR-J",     ["cefr-j"],                  ["cefr_tagged"]),
]


def _norm(name):
    """`'Wiktionary / kaikki.org（CC BY-SA 4.0）'` -> `'wiktionary / kaikki.org'`."""
    return re.split(r"[（(]", name)[0].strip().lower()


def _strip_md(cell):
    """`'[`X.txt`](./X.txt)'` -> 'X.txt'; markdown links fall back to text."""
    return re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", cell).strip()


def parse_readme(text):
    """Extract {name_norm: table_row_cells} and {name_norm: section_body}."""
    rows = {}
    for line in text.splitlines():
        if line.startswith("| ["):
            name = _norm(line[3:line.index("]")])
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) >= 4:
                rows[name] = cells
    sections = {}
    for m in re.finditer(r"^### (.+?)$[ \t]*\n(.*?)(?=^### |\Z)",
                         text, flags=re.M | re.S):
        sections[_norm(m.group(1))] = m.group(2).rstrip()
    return rows, sections


def _meta_int(meta, key):
    try:
        return int(meta.get(key, "0"))
    except ValueError:
        return 0


def used_sources(meta):
    """Source keys present in the build, with a stat summary line."""
    out = []
    for key, terms, meta_keys in SOURCES:
        counts = {k: _meta_int(meta, k) for k in meta_keys}
        if any(v > 0 for v in counts.values()):
            stat = ", ".join("{}={}".format(k, v) for k, v in counts.items())
            out.append((key, terms, stat))
    return out


def render(readme_text, meta):
    rows, sections = parse_readme(readme_text)
    included = used_sources(meta)

    header = [
        LINE,
        " synstudy — 第三方数据声明 / THIRD-PARTY NOTICES",
        LINE,
        "",
        "本应用代码许可：MIT OR Apache-2.0（见安装包根目录 LICENSE-MIT / LICENSE-APACHE）。",
        "应用随附的词典数据库 assets/synstudy.db 含以下第三方数据的演绎成果；",
        "各数据完整条款文本见安装包/仓库的 license/ 子目录。",
        "",
        "全部数据均可由开源构建管线从原始数据源重建：",
        "    scripts/fetch_data.sh  +  scripts/build_dict_db.py",
        "本声明由 scripts/gen_notices.py 从 license/README.md 自动生成，仅列出",
        "本次构建实际入库的数据源（依据数据库 meta 统计判定），与库内数据一一对应。",
        "CC-BY-SA 4.0 演绎数据（汉英词典 / Wiktionary 例句 / CEFR-J C1-C2）通过上述",
        "开源管线以相同许可提供，任何人可自行重建获取。",
        "",
    ]

    body_lines = []
    warnings = []
    for idx, (key, terms, stat) in enumerate(included, 1):
        row = next((rows[n] for n in rows
                    if any(n.startswith(t) for t in terms)), None)
        sec = next((sections[n] for n in sections
                    if any(n.startswith(t) for t in terms)), None)
        if row is None:
            warnings.append("数据源 {} 已入库但 license/README.md 未声明，请补充！".format(key))
        body_lines.append("")
        body_lines.append("-" * 72)
        body_lines.append("[{}] {}".format(idx, key))
        body_lines.append("-" * 72)
        if row:
            body_lines.append("本项目使用的内容：{}".format(_strip_md(row[1])))
            body_lines.append("许可证：          {}".format(_strip_md(row[2])))
            body_lines.append("条款文件：        {}".format(_strip_md(row[3])))
        if sec:
            body_lines.append("")
            body_lines.extend(sec.splitlines())
        body_lines.append("")
        body_lines.append("（本次构建统计：{}）".format(stat))

    if warnings:
        body_lines.append("")
        body_lines.append(LINE)
        body_lines.append("⚠️ 生成警告（请修复 license/README.md 后重新生成）：")
        body_lines.extend("  - " + w for w in warnings)

    body = "\n".join(header + body_lines + [""])
    version = hashlib.sha256(body.encode("utf-8")).hexdigest()[:12]
    footer = [
        LINE,
        "notices_version: {}  （声明正文的 sha256 前 12 位；同步记录于数据库 meta 表）".format(version),
        "generated: {}  by scripts/gen_notices.py（本行不参与哈希）".format(
            datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")),
        "",
    ]
    return version, body + "\n".join(footer)


def generate_from_conn(conn, readme_path, out_path):
    """Render notices using live meta stats; returns notices_version."""
    meta = dict(conn.execute("SELECT key, value FROM meta"))
    with open(readme_path, "r", encoding="utf-8") as f:
        version, text = render(f.read(), meta)
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(text)
    return version


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--db", default=os.path.join(ROOT, "assets", "synstudy.db"))
    ap.add_argument("--readme", default=os.path.join(ROOT, "license", "README.md"))
    ap.add_argument("--out", default=os.path.join(ROOT, "assets", "THIRD-PARTY-NOTICES.txt"))
    args = ap.parse_args()
    if not os.path.exists(args.db):
        sys.exit("ERROR: {} not found (run scripts/build_dict_db.py first)".format(args.db))
    conn = sqlite3.connect("file:{}?mode=ro".format(args.db), uri=True)
    try:
        version = generate_from_conn(conn, args.readme, args.out)
    finally:
        conn.close()
    print("Wrote {} (notices_version {})".format(args.out, version))


if __name__ == "__main__":
    main()
