use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags, Row};

use crate::models::{Category, Example, Word, ZhEntry};

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

    /// Prefix search on the headword. Exact matches rank first, then by
    /// frequency (lower `frq` == more common).
    pub fn search(&self, query: &str, limit: i64) -> Vec<Word> {
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
}
