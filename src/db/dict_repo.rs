use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags, Row};

use crate::models::{Category, Example, Sentence, SentenceWord, Word, WordRoot, ZhEntry};

const WORD_COLS: &str = "w.id, w.word, w.phonetic, w.translation, w.definition, \
                         w.pos, w.tag, w.bnc, w.frq, w.exchange, w.cefr";

const ZH_COLS: &str = "z.id, z.traditional, z.simplified, z.pinyin, z.glosses";

/// Read-only access to the bundled dictionary database.
///
/// A connection is opened per call so the handle stays `Send + Sync` and can be
/// shared through Dioxus context without lifetime issues. Opening SQLite is
/// cheap even for large databases.
#[derive(Clone, Debug)]
pub struct DictRepo {
    pub path: Option<PathBuf>,
}

fn row_to_word(row: &Row) -> rusqlite::Result<Word> {
    Ok(Word {
        id: row.get(0)?,
        word: row.get(1)?,
        phonetic: row.get(2)?,
        translation: row.get(3)?,
        definition: row.get(4)?,
        pos: row.get(5)?,
        tag: row.get(6)?,
        bnc: row.get(7)?,
        frq: row.get(8)?,
        exchange: row.get(9)?,
        cefr: row.get(10)?,
    })
}

fn row_to_zh_entry(row: &Row) -> rusqlite::Result<ZhEntry> {
    Ok(ZhEntry {
        id: row.get(0)?,
        traditional: row.get(1)?,
        simplified: row.get(2)?,
        pinyin: row.get(3)?,
        glosses: row.get(4)?,
    })
}

fn row_to_root(row: &Row) -> rusqlite::Result<WordRoot> {
    Ok(WordRoot {
        root_key: row.get(0)?,
        class: row.get(1)?,
        meaning: row.get(2)?,
        origin: row.get(3)?,
        synonyms: row.get(4)?,
        antonyms: row.get(5)?,
    })
}

const ROOT_COLS: &str = "r.root_key, r.class, r.meaning, r.origin, r.synonyms, r.antonyms";

const SENT_COLS: &str = "s.id, s.text, s.translation, s.source, s.source_id, \
                         s.cefr_min, s.exam_levels, s.difficulty, s.word_count";

fn row_to_sentence(row: &Row) -> rusqlite::Result<Sentence> {
    Ok(Sentence {
        id: row.get(0)?,
        text: row.get(1)?,
        translation: row.get(2)?,
        source: row.get(3)?,
        source_id: row.get(4)?,
        cefr_min: row.get(5)?,
        exam_levels: row.get(6)?,
        difficulty: row.get(7)?,
        word_count: row.get(8)?,
    })
}

fn row_to_sentence_word(row: &Row) -> rusqlite::Result<SentenceWord> {
    let flag: i64 = row.get(4)?;
    Ok(SentenceWord {
        sentence_id: row.get(0)?,
        word_id: row.get(1)?,
        token_index: row.get(2)?,
        token_text: row.get(3)?,
        is_target: flag != 0,
    })
}

/// True when `a` and `b` differ by at most one edit. Uses the optimal string
/// alignment (restricted Damerau-Levenshtein) distance so a single adjacent
/// transposition (e.g. "acheive" -> "achieve") counts as one edit. Used for
/// bounded typo tolerance in `search_fuzzy`.
fn edit_distance_le1(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n.abs_diff(m) > 1 {
        return false;
    }
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..=n {
        d[i][0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let mut val = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                val = val.min(d[i - 2][j - 2] + cost);
            }
            d[i][j] = val;
        }
    }
    d[n][m] <= 1
}

#[allow(dead_code)]
impl DictRepo {
    pub fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// Build from an optional resolved path (None when the DB is missing).
    pub fn from_resolved(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    /// Whether a dictionary database was found.
    pub fn available(&self) -> bool {
        self.path.is_some()
    }

    /// Path shown to the user in diagnostics/settings.
    pub fn display_path(&self) -> String {
        self.path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(not found)".to_string())
    }

    fn conn(&self) -> rusqlite::Result<Connection> {
        let path = self
            .path
            .as_ref()
            .ok_or(rusqlite::Error::InvalidPath(Default::default()))?;
        Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
    }

    /// Search the dictionary. Prefers the FTS5 full-text index (prefix match on
    /// the headword plus full-text match on the translation) when present;
    /// falls back to a plain `LIKE` prefix query, and finally to a bounded
    /// edit-distance (<= 1) fuzzy pass for likely typos.
    pub fn search(&self, query: &str, limit: i64) -> Vec<Word> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        if let Some(hits) = self.search_fts(q, limit) {
            if !hits.is_empty() {
                return hits;
            }
        }
        let mut hits = self.search_like(q, limit);
        if hits.is_empty() {
            hits = self.search_fuzzy(q, limit);
        }
        hits
    }

    /// Prefix search on the headword. Exact matches rank first, then by
    /// frequency (lower `frq` == more common).
    fn search_like(&self, query: &str, limit: i64) -> Vec<Word> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let lower = q.to_lowercase();
        let prefix = format!("{}%", lower);
        let sql = format!(
            "SELECT {WORD_COLS} FROM words w WHERE w.word LIKE ?1 \
             ORDER BY CASE WHEN lower(w.word) = ?2 THEN 0 \
                           WHEN lower(w.word) LIKE ?1 THEN 1 ELSE 2 END, \
                      CASE WHEN w.frq > 0 THEN w.frq ELSE 99999999 END, w.word \
             LIMIT ?3"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(rusqlite::params![prefix, lower, limit], row_to_word)
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// FTS5-backed search. Returns `None` when the `words_fts` table is absent
    /// (older databases / FTS5 unavailable at build time) or the query cannot be
    /// prepared, so the caller can degrade to `search_like`. The term is quoted
    /// (with an inner-quote escape) and given a `*` suffix for prefix matching.
    fn search_fts(&self, query: &str, limit: i64) -> Option<Vec<Word>> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        let conn = self.conn().ok()?;
        let lower = q.to_lowercase();
        let escaped = lower.replace('"', "\"\"");
        let match_expr = format!("\"{escaped}\"*");
        let prefix = format!("{}%", lower);
        let sql = format!(
            "SELECT {WORD_COLS} FROM words_fts JOIN words w ON w.id = words_fts.rowid \
             WHERE words_fts MATCH ?1 \
             ORDER BY CASE WHEN lower(w.word) = ?2 THEN 0 \
                           WHEN lower(w.word) LIKE ?3 THEN 1 ELSE 2 END, \
                      bm25(words_fts), \
                      CASE WHEN w.frq > 0 THEN w.frq ELSE 99999999 END \
             LIMIT ?4"
        );
        let mut stmt = conn.prepare(&sql).ok()?;
        let rows = stmt
            .query_map(
                rusqlite::params![match_expr, lower, prefix, limit],
                row_to_word,
            )
            .ok()?
            .collect::<rusqlite::Result<Vec<_>>>()
            .ok()?;
        Some(rows)
    }

    /// Bounded typo tolerance: only reached when prefix/full-text search found
    /// nothing. Pulls a frequency-ordered candidate pool sharing the query's
    /// first two letters and keeps those within edit distance 1. Typos in the
    /// first two letters are intentionally not corrected (keeps the scan small).
    fn search_fuzzy(&self, query: &str, limit: i64) -> Vec<Word> {
        let q = query.trim().to_lowercase();
        let len = q.chars().count();
        if len < 3 || len > 24 || !q.chars().all(|c| c.is_ascii_alphabetic()) {
            return Vec::new();
        }
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let prefix: String = q.chars().take(2).collect();
        let like = format!("{}%", prefix);
        let sql = format!(
            "SELECT {WORD_COLS} FROM words w WHERE w.word LIKE ?1 \
             ORDER BY CASE WHEN w.frq > 0 THEN w.frq ELSE 99999999 END, w.word LIMIT 5000"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let candidates: Vec<Word> = stmt
            .query_map(rusqlite::params![like], row_to_word)
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default();
        let mut hits: Vec<Word> = candidates
            .into_iter()
            .filter(|w| edit_distance_le1(&q, &w.word.to_lowercase()))
            .collect();
        hits.sort_by_key(|w| if w.frq > 0 { w.frq } else { 99999999 });
        if limit > 0 {
            hits.truncate(limit as usize);
        }
        hits
    }

    /// Chinese->English (汉英) lookup against the CC-CEDICT `zh_entries`
    /// table, which is keyed by the Chinese headword. Matching runs over the
    /// simplified form, the traditional form, and (for romaji input) the
    /// toneless pinyin. Results are ranked exact-headword first, then
    /// prefix, then substring; ties break toward shorter (more base) words.
    pub fn search_zh(&self, query: &str, limit: i64) -> Vec<ZhEntry> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let substr = format!("%{}%", q);
        let prefix = format!("{}%", q);
        // Toneless romaji form for pinyin matching (Chinese input yields "").
        let py: String = q
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect();
        let py_prefix = format!("{}%", py);
        // Only match pinyin when the query is romaji; an empty `py` would make
        // `pinyin_plain LIKE '%'` match every row and pollute Chinese searches.
        let romaji = !py.is_empty() && q.chars().all(|c| c.is_ascii());
        let where_clause = if romaji {
            "z.simplified LIKE ?1 OR z.traditional LIKE ?1 OR z.pinyin_plain LIKE ?2"
        } else {
            "z.simplified LIKE ?1 OR z.traditional LIKE ?1"
        };
        let sql = format!(
            "SELECT {ZH_COLS} FROM zh_entries z \
             WHERE {where_clause} \
             ORDER BY CASE WHEN z.simplified = ?3 OR z.traditional = ?3 THEN 0 \
                           WHEN z.simplified LIKE ?4 OR z.traditional LIKE ?4 THEN 1 \
                           ELSE 2 END, \
                      length(z.simplified) ASC, z.simplified ASC \
             LIMIT ?5"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(
                rusqlite::params![substr, py_prefix, q, prefix, limit],
                row_to_zh_entry,
            )
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// Total number of Chinese->English (汉英) entries.
    pub fn zh_total(&self) -> i64 {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM zh_entries", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Natural example sentences for a word (Wiktionary/kaikki). Returns empty
    /// when the `examples` table is absent (older databases) or has no rows.
    pub fn get_examples(&self, word_id: i64, limit: i64) -> Vec<Example> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let sql = "SELECT e.text, e.source FROM examples e \
                   WHERE e.word_id = ?1 ORDER BY e.order_index LIMIT ?2";
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(rusqlite::params![word_id, limit], |r| {
                Ok(Example {
                    text: r.get(0)?,
                    source: r.get(1)?,
                })
            })
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    pub fn get_by_word(&self, word: &str) -> Option<Word> {
        let conn = self.conn().ok()?;
        let sql = format!("SELECT {WORD_COLS} FROM words w WHERE w.word = ?1 COLLATE NOCASE LIMIT 1");
        conn.query_row(&sql, rusqlite::params![word.trim()], row_to_word)
            .ok()
    }

    pub fn get_by_id(&self, id: i64) -> Option<Word> {
        let conn = self.conn().ok()?;
        let sql = format!("SELECT {WORD_COLS} FROM words w WHERE w.id = ?1 LIMIT 1");
        conn.query_row(&sql, rusqlite::params![id], row_to_word).ok()
    }

    pub fn categories(&self) -> Vec<Category> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn
            .prepare("SELECT id, key, name, word_count FROM categories ORDER BY id")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], |r| {
                Ok(Category {
                    id: r.get(0)?,
                    key: r.get(1)?,
                    name: r.get(2)?,
                    word_count: r.get(3)?,
                })
            })
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// Ordered words of a category with paging. `reverse` flips the order
    /// (used for the planned reverse-order study mode).
    pub fn category_words(
        &self,
        category_id: i64,
        limit: i64,
        offset: i64,
        reverse: bool,
    ) -> Vec<Word> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let dir = if reverse { "DESC" } else { "ASC" };
        let sql = format!(
            "SELECT {WORD_COLS} FROM category_words cw \
             JOIN words w ON w.id = cw.word_id \
             WHERE cw.category_id = ?1 ORDER BY cw.order_index {dir} LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(rusqlite::params![category_id, limit, offset], row_to_word)
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// All word ids of a category in study order.
    pub fn category_word_ids(&self, category_id: i64, reverse: bool) -> Vec<i64> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let dir = if reverse { "DESC" } else { "ASC" };
        let sql = format!(
            "SELECT word_id FROM category_words WHERE category_id = ?1 ORDER BY order_index {dir}"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(rusqlite::params![category_id], |r| r.get::<_, i64>(0))
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// Fetch words for a set of ids, preserving the order of `ids`.
    pub fn words_by_ids(&self, ids: &[i64]) -> Vec<Word> {
        if ids.is_empty() {
            return Vec::new();
        }
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<Option<Word>> = (0..ids.len()).map(|_| None).collect();
        let placeholders: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT {WORD_COLS} FROM words w WHERE w.id IN ({})",
            placeholders.join(",")
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let params: Vec<&dyn rusqlite::types::ToSql> =
                ids.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
            if let Ok(rows) = stmt.query_map(params.as_slice(), row_to_word) {
                for row in rows.flatten() {
                    if let Some(pos) = ids.iter().position(|id| *id == row.id) {
                        out[pos] = Some(row);
                    }
                }
            }
        }
        out.into_iter().flatten().collect()
    }

    pub fn word_total(&self) -> i64 {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM words", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// All word roots/affixes ordered by key. Returns empty when the
    /// `word_roots` table is absent (databases built before schema v6).
    pub fn roots(&self) -> Vec<WordRoot> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let sql = format!(
            "SELECT {ROOT_COLS} FROM word_roots r ORDER BY r.root_key"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], row_to_root)
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// Words that use a given root, in stored order.
    pub fn root_words(&self, root_key: &str) -> Vec<Word> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let sql = format!(
            "SELECT {WORD_COLS} FROM root_words rw \
             JOIN words w ON w.id = rw.word_id \
             WHERE rw.root_key = ?1 ORDER BY rw.order_index, w.word"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(rusqlite::params![root_key], row_to_word)
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// Roots/affixes that a given word is built from.
    pub fn roots_for_word(&self, word_id: i64) -> Vec<WordRoot> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let sql = format!(
            "SELECT {ROOT_COLS} FROM root_words rw \
             JOIN word_roots r ON r.root_key = rw.root_key \
             WHERE rw.word_id = ?1 ORDER BY r.root_key"
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(rusqlite::params![word_id], row_to_root)
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// Number of root/affix entries (0 when the table is absent).
    pub fn root_total(&self) -> i64 {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM word_roots", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Total rows in the graded sentence bank. Returns 0 when the table is
    /// absent (databases built before schema v7 or without Tatoeba data).
    pub fn sentence_total(&self) -> i64 {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM sentence_bank", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Sentence-bank rows that contain a given word, ordered so sentences with
    /// a Chinese translation surface first (they support the widest set of
    /// quiz modes). `exam` optionally narrows the results to one exam tag.
    pub fn sentences_for_word(
        &self,
        word_id: i64,
        exam: Option<&str>,
        limit: i64,
    ) -> Vec<Sentence> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let exam_like = exam
            .filter(|e| !e.trim().is_empty())
            .map(|e| format!("%,{},%", e.trim().to_lowercase()));
        let sql = if exam_like.is_some() {
            format!(
                "SELECT {SENT_COLS} FROM sentence_bank s \
                 JOIN sentence_words sw ON sw.sentence_id = s.id \
                 WHERE sw.word_id = ?1 AND (',' || s.exam_levels || ',') LIKE ?2 \
                 ORDER BY CASE WHEN s.translation <> '' THEN 0 ELSE 1 END, s.difficulty DESC, s.id \
                 LIMIT ?3"
            )
        } else {
            format!(
                "SELECT {SENT_COLS} FROM sentence_bank s \
                 JOIN sentence_words sw ON sw.sentence_id = s.id \
                 WHERE sw.word_id = ?1 \
                 ORDER BY CASE WHEN s.translation <> '' THEN 0 ELSE 1 END, s.difficulty DESC, s.id \
                 LIMIT ?3"
            )
        };
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = if let Some(pat) = exam_like {
            stmt.query_map(rusqlite::params![word_id, pat, limit], row_to_sentence)
        } else {
            stmt.query_map(
                rusqlite::params![word_id, rusqlite::types::Null, limit],
                row_to_sentence,
            )
        };
        rows.and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
    }

    /// Random sample of sentences for quiz sessions. When `exam` is `Some`,
    /// only sentences tagged with that exam key are returned. When
    /// `require_translation` is true, only rows with a non-empty Chinese
    /// translation are eligible (used by the sentence-translation quiz).
    pub fn random_sentences(
        &self,
        exam: Option<&str>,
        require_translation: bool,
        count: i64,
    ) -> Vec<Sentence> {
        if count <= 0 {
            return Vec::new();
        }
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let exam_like = exam
            .filter(|e| !e.trim().is_empty())
            .map(|e| format!("%,{},%", e.trim().to_lowercase()));
        // Build the WHERE clause incrementally so positional params stay in
        // sync with the number of bound values.
        let mut clauses: Vec<String> = vec![
            "s.id IN (SELECT DISTINCT sentence_id FROM sentence_words WHERE is_target = 1)"
                .to_string(),
        ];
        if exam_like.is_some() {
            clauses.push("(',' || s.exam_levels || ',') LIKE ?1".to_string());
        }
        if require_translation {
            clauses.push("s.translation <> ''".to_string());
        }
        // LIMIT is always the last positional param: ?2 when exam is bound,
        // ?1 otherwise.
        let limit_param = if exam_like.is_some() { "?2" } else { "?1" };
        let sql = format!(
            "SELECT {SENT_COLS} FROM sentence_bank s WHERE {} \
             ORDER BY RANDOM() LIMIT {}",
            clauses.join(" AND "),
            limit_param
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = if let Some(pat) = exam_like {
            stmt.query_map(rusqlite::params![pat, count], row_to_sentence)
        } else {
            stmt.query_map(rusqlite::params![count], row_to_sentence)
        };
        rows.and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
    }

    /// Look up one sentence by id.
    pub fn sentence_by_id(&self, id: i64) -> Option<Sentence> {
        let conn = self.conn().ok()?;
        let sql = format!("SELECT {SENT_COLS} FROM sentence_bank s WHERE s.id = ?1 LIMIT 1");
        conn.query_row(&sql, rusqlite::params![id], row_to_sentence).ok()
    }

    /// All word occurrences inside a sentence, ordered by token index. Used by
    /// the cloze quiz to blank out a target token while preserving the rest.
    pub fn sentence_words(&self, sentence_id: i64, only_targets: bool) -> Vec<SentenceWord> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let sql = if only_targets {
            "SELECT sentence_id, word_id, token_index, token_text, is_target \
             FROM sentence_words WHERE sentence_id = ?1 AND is_target = 1 \
             ORDER BY token_index"
        } else {
            "SELECT sentence_id, word_id, token_index, token_text, is_target \
             FROM sentence_words WHERE sentence_id = ?1 ORDER BY token_index"
        };
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(rusqlite::params![sentence_id], row_to_sentence_word)
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>());
        rows.unwrap_or_default()
    }

    /// Random words drawn from a single exam tag's syllabus, used to build the
    /// distractor options for cloze / listening quizzes. `exclude` is skipped
    /// (usually the correct answer). Falls back to any exam-tagged word when
    /// `exam` is `None`.
    pub fn random_exam_words(&self, exam: Option<&str>, exclude: i64, count: i64) -> Vec<Word> {
        if count <= 0 {
            return Vec::new();
        }
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let tag = exam.unwrap_or("").trim().to_lowercase();
        // Match the exam tag as a whole token inside the space-separated `tag`
        // column. When no exam filter is set, accept any row with a non-empty
        // tag so we still pull syllabus vocabulary rather than obscure words.
        let sql = if tag.is_empty() {
            format!(
                "SELECT {WORD_COLS} FROM words w \
                 WHERE w.id != ?1 AND w.tag <> '' \
                 ORDER BY RANDOM() LIMIT ?2"
            )
        } else {
            format!(
                "SELECT {WORD_COLS} FROM words w \
                 WHERE w.id != ?1 AND (' ' || w.tag || ' ') LIKE ?3 \
                 ORDER BY RANDOM() LIMIT ?2"
            )
        };
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let like = format!("% {} %", tag);
        let rows = if tag.is_empty() {
            stmt.query_map(rusqlite::params![exclude, count], row_to_word)
        } else {
            stmt.query_map(rusqlite::params![exclude, count, like], row_to_word)
        };
        rows.and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ZhEntry;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn zh(id: i64, trad: &str, simp: &str, py: &str, glosses: &str) -> ZhEntry {
        ZhEntry {
            id,
            traditional: trad.into(),
            simplified: simp.into(),
            pinyin: py.into(),
            glosses: glosses.into(),
        }
    }

    #[test]
    fn gloss_list_splits_and_trims() {
        let e = zh(1, "蘋果", "苹果", "ping2 guo3", "apple/CL:個|个[ge4]/FIG. fruit");
        assert_eq!(e.gloss_list(), vec!["apple", "CL:個|个[ge4]", "FIG. fruit"]);
        assert!(zh(2, "x", "x", "", "").gloss_list().is_empty());
    }

    #[test]
    fn english_heads_skips_markers_and_stopwords() {
        // "to X" -> X ; trailing CL: marker contributes nothing
        let e = zh(1, "放棄", "放弃", "fang4 qi4", "to renounce/to abandon/CL:個|个[ge4]");
        assert_eq!(
            e.english_heads(),
            vec!["renounce".to_string(), "abandon".to_string()]
        );
        // parenthetical "(Tw)" marker skipped -> "calculator"
        let e2 = zh(2, "計算機", "计算机", "ji4 suan4 ji1", "computer/(Tw) calculator");
        assert_eq!(
            e2.english_heads(),
            vec!["computer".to_string(), "calculator".to_string()]
        );
    }

    /// Build a throwaway DB with a `zh_entries` table and return a repo for it.
    fn temp_zh_repo(rows: &[(&str, &str, &str, &str)]) -> (DictRepo, PathBuf) {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "synstudy_zh_test_{}_{}.db",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE zh_entries (
                id INTEGER PRIMARY KEY, traditional TEXT NOT NULL, simplified TEXT NOT NULL,
                pinyin TEXT DEFAULT '', pinyin_plain TEXT DEFAULT '', glosses TEXT DEFAULT ''
            );",
        )
        .unwrap();
        for (i, (trad, simp, py_plain, glosses)) in rows.iter().enumerate() {
            conn.execute(
                "INSERT INTO zh_entries \
                 (id, traditional, simplified, pinyin, pinyin_plain, glosses) \
                 VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![i as i64 + 1, *trad, *simp, "", *py_plain, *glosses],
            )
            .unwrap();
        }
        drop(conn);
        (DictRepo::new(path.clone()), path)
    }

    #[test]
    fn search_zh_ranks_exact_then_prefix_then_substring() {
        let (repo, path) = temp_zh_repo(&[
            ("蘋果", "苹果", "pingguo", "apple"),
            ("果", "果", "guo", "fruit/result"),
            ("果汁", "果汁", "guozhi", "juice"),
            ("水果", "水果", "shuiguo", "fruit"),
        ]);
        let hits = repo.search_zh("果", 10);
        let simp: Vec<&str> = hits.iter().map(|e| e.simplified.as_str()).collect();
        // exact headword "果" ranks first
        assert_eq!(simp.first().copied(), Some("果"));
        // prefix (果汁) and substring (水果/苹果) matches all present
        assert!(simp.contains(&"果汁"));
        assert!(simp.contains(&"水果"));
        assert!(simp.contains(&"苹果"));
        // romaji pinyin search resolves to the Chinese headword
        let py = repo.search_zh("pingguo", 10);
        assert!(py.iter().any(|e| e.simplified == "苹果"));
        // empty / whitespace queries return nothing
        assert!(repo.search_zh("   ", 10).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    /// Build a throwaway DB with `words` (incl. cefr) + `examples` tables.
    fn temp_word_repo() -> (DictRepo, PathBuf) {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "synstudy_word_test_{}_{}.db",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE words (
                id INTEGER PRIMARY KEY, word TEXT NOT NULL UNIQUE, phonetic TEXT, translation TEXT,
                definition TEXT, pos TEXT, tag TEXT, bnc INTEGER, frq INTEGER, exchange TEXT, cefr TEXT
            );
            CREATE TABLE examples (
                id INTEGER PRIMARY KEY, word_id INTEGER NOT NULL, text TEXT NOT NULL,
                source TEXT DEFAULT 'Wiktionary', order_index INTEGER DEFAULT 0
            );
            INSERT INTO words (id, word, phonetic, translation, definition, pos, tag, bnc, frq, exchange, cefr)
                VALUES (1,'apple','','n. 苹果','','','',0,0,'','A1');
            INSERT INTO examples (word_id, text, source, order_index) VALUES
                (1,'She ate an apple.','Corpus',0),
                (1,'An apple a day.',           'proverb',1);",
        )
        .unwrap();
        drop(conn);
        (DictRepo::new(path.clone()), path)
    }

    #[test]
    fn cefr_and_examples_round_trip() {
        let (repo, path) = temp_word_repo();
        let w = repo.get_by_word("Apple").expect("word found");
        assert_eq!(w.cefr, "A1");
        assert!(w.has_cefr());
        let ex = repo.get_examples(w.id, 5);
        assert_eq!(ex.len(), 2);
        assert_eq!(ex[0].text, "She ate an apple.");
        assert_eq!(ex[1].source, "proverb");
        // unknown word id -> empty (and a missing table would also degrade to empty)
        assert!(repo.get_examples(999, 5).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    /// Build a words table (optionally with an FTS5 index) for search tests.
    fn temp_search_repo(with_fts: bool) -> (DictRepo, PathBuf) {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "synstudy_search_test_{}_{}.db",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE words (
                id INTEGER PRIMARY KEY, word TEXT NOT NULL UNIQUE,
                phonetic TEXT DEFAULT '', translation TEXT DEFAULT '',
                definition TEXT DEFAULT '', pos TEXT DEFAULT '', tag TEXT DEFAULT '',
                bnc INTEGER DEFAULT 0, frq INTEGER DEFAULT 0, exchange TEXT DEFAULT '', cefr TEXT DEFAULT ''
            );",
        )
        .unwrap();
        let rows = [
            (1i64, "achieve", "v. 实现；达成", 100i64),
            (2, "achievement", "n. 成就", 200),
            (3, "apple", "n. 苹果", 50),
            (4, "application", "n. 应用；申请", 80),
        ];
        for (id, word, trans, frq) in rows {
            conn.execute(
                "INSERT INTO words (id, word, translation, frq) VALUES (?1,?2,?3,?4)",
                rusqlite::params![id, word, trans, frq],
            )
            .unwrap();
        }
        if with_fts {
            conn.execute_batch(
                "CREATE VIRTUAL TABLE words_fts USING fts5(
                    word, translation, content='words', content_rowid='id');
                 INSERT INTO words_fts(rowid, word, translation)
                    SELECT id, word, translation FROM words;",
            )
            .expect("FTS5 must be available in the bundled SQLite build");
        }
        drop(conn);
        (DictRepo::new(path.clone()), path)
    }

    #[test]
    fn edit_distance_le1_handles_transposition_and_single_edits() {
        assert!(edit_distance_le1("acheive", "achieve")); // adjacent transposition
        assert!(edit_distance_le1("aple", "apple")); // one insertion
        assert!(edit_distance_le1("apple", "apples")); // trailing insert
        assert!(edit_distance_le1("cat", "bat")); // one substitution
        assert!(edit_distance_le1("apple", "apple")); // identical
        assert!(!edit_distance_le1("apple", "orange")); // far apart
        assert!(edit_distance_le1("ab", "abc")); // one insertion
        assert!(!edit_distance_le1("ab", "abcd")); // two insertions
    }

    #[test]
    fn fts_prefix_and_fulltext_search() {
        let (repo, path) = temp_search_repo(true);
        // Headword prefix.
        let hits = repo.search("ach", 10);
        let words: Vec<&str> = hits.iter().map(|w| w.word.as_str()).collect();
        assert!(words.contains(&"achieve"));
        assert!(words.contains(&"achievement"));
        // Full-text on the Chinese translation.
        let zh = repo.search("苹果", 10);
        assert!(zh.iter().any(|w| w.word == "apple"));
        // Exact headword ranks first.
        let exact = repo.search("apple", 10);
        assert_eq!(exact.first().map(|w| w.word.as_str()), Some("apple"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fuzzy_fallback_fixes_typo_without_fts() {
        let (repo, path) = temp_search_repo(false);
        // No FTS table -> search_like finds nothing for the typo, fuzzy recovers it.
        let hits = repo.search("acheive", 10);
        assert!(hits.iter().any(|w| w.word == "achieve"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fuzzy_fallback_fixes_typo_with_fts_present() {
        let (repo, path) = temp_search_repo(true);
        // FTS prefix on the typo yields nothing, so the fuzzy pass still runs.
        let hits = repo.search("aple", 10);
        assert!(hits.iter().any(|w| w.word == "apple"));
        let _ = std::fs::remove_file(&path);
    }

    /// Words + `word_roots`/`root_words` for root-query tests. Links are
    /// inserted out of order to prove `root_words` sorts by `order_index`.
    fn temp_root_repo() -> (DictRepo, PathBuf) {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "synstudy_root_test_{}_{}.db",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE words (
                id INTEGER PRIMARY KEY, word TEXT NOT NULL UNIQUE,
                phonetic TEXT DEFAULT '', translation TEXT DEFAULT '',
                definition TEXT DEFAULT '', pos TEXT DEFAULT '', tag TEXT DEFAULT '',
                bnc INTEGER DEFAULT 0, frq INTEGER DEFAULT 0, exchange TEXT DEFAULT '', cefr TEXT DEFAULT ''
            );
            CREATE TABLE word_roots (
                root_key TEXT PRIMARY KEY, class TEXT DEFAULT '', meaning TEXT DEFAULT '',
                origin TEXT DEFAULT '', synonyms TEXT DEFAULT '', antonyms TEXT DEFAULT ''
            );
            CREATE TABLE root_words (
                root_key TEXT NOT NULL, word_id INTEGER NOT NULL, order_index INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (root_key, word_id)
            );
            INSERT INTO words (id, word, translation) VALUES
                (1,'malfunction','n. 故障'),
                (2,'malware','n. 恶意软件');
            INSERT INTO word_roots (root_key, class, meaning, origin) VALUES
                ('mal','prefix','bad','Latin');
            INSERT INTO root_words (root_key, word_id, order_index) VALUES
                ('mal',2,1),('mal',1,0);",
        )
        .unwrap();
        drop(conn);
        (DictRepo::new(path.clone()), path)
    }

    #[test]
    fn roots_degrade_when_table_absent() {
        // temp_word_repo has no root tables -> queries degrade to empty.
        let (repo, path) = temp_word_repo();
        assert!(repo.roots().is_empty());
        assert!(repo.roots_for_word(1).is_empty());
        assert!(repo.root_words("mal").is_empty());
        assert_eq!(repo.root_total(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roots_round_trip_and_reverse_lookup() {
        let (repo, path) = temp_root_repo();
        let roots = repo.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].root_key, "mal");
        assert_eq!(roots[0].meaning, "bad");
        assert_eq!(roots[0].class, "prefix");
        // Ordered by order_index, not insertion order.
        let words = repo.root_words("mal");
        let ws: Vec<&str> = words.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(ws, vec!["malfunction", "malware"]);
        // Reverse lookup maps a word back to its root.
        let fw = repo.roots_for_word(words[0].id);
        assert_eq!(fw.len(), 1);
        assert_eq!(fw[0].root_key, "mal");
        assert_eq!(repo.root_total(), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// Fixture with a mini sentence_bank + sentence_words + words so the
    /// sentence queries can be exercised end-to-end.
    fn temp_sentence_repo() -> (DictRepo, PathBuf) {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "synstudy_sent_test_{}_{}.db",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE words (
                id INTEGER PRIMARY KEY, word TEXT NOT NULL UNIQUE,
                phonetic TEXT DEFAULT '', translation TEXT DEFAULT '',
                definition TEXT DEFAULT '', pos TEXT DEFAULT '', tag TEXT DEFAULT '',
                bnc INTEGER DEFAULT 0, frq INTEGER DEFAULT 0, exchange TEXT DEFAULT '', cefr TEXT DEFAULT ''
            );
            CREATE TABLE sentence_bank (
                id INTEGER PRIMARY KEY, text TEXT NOT NULL,
                translation TEXT DEFAULT '', source TEXT DEFAULT 'Tatoeba',
                source_id TEXT DEFAULT '', cefr_min TEXT DEFAULT '',
                exam_levels TEXT DEFAULT '', difficulty INTEGER DEFAULT 0, word_count INTEGER DEFAULT 0
            );
            CREATE TABLE sentence_words (
                sentence_id INTEGER NOT NULL, word_id INTEGER NOT NULL,
                token_index INTEGER NOT NULL, token_text TEXT DEFAULT '',
                is_target INTEGER DEFAULT 0,
                PRIMARY KEY (sentence_id, token_index)
            );
            INSERT INTO words (id, word, translation, tag) VALUES
                (1,'apple','n. 苹果','cet4'),
                (2,'ubiquitous','adj. 无处不在的','gre'),
                (3,'the','','');
            INSERT INTO sentence_bank (id, text, translation, source, source_id, cefr_min, exam_levels, difficulty, word_count) VALUES
                (10,'The apple is red.','苹果是红的。','Tatoeba','t10','A1','cet4',1,4),
                (11,'Smartphones are ubiquitous today.','','Tatoeba','t11','C2','gre',5,4),
                (12,'The apple and the phone.','','Tatoeba','t12','A1','cet4,gre',5,5);
            INSERT INTO sentence_words (sentence_id, word_id, token_index, token_text, is_target) VALUES
                (10, 3, 0, 'The', 0),
                (10, 1, 1, 'apple', 1),
                (11, 2, 2, 'ubiquitous', 1),
                (12, 3, 0, 'The', 0),
                (12, 1, 1, 'apple', 1);",
        )
        .unwrap();
        drop(conn);
        (DictRepo::new(path.clone()), path)
    }

    #[test]
    fn sentence_bank_degrades_when_absent() {
        // temp_word_repo has no sentence_bank/sentence_words tables.
        let (repo, path) = temp_word_repo();
        assert_eq!(repo.sentence_total(), 0);
        assert!(repo.sentences_for_word(1, None, 10).is_empty());
        assert!(repo.random_sentences(None, false, 5).is_empty());
        assert!(repo.sentence_by_id(1).is_none());
        assert!(repo.sentence_words(1, false).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sentences_for_word_filters_by_exam() {
        let (repo, path) = temp_sentence_repo();
        assert_eq!(repo.sentence_total(), 3);
        // apple appears in sentences 10 and 12 (both cet4).
        let apple_all = repo.sentences_for_word(1, None, 10);
        assert_eq!(apple_all.len(), 2);
        // Filtering to `gre` should drop sentence 10 (which is cet4-only).
        let apple_gre = repo.sentences_for_word(1, Some("gre"), 10);
        assert_eq!(apple_gre.len(), 1);
        assert_eq!(apple_gre[0].id, 12);
        // Translation-first ordering: sentence 10 (has zh) precedes 12.
        assert_eq!(apple_all[0].id, 10);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn random_sentences_respects_translation_and_exam_filters() {
        let (repo, path) = temp_sentence_repo();
        // require_translation=true -> only sentence 10 qualifies.
        let with_zh = repo.random_sentences(None, true, 10);
        assert_eq!(with_zh.len(), 1);
        assert_eq!(with_zh[0].id, 10);
        // exam=gre + has_target -> sentences 11 and 12.
        let gre = repo.random_sentences(Some("gre"), false, 10);
        let ids: std::collections::HashSet<i64> = gre.iter().map(|s| s.id).collect();
        assert!(ids.contains(&11));
        assert!(ids.contains(&12));
        // exam=gre + translation -> none of them have both.
        let gre_zh = repo.random_sentences(Some("gre"), true, 10);
        assert!(gre_zh.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sentence_words_target_filter_and_lookup() {
        let (repo, path) = temp_sentence_repo();
        let s = repo.sentence_by_id(10).expect("sentence 10");
        assert_eq!(s.text, "The apple is red.");
        assert!(s.has_exam("cet4"));
        assert!(!s.has_exam("gre"));
        assert_eq!(s.exam_labels(), vec!["四级"]);
        // All tokens, in order.
        let all = repo.sentence_words(10, false);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].token_text, "The");
        assert!(!all[0].is_target);
        // Only is_target=1 rows.
        let targets = repo.sentence_words(10, true);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].word_id, 1);
        assert!(targets[0].is_target);
        let _ = std::fs::remove_file(&path);
    }
}
