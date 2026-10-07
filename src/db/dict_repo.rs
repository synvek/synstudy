use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags, Row};

use crate::models::{Category, Word};

const WORD_COLS: &str = "w.id, w.word, w.phonetic, w.translation, w.definition, \
                         w.pos, w.tag, w.bnc, w.frq, w.exchange";

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
