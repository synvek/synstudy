#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Build the read-only SQLite dictionary database for synstudy.

Inputs (place under data/):
  data/ecdict.csv            ECDICT master dictionary (UTF-8). Source:
                             https://github.com/skywind3000/ECDICT  (MIT)
  data/dicts/*.json          qwerty-learner style word lists. Source:
                             https://github.com/RealKai42/qwerty-learner

Output:
  assets/synstudy.db         Bundled with the app, opened read-only at runtime.

Usage:
  python3 scripts/build_dict_db.py            # full build
  python3 scripts/build_dict_db.py --limit-words 50000   # smaller test build

If data/ is empty, run `python3 scripts/gen_sample_data.py` first to generate a
small sample dataset for development.
"""
import argparse
import csv
import glob
import os
import sqlite3
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Category definition:
#   key     stable identifier used by the app
#   name    display name (Chinese)
#   source  "tag"  -> selected from ECDICT `tag` column (ordered by frequency)
#           "json" -> selected from qwerty-learner JSON lists (original order)
#   tag     ECDICT tag token (source == "tag")
#   globs   candidate JSON filename patterns (source == "json"), tried in order
CATEGORIES = [
    {"key": "xiaoxue",  "name": "小学",         "source": "json",
     "globs": ["xiaoxue.json", "PEPXiaoXue*.json"]},
    {"key": "chuzhong", "name": "初中",         "source": "tag", "tag": "zk"},
    {"key": "gaozhong", "name": "高中",         "source": "tag", "tag": "gk"},
    {"key": "cet4",     "name": "大学英语四级", "source": "tag", "tag": "cet4"},
    {"key": "cet6",     "name": "大学英语六级", "source": "tag", "tag": "cet6"},
    {"key": "tem4",     "name": "英语专业四级", "source": "json",
     "globs": ["tem4.json", "TEM4*.json"]},
    {"key": "tem8",     "name": "英语专业八级", "source": "json",
     "globs": ["tem8.json", "TEM8*.json"]},
    {"key": "toefl",    "name": "托福",         "source": "tag", "tag": "toefl"},
    {"key": "gre",      "name": "GRE",         "source": "tag", "tag": "gre"},
]

SCHEMA = """
CREATE TABLE words (
    id          INTEGER PRIMARY KEY,
    word        TEXT NOT NULL UNIQUE,
    phonetic    TEXT DEFAULT '',
    translation TEXT DEFAULT '',
    definition  TEXT DEFAULT '',
    pos         TEXT DEFAULT '',
    tag         TEXT DEFAULT '',
    bnc         INTEGER DEFAULT 0,
    frq         INTEGER DEFAULT 0,
    exchange    TEXT DEFAULT ''
);
CREATE INDEX idx_words_word ON words(word);

CREATE TABLE categories (
    id         INTEGER PRIMARY KEY,
    key        TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    word_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE category_words (
    category_id INTEGER NOT NULL,
    word_id     INTEGER NOT NULL,
    order_index INTEGER NOT NULL,
    PRIMARY KEY (category_id, word_id)
);
CREATE INDEX idx_cw_order ON category_words(category_id, order_index);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT
);
"""


def to_int(v):
    try:
        return int(str(v).strip())
    except (ValueError, TypeError):
        return 0


def load_ecdict(conn, csv_path, limit):
    """Stream ECDICT csv into the words table. Returns count inserted."""
    if not os.path.exists(csv_path):
        sys.exit("ERROR: {} not found.\n"
                 "Run scripts/gen_sample_data.py for a sample, or download the "
                 "real ECDICT csv.".format(csv_path))
    cur = conn.cursor()
    n = 0
    with open(csv_path, "r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            word = (row.get("word") or "").strip()
            if not word:
                continue
            cur.execute(
                "INSERT OR IGNORE INTO words "
                "(word, phonetic, translation, definition, pos, tag, bnc, frq, exchange) "
                "VALUES (?,?,?,?,?,?,?,?,?)",
                (word,
                 row.get("phonetic", "") or "",
                 (row.get("translation", "") or "").replace("\\n", "\n"),
                 (row.get("definition", "") or "").replace("\\n", "\n"),
                 row.get("pos", "") or "",
                 row.get("tag", "") or "",
                 to_int(row.get("bnc")),
                 to_int(row.get("frq")),
                 row.get("exchange", "") or ""))
            n += 1
            if limit and n >= limit:
                break
    conn.commit()
    return n


def ensure_word(conn, word, phonetic, translation):
    """Return word_id, inserting a minimal entry if the word is unknown."""
    cur = conn.cursor()
    cur.execute("SELECT id FROM words WHERE word = ?", (word,))
    row = cur.fetchone()
    if row:
        return row[0]
    cur.execute(
        "INSERT INTO words (word, phonetic, translation) VALUES (?,?,?)",
        (word, phonetic or "", translation or ""))
    conn.commit()
    return cur.lastrowid


def word_id_map(conn):
    cur = conn.cursor()
    cur.execute("SELECT id, word FROM words")
    return {w: i for i, w in cur.fetchall()}


def resolve_json_files(data_dir, globs):
    found = []
    for pat in globs:
        matches = sorted(glob.glob(os.path.join(data_dir, "dicts", pat)))
        for m in matches:
            if m not in found:
                found.append(m)
    return found


def build_json_category(conn, cat, data_dir, ids):
    """Insert category_words for a JSON-sourced category, preserving order."""
    import json
    files = resolve_json_files(data_dir, cat["globs"])
    if not files:
        print("  [warn] {}: no JSON files matched {}".format(cat["key"], cat["globs"]))
        return 0
    cur = conn.cursor()
    order_index = 0
    seen = set()
    for path in files:
        with open(path, "r", encoding="utf-8") as f:
            items = json.load(f)
        for it in items:
            name = (it.get("name") or "").strip()
            if not name or name in seen:
                continue
            seen.add(name)
            trans = it.get("trans") or []
            if isinstance(trans, list):
                trans = "\n".join(str(t) for t in trans)
            phone = it.get("usphone") or it.get("ukphone") or ""
            if phone and not phone.startswith("/"):
                phone = "/" + phone + "/"
            wid = ids.get(name)
            if wid is None:
                wid = ensure_word(conn, name, phone, trans)
                ids[name] = wid
            cur.execute(
                "INSERT OR IGNORE INTO category_words "
                "(category_id, word_id, order_index) VALUES (?,?,?)",
                (cat["rowid"], wid, order_index))
            order_index += 1
    conn.commit()
    return order_index


def build_tag_category(conn, cat, ids):
    """Insert category_words for a tag-sourced category, ordered by frequency."""
    tag = cat["tag"]
    cur = conn.cursor()
    # match the tag as a whole token within the space-separated tag column
    cur.execute(
        "SELECT id FROM words "
        "WHERE (' ' || tag || ' ') LIKE ? "
        "ORDER BY CASE WHEN frq > 0 THEN frq ELSE 99999999 END, "
        "         CASE WHEN bnc > 0 THEN bnc ELSE 99999999 END, word",
        ("% " + tag + " %",))
    rows = cur.fetchall()
    for order_index, (wid,) in enumerate(rows):
        cur.execute(
            "INSERT OR IGNORE INTO category_words "
            "(category_id, word_id, order_index) VALUES (?,?,?)",
            (cat["rowid"], wid, order_index))
    conn.commit()
    return len(rows)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data-dir", default=os.path.join(ROOT, "data"))
    ap.add_argument("--out", default=os.path.join(ROOT, "assets", "synstudy.db"))
    ap.add_argument("--limit-words", type=int, default=0,
                    help="limit number of ECDICT words (0 = all)")
    args = ap.parse_args()

    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    if os.path.exists(args.out):
        os.remove(args.out)

    conn = sqlite3.connect(args.out)
    conn.executescript(SCHEMA)

    csv_path = os.path.join(args.data_dir, "ecdict.csv")
    if not os.path.exists(csv_path):
        alt = os.path.join(args.data_dir, "stardict.csv")
        if os.path.exists(alt):
            csv_path = alt
    print("Loading ECDICT words from {} ...".format(csv_path))
    n = load_ecdict(conn, csv_path, args.limit_words)
    print("  inserted {} word rows".format(n))

    ids = word_id_map(conn)
    cur = conn.cursor()
    print("Building categories ...")
    for cat in CATEGORIES:
        cur.execute("INSERT INTO categories (key, name) VALUES (?,?)",
                    (cat["key"], cat["name"]))
        cat["rowid"] = cur.lastrowid
    conn.commit()

    for cat in CATEGORIES:
        if cat["source"] == "json":
            count = build_json_category(conn, cat, args.data_dir, ids)
        else:
            count = build_tag_category(conn, cat, ids)
        cur.execute("UPDATE categories SET word_count = ? WHERE id = ?",
                    (count, cat["rowid"]))
        conn.commit()
        print("  {:<10} {:<8} {:>6} words".format(cat["key"], cat["name"], count))

    cur.execute("SELECT COUNT(*) FROM words")
    total_words = cur.fetchone()[0]
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("word_total", str(total_words)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("schema_version", "1"))
    conn.commit()
    conn.close()

    size_mb = os.path.getsize(args.out) / (1024 * 1024)
    print("\nWrote {} ({:.1f} MB), {} words total.".format(args.out, size_mb, total_words))


if __name__ == "__main__":
    main()
