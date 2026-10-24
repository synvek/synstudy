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
                                 [--with-translate]

`--with-translate` appends an (unhashed) appendix listing the optional
`translate` feature's code/model components; see TRANSLATE_APPENDIX below.
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

# Optional appendix for builds that enable the `translate` feature. It lists the
# code/model components behind screenshot translation: inference engines that
# are statically linked (MNN, llama.cpp) and models the user downloads at
# runtime (PaddleOCR / HY-MT weights, never redistributed). It is appended
# OUTSIDE the hashed data body, so notices_version stays tied to the bundled
# dictionary data and a default (data-only) run still produces a byte-identical
# file.
TRANSLATE_APPENDIX = "\n".join([
    LINE,
    " 可选「翻译」功能组件（仅在以 --features translate 构建时适用）",
    LINE,
    "",
    "以下组件与上面的词典数据许可相互独立，且不计入 notices_version 哈希：",
    "  · MNN            —— OCR 推理引擎（经 rust-paddle-ocr 静态链接）   Apache-2.0",
    "  · PaddleOCR      —— PP-OCRv5 mobile 检测 / 识别模型（运行时下载） Apache-2.0",
    "  · llama.cpp      —— 翻译模型推理引擎（经 llama-cpp-2 静态链接）   MIT",
    "  · HY-MT1.5-1.8B  —— 翻译模型权重（运行时由用户从本方托管的 ModelScope 镜像",
    "                      下载量化版本，不随安装包分发）适用《腾讯 HY 社区许可",
    "                      协议》（非标准开源许可，排除欧盟/英国/韩国；首次下载前",
    "                      应用内展示要点并取得点击同意），全文见 HY-MT-LICENSE.txt",
    "",
    "推理引擎随程序静态链接；模型文件均不随安装包分发，由用户在运行时从官方源",
    "（PaddleOCR）或本方托管镜像（HY-MT 量化版）下载并做完整性校验。完整条款文本",
    "见仓库 license/ 子目录（MNN-NOTICE.txt / PaddleOCR-NOTICE.txt /",
    "llama-cpp-NOTICE.txt / HY-MT-NOTICE.txt / HY-MT-LICENSE.txt）。",
    "",
])


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


def render(readme_text, meta, with_translate=False):
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
    # notices_version covers the DATA body only, so the optional translate
    # appendix (code/model components, never bundled data) can be added without
    # changing the hash recorded in the DB meta table.
    version = hashlib.sha256(body.encode("utf-8")).hexdigest()[:12]
    appendix = TRANSLATE_APPENDIX if with_translate else ""
    footer = [
        LINE,
        "notices_version: {}  （声明正文的 sha256 前 12 位；同步记录于数据库 meta 表）".format(version),
        "generated: {}  by scripts/gen_notices.py（本行不参与哈希）".format(
            datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")),
        "",
    ]
    return version, body + appendix + "\n".join(footer)


def generate_from_conn(conn, readme_path, out_path, with_translate=False):
    """Render notices using live meta stats; returns notices_version."""
    meta = dict(conn.execute("SELECT key, value FROM meta"))
    with open(readme_path, "r", encoding="utf-8") as f:
        version, text = render(f.read(), meta, with_translate)
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(text)
    return version


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--db", default=os.path.join(ROOT, "assets", "synstudy.db"))
    ap.add_argument("--readme", default=os.path.join(ROOT, "license", "README.md"))
    ap.add_argument("--out", default=os.path.join(ROOT, "assets", "THIRD-PARTY-NOTICES.txt"))
    ap.add_argument("--with-translate", action="store_true",
                    help="append the optional `translate` feature components "
                         "(MNN / PaddleOCR / llama.cpp / HY-MT); not part of notices_version")
    args = ap.parse_args()
    if not os.path.exists(args.db):
        sys.exit("ERROR: {} not found (run scripts/build_dict_db.py first)".format(args.db))
    conn = sqlite3.connect("file:{}?mode=ro".format(args.db), uri=True)
    try:
        version = generate_from_conn(conn, args.readme, args.out, args.with_translate)
    finally:
        conn.close()
    print("Wrote {} (notices_version {})".format(args.out, version))


if __name__ == "__main__":
    main()
