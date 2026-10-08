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
import re
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
    exchange    TEXT DEFAULT '',
    cefr        TEXT DEFAULT ''
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

CREATE TABLE zh_entries (
    id           INTEGER PRIMARY KEY,
    traditional  TEXT NOT NULL,
    simplified   TEXT NOT NULL,
    pinyin       TEXT DEFAULT '',
    pinyin_plain TEXT DEFAULT '',
    glosses      TEXT DEFAULT ''
);
CREATE INDEX idx_zh_simplified ON zh_entries(simplified);
CREATE INDEX idx_zh_traditional ON zh_entries(traditional);
CREATE INDEX idx_zh_pinyin ON zh_entries(pinyin_plain);

CREATE TABLE examples (
    id          INTEGER PRIMARY KEY,
    word_id     INTEGER NOT NULL,
    text        TEXT NOT NULL,
    source      TEXT DEFAULT 'Wiktionary',
    order_index INTEGER DEFAULT 0
);
CREATE INDEX idx_examples_word ON examples(word_id);
"""

# CC-CEDICT line:  Trad Simp [pin1 yin1] /gloss/gloss/
CEDICT_RE = re.compile(r'^(\S+)\s+(\S+)\s+\[([^\]]+)\]\s+/(.*)$')

# CEFR ordering; lower rank == earlier-learnable level.
CEFR_RANK = {"A1": 1, "A2": 2, "B1": 3, "B2": 4, "C1": 5, "C2": 6}


def pinyin_plain(py):
    """Lowercase pinyin with tone numbers/spaces removed, for romaji search."""
    return re.sub(r'[^a-z]', '', py.lower())


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


def build_zh_entries(conn, cedict_path):
    """Parse CC-CEDICT into the zh_entries (真·汉英) table. Returns row count."""
    if not os.path.exists(cedict_path):
        print("  [warn] {} not found; 汉英词典将为空（运行 scripts/fetch_data.sh 下载）".format(cedict_path))
        return 0
    cur = conn.cursor()
    n = 0
    with open(cedict_path, "r", encoding="utf-8") as f:
        for line in f:
            if not line.strip() or line.startswith("#"):
                continue
            m = CEDICT_RE.match(line.rstrip("\n"))
            if not m:
                continue
            trad, simp, py, defs = m.group(1), m.group(2), m.group(3), m.group(4)
            segs = [s.strip() for s in defs.split("/") if s.strip()]
            glosses = "/".join(segs)
            cur.execute(
                "INSERT INTO zh_entries "
                "(traditional, simplified, pinyin, pinyin_plain, glosses) VALUES (?,?,?,?,?)",
                (trad, simp, py, pinyin_plain(py), glosses))
            n += 1
    conn.commit()
    return n


def apply_cefr(conn, paths):
    """Annotate `words.cefr` with the earliest CEFR-J level (A1..C2).

    Reads CEFR-J vocabulary profile CSVs (columns: headword,pos,CEFR,...),
    splits alternative headword spellings on '/', and gives each dictionary
    word the *lowest* (earliest-learnable) level observed. Returns
    (annotations, words_updated).
    """
    best = {}  # lower variant -> rank
    for p in paths:
        if not os.path.exists(p):
            print("  [warn] CEFR file missing: {}".format(p))
            continue
        with open(p, "r", encoding="utf-8-sig", newline="") as f:
            reader = csv.DictReader(f)
            for row in reader:
                hw = (row.get("headword") or "").strip()
                lvl = (row.get("CEFR") or "").strip().upper()
                if not hw or lvl not in CEFR_RANK:
                    continue
                rank = CEFR_RANK[lvl]
                for variant in hw.split("/"):
                    v = variant.strip().lower()
                    if not v:
                        continue
                    if v not in best or rank < best[v]:
                        best[v] = rank
    if not best:
        return 0, 0
    inv = {r: l for l, r in CEFR_RANK.items()}
    cur = conn.cursor()
    by_lower = {}
    cur.execute("SELECT id, word FROM words")
    for wid, w in cur.fetchall():
        key = w.strip().lower()
        by_lower.setdefault(key, wid)
    updated = 0
    for v, rank in best.items():
        wid = by_lower.get(v)
        if wid is None:
            continue
        cur.execute("UPDATE words SET cefr = ? WHERE id = ?", (inv[rank], wid))
        updated += 1
    conn.commit()
    return len(best), updated


# Max example sentences kept per headword (the graded banks are the target).
MAX_EX_PER_WORD = 3


def build_examples(conn, kaikki_path, max_per_word=MAX_EX_PER_WORD):
    """Extract Wiktionary (kaikki.org) example sentences for words that appear
    in the graded category word banks.

    Streams the ~3GB `kaikki.org-dictionary-English.jsonl` and, for each English
    headword that belongs to a study bank, keeps up to `max_per_word` clean
    natural sentences. Restricting to the graded banks (instead of all 770k
    dictionary words) keeps the `examples` table small while covering exactly
    the vocabulary learners study. Returns the number of rows inserted.
    """
    if not os.path.exists(kaikki_path):
        print("  [warn] {} not found; \u4f8b\u53e5\u5c06\u4e3a\u7a7a\uff08\u8fd0\u884c scripts/fetch_data.sh \u4e0b\u8f7d kaikki\uff09".format(kaikki_path))
        return 0
    import json
    cur = conn.cursor()
    # Target = distinct headwords that belong to at least one category.
    cur.execute(
        "SELECT DISTINCT w.id, LOWER(w.word) FROM category_words cw "
        "JOIN words w ON w.id = cw.word_id")
    targets = {}
    for wid, lw in cur.fetchall():
        if lw:
            targets.setdefault(lw, wid)
    if not targets:
        return 0
    kept = {}  # word_id -> count kept so far
    n = 0
    with open(kaikki_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or not line.startswith("{"):
                continue
            try:
                o = json.loads(line)
            except Exception:
                continue
            if o.get("lang") != "English":
                continue
            w = (o.get("word") or "").strip().lower()
            wid = targets.get(w)
            if wid is None or kept.get(wid, 0) >= max_per_word:
                continue
            for sense in o.get("senses", []):
                for ex in sense.get("examples", []):
                    text = (ex.get("text") or "").strip()
                    # Skip short/boilerplate or template-leftover fragments.
                    if len(text) < 8 or "{" in text or "}" in text:
                        continue
                    ref = (ex.get("ref") or "").strip()
                    cur.execute(
                        "INSERT INTO examples (word_id, text, source, order_index) "
                        "VALUES (?,?,?,?)",
                        (wid, text, ref, kept.get(wid, 0)))
                    kept[wid] = kept.get(wid, 0) + 1
                    n += 1
                    if kept[wid] >= max_per_word:
                        break
                if kept.get(wid, 0) >= max_per_word:
                    break
    conn.commit()
    return n


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

    cedict_path = os.path.join(args.data_dir, "cedict.txt")
    print("Building 汉英 (zh_entries) from {} ...".format(cedict_path))
    zh_n = build_zh_entries(conn, cedict_path)
    print("  inserted {} Chinese-English entries".format(zh_n))

    cefr_paths = [
        os.path.join(args.data_dir, "cefrj", "cefrj-vocabulary-profile-1.5.csv"),
        os.path.join(args.data_dir, "cefrj", "octanove-vocabulary-profile-c1c2-1.0.csv"),
    ]
    print("Applying CEFR-J levels ...")
    cefr_seen, cefr_updated = apply_cefr(conn, cefr_paths)
    print("  {} annotations, {} words tagged".format(cefr_seen, cefr_updated))

    kaikki_path = os.path.join(args.data_dir, "kaikki.en.jsonl")
    print("Building examples (Wiktionary) from {} ...".format(kaikki_path))
    ex_n = build_examples(conn, kaikki_path)
    print("  inserted {} example sentences".format(ex_n))

    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("word_total", str(total_words)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("zh_total", str(zh_n)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("cefr_tagged", str(cefr_updated)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("example_total", str(ex_n)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("schema_version", "4"))
    conn.commit()
    conn.close()

    size_mb = os.path.getsize(args.out) / (1024 * 1024)
    print("\nWrote {} ({:.1f} MB), {} words total.".format(args.out, size_mb, total_words))


if __name__ == "__main__":
    main()
