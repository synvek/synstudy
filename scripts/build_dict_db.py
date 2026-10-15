#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Build the read-only SQLite dictionary database for synstudy.

Inputs (place under data/):
  data/ecdict.csv            ECDICT master dictionary (UTF-8). Source:
                             https://github.com/skywind3000/ECDICT  (MIT)
  data/dicts/*.json          Optional syllabus word lists (xiaoxue.json /
                             tem4.json / tem8.json) for 小学/专四/专八.
                             Bring your own official-syllabus lists; the
                             categories are skipped when the files are absent.
                             (GPL/unlicensed third-party lists were removed.)
  data/tatoeba/en_sentences.tsv.bz2   Tatoeba English sentences (CC-BY 2.0 FR;
                                        fetched from the per_language/eng export —
                                        Tatoeba renamed "en" to ISO639-3 "eng")
  data/tatoeba/cmn_sentences.tsv.bz2  Tatoeba Mandarin sentences (optional)
  data/tatoeba/links.tsv.bz2          Tatoeba cross-language links (optional)
  data/opencc/TSCharacters.txt        OpenCC Traditional->Simplified char map
  data/opencc/TSPhrases.txt           OpenCC Traditional->Simplified phrase map
  data/wordnet/wordnet31/data.*       Princeton WordNet 3.1 dict files (synonyms
                             / antonyms supplement; via scripts/fetch_data.sh)

Output:
  assets/synstudy.db         Bundled with the app, opened read-only at runtime.

Usage:
  python3 scripts/build_dict_db.py            # full build
  python3 scripts/build_dict_db.py --limit-words 50000   # smaller test build

If data/ is empty, run `python3 scripts/gen_sample_data.py` first to generate a
small sample dataset for development.
"""
import argparse
import bz2
import collections
import csv
import glob
import os
import random
import re
import sqlite3
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Category definition:
#   key     stable identifier used by the app
#   name    display name (Chinese)
#   source  "tag"  -> selected from ECDICT `tag` column (ordered A-Z)
#           "json" -> selected from a local JSON word list (original order);
#                     the category is skipped when no list file is present
#   tag     ECDICT tag token (source == "tag")
#   globs   candidate JSON filename patterns (source == "json"), tried in order
CATEGORIES = [
    {"key": "xiaoxue",  "name": "小学",         "source": "json",
     "globs": ["xiaoxue.json"]},
    {"key": "chuzhong", "name": "初中",         "source": "tag", "tag": "zk"},
    {"key": "gaozhong", "name": "高中",         "source": "tag", "tag": "gk"},
    {"key": "cet4",     "name": "大学英语四级", "source": "tag", "tag": "cet4"},
    {"key": "cet6",     "name": "大学英语六级", "source": "tag", "tag": "cet6"},
    {"key": "kaoyan",   "name": "考研英语",     "source": "tag", "tag": "ky"},
    {"key": "tem4",     "name": "英语专业四级", "source": "json",
     "globs": ["tem4.json"]},
    {"key": "tem8",     "name": "英语专业八级", "source": "json",
     "globs": ["tem8.json"]},
    {"key": "toefl",    "name": "托福",         "source": "tag", "tag": "toefl"},
    {"key": "ielts",    "name": "雅思",         "source": "tag", "tag": "ielts"},
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

-- Synonyms / antonyms extracted from Wiktionary (kaikki), one row per
-- (word, kind, target). Kept to the graded category headwords like `examples`.
-- `target` is normalized lowercase and always exists in `words` (validated at
-- extraction time), so the runtime can resolve it to a clickable entry.
CREATE TABLE word_relations (
    word_id     INTEGER NOT NULL,
    rel_kind    TEXT NOT NULL,       -- 'syn' synonyms | 'ant' antonyms
    target      TEXT NOT NULL,
    source      TEXT NOT NULL DEFAULT 'Wiktionary',
    order_index INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (word_id, rel_kind, target)
);
CREATE INDEX idx_wordrels_kind ON word_relations(word_id, rel_kind, order_index);

CREATE TABLE word_roots (
    root_key TEXT PRIMARY KEY,
    class    TEXT DEFAULT '',
    meaning  TEXT DEFAULT '',
    origin   TEXT DEFAULT '',
    synonyms TEXT DEFAULT '',
    antonyms TEXT DEFAULT '',
    note     TEXT DEFAULT '',
    examples TEXT DEFAULT ''
);

CREATE TABLE root_words (
    root_key    TEXT NOT NULL,
    word_id     INTEGER NOT NULL,
    order_index INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (root_key, word_id)
);
CREATE INDEX idx_root_words_word ON root_words(word_id);

-- Graded sentence bank built from open corpora (Tatoeba CC-BY 2.0 FR,
-- Wiktionary CC BY-SA 4.0). Each row is tagged with the exam levels whose
-- syllabus vocabulary it contains, so the runtime can filter by 四级/六级/
-- 专四/专八/托福/GRE. `sentence_words` links back to `words` for cloze and
-- translation-choice quiz generation.
CREATE TABLE sentence_bank (
    id           INTEGER PRIMARY KEY,
    text         TEXT NOT NULL,
    translation  TEXT DEFAULT '',
    source       TEXT NOT NULL DEFAULT 'Tatoeba',
    source_id    TEXT DEFAULT '',
    cefr_min     TEXT DEFAULT '',
    exam_levels  TEXT DEFAULT '',
    difficulty   INTEGER DEFAULT 0,
    word_count   INTEGER DEFAULT 0
);
CREATE INDEX idx_sentence_exam ON sentence_bank(exam_levels);
CREATE INDEX idx_sentence_diff ON sentence_bank(difficulty);
CREATE INDEX idx_sentence_cefr ON sentence_bank(cefr_min);

CREATE TABLE sentence_words (
    sentence_id  INTEGER NOT NULL,
    word_id      INTEGER NOT NULL,
    token_index  INTEGER NOT NULL,
    token_text   TEXT NOT NULL DEFAULT '',
    is_target    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (sentence_id, token_index)
);
CREATE INDEX idx_sw_word ON sentence_words(word_id);
CREATE INDEX idx_sw_target ON sentence_words(sentence_id, is_target);
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
    """Insert category_words for a tag-sourced category, ordered A-Z.

    ECDICT's `tag` lists carry no canonical textbook order, so the words are
    sorted alphabetically (case-insensitive). That makes the app's 正序/倒序
    modes behave as learners expect (A→Z / Z→A); a raw frequency ranking reads
    as scrambled and is intentionally not used here.
    """
    tag = cat["tag"]
    cur = conn.cursor()
    # match the tag as a whole token within the space-separated tag column
    cur.execute(
        "SELECT id FROM words "
        "WHERE (' ' || tag || ' ') LIKE ? "
        "ORDER BY word COLLATE NOCASE, id",
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


def build_fts(conn):
    """Create an FTS5 full-text index over the words table.

    Indexes the headword and its Chinese/English translation so the runtime can
    do prefix + full-text search (and fall back to it for typo tolerance). The
    index uses external content (`content='words'`) to avoid duplicating the
    payload. Returns True on success, False when FTS5 is unavailable, in which
    case the app degrades to plain LIKE search.
    """
    cur = conn.cursor()
    try:
        cur.execute("DROP TABLE IF EXISTS words_fts")
        cur.execute(
            "CREATE VIRTUAL TABLE words_fts USING fts5("
            "word, translation, content='words', content_rowid='id')")
        cur.execute(
            "INSERT INTO words_fts(rowid, word, translation) "
            "SELECT id, word, translation FROM words")
        conn.commit()
        return True
    except sqlite3.OperationalError as e:
        print("  [warn] FTS5 unavailable, skipping full-text index: {}".format(e))
        try:
            cur.execute("DROP TABLE IF EXISTS words_fts")
            conn.commit()
        except sqlite3.Error:
            pass
        return False


def _as_text(v):
    """Normalise a wordroot JSON field to a string (lists become comma-joined)."""
    if isinstance(v, list):
        return ", ".join(str(x).strip() for x in v if str(x).strip())
    return str(v).strip() if v is not None else ""


def _fix_affix_spacing(note, key):
    """Re-insert spaces lost by the source's HTML stripping.

    In wordroot.txt the `function` text glues an affix to the following
    lowercase word (e.g. "-lessattaches", "syl-occurs"). Only fix occurrences
    that start at a boundary, so inner-word matches are left alone.
    """
    if not note:
        return note
    for variant in (v.strip() for v in key.split(",")):
        if variant:
            note = re.sub(r"(^|\s)(" + re.escape(variant) + r")(?=[a-z])",
                          r"\1\2 ", note)
    return note


def build_roots(conn, wordroot_path):
    """Parse ECDICT `wordroot.txt` (JSON keyed by root/affix) into `word_roots`
    and `root_words`.

    Each key becomes a root entry (meaning/class/origin/synonyms/antonyms). Its
    `example` list is linked to existing `words` rows only; markers that are not
    real headwords (e.g. "soundless1", "Homo sapiens") are skipped. The raw
    example list is also kept verbatim in `examples` (fallback display), and the
    source's `function` usage commentary is stored in `note`. Returns
    (root_count, link_count).
    """
    if not os.path.exists(wordroot_path):
        print("  [warn] {} not found; 词根数据将为空".format(wordroot_path))
        return 0, 0
    import json
    with open(wordroot_path, "r", encoding="utf-8") as f:
        data = json.load(f)
    cur = conn.cursor()
    ids = word_id_map(conn)
    lower_ids = {}
    for w, i in ids.items():
        lower_ids.setdefault(w.strip().lower(), i)
    root_count = 0
    link_count = 0
    for key, info in data.items():
        if not isinstance(info, dict):
            continue
        cur.execute(
            "INSERT OR REPLACE INTO word_roots "
            "(root_key, class, meaning, origin, synonyms, antonyms, note, examples) "
            "VALUES (?,?,?,?,?,?,?,?)",
            (key,
             _as_text(info.get("class")),
             _as_text(info.get("meaning")),
             _as_text(info.get("origin")),
             _as_text(info.get("synonyms")),
             _as_text(info.get("antonyms")),
             _fix_affix_spacing(_as_text(info.get("function")).replace("\r\n", "\n"), key),
             _as_text(info.get("example"))))
        root_count += 1
        order = 0
        for ex in (info.get("example") or []):
            name = str(ex or "").strip()
            if not name:
                continue
            wid = ids.get(name)
            if wid is None:
                wid = lower_ids.get(name.lower())
            if wid is None:
                continue
            cur.execute(
                "INSERT OR IGNORE INTO root_words "
                "(root_key, word_id, order_index) VALUES (?,?,?)",
                (key, wid, order))
            order += 1
            link_count += 1
    conn.commit()
    return root_count, link_count


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
# Max synonyms / antonyms kept per headword and direction. Wiktionary lists
# sense-ordered candidates, so the first few are the most common ones.
MAX_REL_PER_KIND = 5

# kaikki relation entries carrying these tags are too specialized for learners.
_REL_SKIP_TAGS = {"obsolete", "archaic"}
# A kept relation is a short lowercase English word or <=3-word phrase.
_REL_WORD_RE = re.compile(r"^[a-z][a-z' -]*[a-z']?$")


def _rel_candidates(v):
    """Flatten one kaikki `synonyms`/`antonyms` field into candidate words.

    The field can be a plain string (`"lexicon, vocabulary"`), a list of
    strings, or (most often) a list of objects like `{"word": ..., "tags":
    [...]}`. Tagged entries (obsolete/archaic) are dropped here; everything is
    lowercased and comma-joined strings are split into parts.
    """
    out = []
    items = v if isinstance(v, list) else [v]
    for it in items:
        if isinstance(it, dict):
            if set(it.get("tags") or []) & _REL_SKIP_TAGS:
                continue
            w = it.get("word") or ""
        else:
            w = it or ""
        for part in str(w).split(","):
            t = part.strip().lower()
            if t:
                out.append(t)
    return out


def _clean_relation(t, head, known_words):
    """Accept a candidate only if it is a plausible short English word/phrase
    present in our dictionary and different from the headword itself."""
    if (t == head or len(t) > 30 or len(t.split()) > 3
            or "{" in t or not _REL_WORD_RE.match(t)):
        return False
    return t in known_words


def build_examples(conn, kaikki_path, known_words, max_per_word=MAX_EX_PER_WORD):
    """Extract Wiktionary (kaikki.org) example sentences *and* synonym/antonym
    relations for words that appear in the graded category word banks.

    Streams the ~3GB `kaikki.org-dictionary-English.jsonl` once and, for each
    English headword that belongs to a study bank, keeps up to `max_per_word`
    clean natural sentences plus up to `MAX_REL_PER_KIND` synonyms and
    antonyms per word. Restricting to the graded banks (instead of all 770k
    dictionary words) keeps the `examples`/`word_relations` tables small while
    covering exactly the vocabulary learners study. `known_words` is the set of
    lowercased headwords in `words`, used to drop junk/non-word relation
    candidates. Returns (example_rows, relation_rows) inserted.
    """
    if not os.path.exists(kaikki_path):
        print("  [warn] {} not found; \u4f8b\u53e5\u5c06\u4e3a\u7a7a\uff08\u8fd0\u884c scripts/fetch_data.sh \u4e0b\u8f7d kaikki\uff09".format(kaikki_path))
        return 0, 0
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
        return 0, 0
    kept = {}  # word_id -> count kept so far
    rel_kept = {}  # (word_id, kind) -> count kept so far
    n = 0
    rel_n = 0
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
            # --- synonyms / antonyms (same streamed record, no extra pass) ---
            for field, kind in (("synonyms", "syn"), ("antonyms", "ant")):
                if rel_kept.get((wid, kind), 0) >= MAX_REL_PER_KIND:
                    continue
                cands = []
                for sense in o.get("senses", []):
                    cands.extend(_rel_candidates(sense.get(field) or []))
                # Word-level entries come after the (more precise) sense ones.
                cands.extend(_rel_candidates(o.get(field) or []))
                for t in cands:
                    if not _clean_relation(t, w, known_words):
                        continue
                    idx = rel_kept.get((wid, kind), 0)
                    cur.execute(
                        "INSERT OR IGNORE INTO word_relations "
                        "(word_id, rel_kind, target, source, order_index) "
                        "VALUES (?,?,?,?,?)",
                        (wid, kind, t, "Wiktionary", idx))
                    if cur.rowcount > 0:
                        rel_n += 1
                        rel_kept[(wid, kind)] = idx + 1
                    if rel_kept.get((wid, kind), 0) >= MAX_REL_PER_KIND:
                        break
    conn.commit()
    return n, rel_n


# WordNet 3.1 dict files (Princeton licence, BSD-style). The NLTK wordnet31
# package ships the same data.* files; adverbs use ss_type 'r'.
WN_POS_FILES = ("data.noun", "data.verb", "data.adj", "data.adv")
WN_POS = {"n", "v", "a", "s", "r"}
_WN_OFFSET_RE = re.compile(r"^\d{7,9}$")
_WN_WORD_RE = re.compile(r"^[A-Za-z][A-Za-z0-9' -]*$")


def _wn_parse(line):
    """Parse one WordNet 3.1 `data.*` synset line.

    Returns `(offset, pos, words, antonym_targets)` where each antonym target
    is `(pos, offset)` — synset offsets are only unique *per pos file*, so
    cross-file lookups must be keyed by both — or None. Handles the 3.1
    quirks: w_cnt and source/target fields written in hex when >= 10 (e.g.
    `0b` = 11 words), 4-token pointers (`sym offset pos srctgt`) and the
    optional `(A` / `(P` / `(S` adjective-position marker before a pointer
    target. Only adjectives carry `!` (antonym) pointers.
    """
    t = line.split()
    if len(t) < 6 or not _WN_OFFSET_RE.match(t[0]) or t[2] not in WN_POS:
        return None
    try:
        w_cnt = int(t[3], 16)
    except ValueError:
        return None
    words = []
    i = 4
    for _ in range(w_cnt):
        if i + 1 >= len(t):
            return None
        w = t[i].replace("_", " ")
        if _WN_WORD_RE.match(w):
            words.append(w.lower())
        i += 2
    antonyms = []
    while i < len(t) and t[i] != "|":
        if t[i] == "!":
            j = i + 1
            if j < len(t) and t[j].startswith("("):
                j += 1  # `(A` / `(P` / `(S` position marker -> skip
            if (j + 1 < len(t) and _WN_OFFSET_RE.match(t[j])
                    and t[j + 1] in WN_POS):
                antonyms.append((t[j + 1], t[j]))
        i += 1
    return t[0], t[2], words, antonyms


def build_wordnet(conn, wn_dir, known_words):
    """Top up synonym/antonym coverage with Princeton WordNet 3.1 synsets.

    Runs after build_examples(): Wiktionary has first claim on each
    (word, kind) group and WordNet only fills the remaining slots up to
    MAX_REL_PER_KIND, tagging its rows `source='WordNet'`. Synonyms are the
    other members of the headword's synsets; antonyms follow `!` pointers
    (adjectives only). Returns rows inserted, 0 when the dict files are
    absent (fetch via scripts/fetch_data.sh).
    """
    if not all(os.path.exists(os.path.join(wn_dir, f)) for f in WN_POS_FILES):
        print("  [warn] {} missing; \u8fd1\u53cd\u4e49\u4e0d\u4f1a\u8865\u5145\uff08\u8fd0\u884c scripts/fetch_data.sh \u4e0b\u8f7d WordNet\uff09".format(wn_dir))
        return 0
    cur = conn.cursor()
    cur.execute(
        "SELECT DISTINCT w.id, LOWER(w.word) FROM category_words cw "
        "JOIN words w ON w.id = cw.word_id")
    targets = {}
    for wid, lw in cur.fetchall():
        if lw:
            targets.setdefault(lw, wid)
    # Wiktionary already filled some slots; continue the per-(word,kind) count.
    rel_kept = {}
    cur.execute("SELECT word_id, rel_kind, COUNT(*) FROM word_relations "
                "GROUP BY 1, 2")
    for wid, kind, cnt in cur.fetchall():
        rel_kept[(wid, kind)] = cnt
    # Pass 1: index synsets by (pos, offset) — offsets collide across files —
    # and headwords by the synsets they appear in.
    synsets = {}
    word_synsets = collections.defaultdict(list)
    for name in WN_POS_FILES:
        with open(os.path.join(wn_dir, name), "r", encoding="utf-8",
                  errors="replace") as f:
            for line in f:
                if not line[:1].isdigit():
                    continue  # license header / verb frame continuation
                r = _wn_parse(line)
                if not r:
                    continue
                off, pos, words, ants = r
                synsets[(pos, off)] = (words, ants)
                for w in words:
                    word_synsets[w].append((pos, off))
    n = 0
    for lw, wid in targets.items():
        offs = word_synsets.get(lw)
        if not offs:
            continue
        for kind in ("syn", "ant"):
            if rel_kept.get((wid, kind), 0) >= MAX_REL_PER_KIND:
                continue
            cands = []
            for key in offs:
                words, ants = synsets[key]
                if kind == "syn":
                    cands.extend(w for w in words if w != lw)
                elif key[0] in ("a", "s"):
                    for tkey in ants:
                        if tkey in synsets:
                            cands.extend(synsets[tkey][0])
            for t in cands:
                if rel_kept.get((wid, kind), 0) >= MAX_REL_PER_KIND:
                    break
                if not _clean_relation(t, lw, known_words):
                    continue
                idx = rel_kept.get((wid, kind), 0)
                cur.execute(
                    "INSERT OR IGNORE INTO word_relations "
                    "(word_id, rel_kind, target, source, order_index) "
                    "VALUES (?,?,?,?,?)",
                    (wid, kind, t, "WordNet", idx))
                if cur.rowcount > 0:
                    rel_kept[(wid, kind)] = idx + 1
                    n += 1
    conn.commit()
    return n


# Exam tags we care about for sentence grading. Order defines the difficulty
# tier used when tagging a sentence with an overall score (higher == harder).
# `zk`/`gk` are deliberately excluded: 中考/高考 vocab sits below CET-4 and
# would drown out more advanced sentences.
SENT_EXAM_TAGS = ["cet4", "cet6", "ky", "tem4", "tem8", "toefl", "ielts", "gre"]
SENT_EXAM_TIER = {"cet4": 1, "cet6": 2, "ky": 3, "tem4": 3, "tem8": 4, "toefl": 4, "ielts": 4, "gre": 5}

# Sentence-level filters. Sentences outside this window are dropped: too short
# to be interesting, too long to display cleanly on a card.
SENT_MIN_TOKENS = 5
SENT_MAX_TOKENS = 22
# Cap total sentences kept so the DB stays < ~50 MB extra. Selection prefers
# sentences with translations and higher-tier vocab.
SENT_TOTAL_CAP = 120000

# Regex for whitespace tokenization that keeps punctuation attached to the
# word (matches what the runtime does when reconstructing a cloze blank).
TOKEN_RE = re.compile(r"\S+")
ALPHA_RE = re.compile(r"[A-Za-z]{2,}")


def _open_maybe_bz2(path):
    """Open a plain or bz2-compressed text file transparently."""
    if path.endswith(".bz2"):
        return bz2.open(path, "rt", encoding="utf-8", errors="replace")
    return open(path, "r", encoding="utf-8", errors="replace")


def _load_exam_vocab(conn):
    """Return {lowercase_word -> (word_id, set_of_exam_tags, cefr, translation)}.

    Only words with at least one SENT_EXAM_TAGS tag are included; they are the
    pool used for tagging sentences and generating cloze distractors.
    """
    cur = conn.cursor()
    cur.execute("SELECT id, word, tag, cefr, translation FROM words")
    out = {}
    for wid, w, tag, cefr, trans in cur.fetchall():
        if not tag:
            continue
        toks = tag.split()
        exams = {t for t in toks if t in SENT_EXAM_TAGS}
        if not exams:
            continue
        out[w.strip().lower()] = (wid, exams, cefr or "", trans or "")
    return out


def _load_cfr_map(conn):
    """Return {lowercase_word -> cefr} for CEFR-tagged words (any word, not
    just exam vocab). Used to compute a sentence's minimum CEFR level."""
    cur = conn.cursor()
    cur.execute("SELECT word, cefr FROM words WHERE cefr <> ''")
    return {w.strip().lower(): c for w, c in cur.fetchall() if c}


def _load_word_ids(conn):
    """Return {lowercase_word -> word_id} for every entry in `words`."""
    cur = conn.cursor()
    cur.execute("SELECT id, word FROM words")
    return {w.strip().lower(): i for i, w in cur.fetchall()}


def _tokenize(text):
    """Split on whitespace, keeping punctuation attached. Returns list of
    (index, raw_token, alpha_part_lowercased_or_empty)."""
    out = []
    for i, tok in enumerate(TOKEN_RE.findall(text)):
        m = ALPHA_RE.search(tok)
        alpha = m.group(0).lower() if m else ""
        out.append((i, tok, alpha))
    return out


def _score_sentence(tokens, exam_vocab, cefr_map, all_word_ids):
    """Analyse a tokenized sentence. Returns None if the sentence should be
    dropped, otherwise (exam_levels_str, cefr_min, difficulty, word_id_map)
    where word_id_map is {token_index -> (word_id, is_target)}."""
    if not (SENT_MIN_TOKENS <= len(tokens) <= SENT_MAX_TOKENS):
        return None
    alpha_tokens = [t for t in tokens if t[2]]
    if len(alpha_tokens) < SENT_MIN_TOKENS:
        return None
    # Reject sentences that are mostly proper nouns / non-English: require the
    # dictionary to cover at least 40% of alphabetic tokens (or at least 3,
    # whichever is bigger). With the full 770k-word ECDICT this is very loose;
    # it exists mainly to keep tiny sample builds from rejecting everything.
    known = sum(1 for _i, _raw, a in alpha_tokens if a in all_word_ids)
    if known < max(3, int(len(alpha_tokens) * 0.4)):
        return None
    exams = set()
    tier = 0
    cefr_ranks = []
    word_id_map = {}
    for idx, raw, a in tokens:
        if not a:
            continue
        wid = all_word_ids.get(a)
        if wid is None:
            continue
        entry = exam_vocab.get(a)
        is_target = 0
        if entry is not None:
            exams |= entry[1]
            tier = max(tier, max(SENT_EXAM_TIER[t] for t in entry[1]))
            is_target = 1
        word_id_map[idx] = (wid, raw, is_target)
        c = cefr_map.get(a) or (entry[2] if entry else "")
        if c in CEFR_RANK:
            cefr_ranks.append(CEFR_RANK[c])
    if not exams:
        # No syllabus word -> not useful for exam-graded practice.
        return None
    inv = {r: l for l, r in CEFR_RANK.items()}
    cefr_min = inv[min(cefr_ranks)] if cefr_ranks else ""
    return (",".join(sorted(exams)), cefr_min, tier, word_id_map)


def _load_tatoeba_links(path):
    """Return {en_sentence_id -> [cmn_sentence_id, ...]} from a Tatoeba links
    TSV (columns: sentence_id, translation_id, user_id, date_added, ...).

    Both directions are indexed so the caller can look up either side.
    """
    if not path or not os.path.exists(path):
        return {}
    links = {}
    with _open_maybe_bz2(path) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 2:
                continue
            a, b = parts[0], parts[1]
            links.setdefault(a, []).append(b)
            links.setdefault(b, []).append(a)
    return links


def _load_tatoeba_cmn(path):
    """Return {sentence_id -> text} for the Mandarin Tatoeba export."""
    if not path or not os.path.exists(path):
        return {}
    out = {}
    with _open_maybe_bz2(path) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 3:
                continue
            out[parts[0]] = parts[2]
    return out


def _iter_tatoeba_en(path):
    """Yield (id, text) for each English Tatoeba sentence."""
    with _open_maybe_bz2(path) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 3:
                continue
            # Some rows have a leading '# ' marker in older dumps; skip those.
            if parts[0].startswith("#"):
                continue
            yield parts[0], parts[2]


def _load_t2s_maps(opencc_dir):
    """Load OpenCC Traditional->Simplified maps (TSCharacters / TSPhrases).

    Returns (char_map, phrase_map, max_phrase_len). Maps are empty when the
    files are unavailable, in which case translations keep their original
    glyph forms (Tatoeba/cmn translations are predominantly Traditional).
    """
    def _load(name):
        path = os.path.join(opencc_dir, name) if opencc_dir else None
        m = {}
        if not path or not os.path.exists(path):
            return m
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) != 2:
                    continue
                # Several targets are context-dependent in OpenCC; the first
                # is its default pick and suffices for plain sentences.
                m[parts[0]] = parts[1].split(" ")[0]
        return m
    cmap = _load("TSCharacters.txt")
    pmap = _load("TSPhrases.txt")
    mlen = max((len(k) for k in pmap), default=1)
    return cmap, pmap, mlen


def _t2s(text, cmap, pmap, mlen):
    """Convert Traditional Chinese text to Simplified (greedy longest phrase)."""
    if not cmap:
        return text
    out = []
    i, n = 0, len(text)
    while i < n:
        for L in range(min(mlen, n - i), 1, -1):
            w = text[i:i + L]
            if w in pmap:
                out.append(pmap[w])
                i += L
                break
        else:
            ch = text[i]
            out.append(cmap.get(ch, ch))
            i += 1
    return "".join(out)


def build_sentence_bank(conn, en_path, cmn_path=None, links_path=None,
                        total_cap=SENT_TOTAL_CAP, seed=20260101, t2s=None):
    """Build `sentence_bank` + `sentence_words` from the Tatoeba English dump.

    Each sentence is tokenized, scored against the exam-tagged vocabulary
    (see `_score_sentence`) and only kept if it contains at least one 考纲词.
    When Chinese (`cmn`) + `links` files are provided, an available Chinese
    translation is attached for the sentence-translation quiz mode. Sentences
    with a translation are preferred when the cap kicks in. If `t2s` (the
    tuple returned by `_load_t2s_maps`) is given, translations are converted
    from Traditional to Simplified Chinese before insertion.

    Returns (kept_count, with_translation_count).
    """
    if not en_path or not os.path.exists(en_path):
        print("  [warn] {} not found; 句库将为空".format(en_path))
        return 0, 0
    exam_vocab = _load_exam_vocab(conn)
    cefr_map = _load_cfr_map(conn)
    all_ids = _load_word_ids(conn)
    if not exam_vocab:
        print("  [warn] no exam-tagged vocabulary; skipping sentence_bank")
        return 0, 0
    links = _load_tatoeba_links(links_path)
    cmn = _load_tatoeba_cmn(cmn_path)

    # Pass 1: collect candidate sentences (id, text, score_tuple, has_zh).
    # We keep candidates in memory because the cap step needs to sort them.
    candidates = []
    for sid, text in _iter_tatoeba_en(en_path):
        text = text.strip()
        if not text or len(text) > 300:
            continue
        # Skip Tatoeba rows with markup or non-sentence fragments.
        if any(c in text for c in ("{", "}", "|", "\u2028")):
            continue
        tokens = _tokenize(text)
        scored = _score_sentence(tokens, exam_vocab, cefr_map, all_ids)
        if scored is None:
            continue
        zh_text = ""
        if links and cmn:
            for tgt in links.get(sid, ()):
                t = cmn.get(tgt)
                if t and 2 <= len(t) <= 120:
                    zh_text = t
                    break
        candidates.append((sid, text, scored, zh_text, tokens))

    if not candidates:
        return 0, 0

    # Prefer sentences with a Chinese translation, then higher difficulty tier,
    # then shorter (easier to read on a card). Deterministic shuffle first so
    # ties break randomly rather than by Tatoeba upload order.
    rng = random.Random(seed)
    rng.shuffle(candidates)
    candidates.sort(key=lambda c: (0 if c[3] else 1, -c[2][2], len(c[4])))
    if len(candidates) > total_cap:
        candidates = candidates[:total_cap]

    cur = conn.cursor()
    kept = 0
    with_zh = 0
    for sid, text, (exams, cefr_min, tier, wid_map), zh_text, tokens in candidates:
        if zh_text and t2s:
            zh_text = _t2s(zh_text, *t2s)
        cur.execute(
            "INSERT INTO sentence_bank "
            "(text, translation, source, source_id, cefr_min, exam_levels, difficulty, word_count) "
            "VALUES (?,?,?,?,?,?,?,?)",
            (text, zh_text, "Tatoeba", sid, cefr_min, exams, tier, len(tokens)))
        sent_id = cur.lastrowid
        for idx, (wid, raw, is_target) in wid_map.items():
            cur.execute(
                "INSERT OR IGNORE INTO sentence_words "
                "(sentence_id, word_id, token_index, token_text, is_target) "
                "VALUES (?,?,?,?,?)",
                (sent_id, wid, idx, raw, is_target))
        kept += 1
        if zh_text:
            with_zh += 1
    conn.commit()
    return kept, with_zh


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
        if count == 0:
            # No word list supplied (json categories expect the user's own
            # official-syllabus file): drop the empty category so the app
            # does not render a useless tile.
            cur.execute("DELETE FROM categories WHERE id = ?", (cat["rowid"],))
            conn.commit()
            print("  {:<10} {:<8}   (skipped: no word list)".format(cat["key"], cat["name"]))
            continue
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
    print("Building examples + relations (Wiktionary) from {} ...".format(kaikki_path))
    known_words = set(w.lower() for w in ids)
    ex_n, rel_n = build_examples(conn, kaikki_path, known_words)
    print("  inserted {} example sentences, {} word relations".format(ex_n, rel_n))

    wn_dir = os.path.join(args.data_dir, "wordnet", "wordnet31")
    print("Topping up relations (WordNet synsets) from {} ...".format(wn_dir))
    wn_n = build_wordnet(conn, wn_dir, known_words)
    print("  inserted {} wordnet relations".format(wn_n))

    print("Building FTS5 index (words_fts) ...")
    fts_ok = build_fts(conn)
    print("  FTS5 index: {}".format(
        "ok" if fts_ok else "unavailable (runtime falls back to LIKE)"))

    wordroot_path = os.path.join(args.data_dir, "wordroot.txt")
    print("Building word roots from {} ...".format(wordroot_path))
    root_total, root_links = build_roots(conn, wordroot_path)
    print("  {} roots, {} word links".format(root_total, root_links))

    tatoeba_dir = os.path.join(args.data_dir, "tatoeba")
    en_path = os.path.join(tatoeba_dir, "en_sentences.tsv.bz2")
    if not os.path.exists(en_path):
        en_path = os.path.join(tatoeba_dir, "en_sentences.tsv")
    cmn_path = os.path.join(tatoeba_dir, "cmn_sentences.tsv.bz2")
    if not os.path.exists(cmn_path):
        cmn_path = os.path.join(tatoeba_dir, "cmn_sentences.tsv")
    links_path = os.path.join(tatoeba_dir, "links.tsv.bz2")
    if not os.path.exists(links_path):
        links_path = os.path.join(tatoeba_dir, "links.tsv")
    print("Building sentence bank (Tatoeba) from {} ...".format(en_path))
    t2s = _load_t2s_maps(os.path.join(args.data_dir, "opencc"))
    if not t2s[0]:
        print("  [warn] data/opencc/TSCharacters.txt not found; "
              "中文译文将保留原始字形（多为繁体）")
    else:
        print("  t2s maps: {} chars / {} phrases (OpenCC, Apache-2.0)".format(
            len(t2s[0]), len(t2s[1])))
    sent_total, sent_zh = build_sentence_bank(
        conn, en_path,
        cmn_path if os.path.exists(cmn_path) else None,
        links_path if os.path.exists(links_path) else None,
        t2s=t2s)
    print("  inserted {} graded sentences ({} with Chinese translation)".format(
        sent_total, sent_zh))

    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("word_total", str(total_words)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("zh_total", str(zh_n)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("cefr_tagged", str(cefr_updated)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("example_total", str(ex_n)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("relation_total", str(rel_n)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("wordnet_total", str(wn_n)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("fts", "1" if fts_ok else "0"))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("root_total", str(root_total)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("sentence_total", str(sent_total)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("sentence_trans_total", str(sent_zh)))
    cur.execute("INSERT INTO meta (key, value) VALUES (?,?)",
                ("schema_version", "9"))
    conn.commit()

    # Render the release-bundle licence declaration from license/README.md,
    # limited to the data sources that actually landed in this build, and stamp
    # its content hash into meta. Adding a new source without declaring it in
    # license/README.md (or vice versa) therefore shows up as a notices diff.
    try:
        import gen_notices
        notices_ver = gen_notices.generate_from_conn(
            conn,
            os.path.join(ROOT, "license", "README.md"),
            os.path.join(os.path.dirname(args.out), "THIRD-PARTY-NOTICES.txt"))
        cur.execute("INSERT OR REPLACE INTO meta (key, value) VALUES (?,?)",
                    ("notices_version", notices_ver))
        conn.commit()
        print("  THIRD-PARTY-NOTICES.txt generated (version {})".format(notices_ver))
    except Exception as e:
        print("  [warn] THIRD-PARTY-NOTICES 生成失败: {}".format(e))
    conn.close()

    size_mb = os.path.getsize(args.out) / (1024 * 1024)
    print("\nWrote {} ({:.1f} MB), {} words total.".format(args.out, size_mb, total_words))


if __name__ == "__main__":
    main()
