use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::models::{CategoryStats, ProgressStatus, WordProgress};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS word_progress (
    word_id       INTEGER NOT NULL,
    category_id   INTEGER NOT NULL,
    status        INTEGER NOT NULL DEFAULT 0,
    reps          INTEGER NOT NULL DEFAULT 0,
    lapses        INTEGER NOT NULL DEFAULT 0,
    ease          REAL    NOT NULL DEFAULT 2.5,
    interval_days REAL    NOT NULL DEFAULT 0,
    last_review   INTEGER,
    next_review   INTEGER,
    PRIMARY KEY (word_id, category_id)
);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT
);
CREATE TABLE IF NOT EXISTS review_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    word_id     INTEGER NOT NULL,
    category_id INTEGER NOT NULL,
    reviewed_at INTEGER NOT NULL,
    rating      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wp_cat ON word_progress(category_id);
CREATE INDEX IF NOT EXISTS idx_wp_due ON word_progress(category_id, next_review);
";

/// Writable store for learning progress and user settings.
#[derive(Clone, Debug)]
pub struct ProgressRepo {
    pub path: PathBuf,
}

fn row_to_progress(r: &rusqlite::Row) -> rusqlite::Result<WordProgress> {
    Ok(WordProgress {
        word_id: r.get(0)?,
        category_id: r.get(1)?,
        status: ProgressStatus::from_i64(r.get(2)?),
        reps: r.get(3)?,
        lapses: r.get(4)?,
        ease: r.get(5)?,
        interval_days: r.get(6)?,
        last_review: r.get(7)?,
        next_review: r.get(8)?,
    })
}

const PROGRESS_COLS: &str = "word_id, category_id, status, reps, lapses, ease, \
                             interval_days, last_review, next_review";

impl ProgressRepo {
    pub fn new(path: PathBuf) -> Self {
        let repo = Self { path };
        let _ = repo.ensure_schema();
        repo
    }

    fn conn(&self) -> rusqlite::Result<Connection> {
        Connection::open(&self.path)
    }

    pub fn ensure_schema(&self) -> rusqlite::Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(SCHEMA)
    }

    pub fn get(&self, word_id: i64, category_id: i64) -> Option<WordProgress> {
        let conn = self.conn().ok()?;
        let sql = format!("SELECT {PROGRESS_COLS} FROM word_progress WHERE word_id=?1 AND category_id=?2");
        conn.query_row(&sql, params![word_id, category_id], row_to_progress)
            .ok()
    }

    /// All progress rows for a category, keyed by word_id.
    pub fn map_for_category(&self, category_id: i64) -> HashMap<i64, WordProgress> {
        let mut map = HashMap::new();
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return map,
        };
        let sql = format!("SELECT {PROGRESS_COLS} FROM word_progress WHERE category_id=?1");
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(params![category_id], row_to_progress) {
                for p in rows.flatten() {
                    map.insert(p.word_id, p);
                }
            }
        }
        map
    }

    pub fn upsert(&self, p: &WordProgress) {
        if let Ok(conn) = self.conn() {
            let _ = conn.execute(
                "INSERT INTO word_progress \
                 (word_id, category_id, status, reps, lapses, ease, interval_days, last_review, next_review) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) \
                 ON CONFLICT(word_id, category_id) DO UPDATE SET \
                   status=excluded.status, reps=excluded.reps, lapses=excluded.lapses, \
                   ease=excluded.ease, interval_days=excluded.interval_days, \
                   last_review=excluded.last_review, next_review=excluded.next_review",
                params![
                    p.word_id,
                    p.category_id,
                    p.status as i64,
                    p.reps,
                    p.lapses,
                    p.ease,
                    p.interval_days,
                    p.last_review,
                    p.next_review
                ],
            );
        }
    }

    pub fn log_review(&self, word_id: i64, category_id: i64, rating: i64, now: i64) {
        if let Ok(conn) = self.conn() {
            let _ = conn.execute(
                "INSERT INTO review_log (word_id, category_id, reviewed_at, rating) VALUES (?1,?2,?3,?4)",
                params![word_id, category_id, now, rating],
            );
        }
    }

    /// Word ids in a category that are due for review at `now`.
    pub fn due_word_ids(&self, category_id: i64, now: i64) -> Vec<i64> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare(
            "SELECT word_id FROM word_progress \
             WHERE category_id=?1 AND next_review IS NOT NULL AND next_review <= ?2 \
             ORDER BY next_review ASC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![category_id, now], |r| r.get::<_, i64>(0))
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
    }

    pub fn stats(&self, category_id: i64, total: i64, now: i64) -> CategoryStats {
        let mut s = CategoryStats::default();
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return s,
        };
        if let Ok(mut stmt) = conn.prepare(
            "SELECT status, COUNT(*) FROM word_progress WHERE category_id=?1 GROUP BY status",
        ) {
            if let Ok(rows) = stmt.query_map(params![category_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            }) {
                for (status, count) in rows.flatten() {
                    match ProgressStatus::from_i64(status) {
                        ProgressStatus::Learning => s.learning += count,
                        ProgressStatus::Reviewing => s.reviewing += count,
                        ProgressStatus::Mastered => s.mastered += count,
                        ProgressStatus::New => {}
                    }
                }
            }
        }
        if let Ok(due) = conn.query_row(
            "SELECT COUNT(*) FROM word_progress WHERE category_id=?1 AND next_review IS NOT NULL AND next_review <= ?2",
            params![category_id, now],
            |r| r.get::<_, i64>(0),
        ) {
            s.due = due;
        }
        s.new_count = (total - s.started()).max(0);
        s
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = self.conn().ok()?;
        conn.query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| {
            r.get::<_, String>(0)
        })
        .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        if let Ok(conn) = self.conn() {
            let _ = conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1,?2) \
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            );
        }
    }

    pub fn reset_category(&self, category_id: i64) {
        if let Ok(conn) = self.conn() {
            let _ = conn.execute(
                "DELETE FROM word_progress WHERE category_id=?1",
                params![category_id],
            );
        }
    }

    pub fn reset_all(&self) {
        if let Ok(conn) = self.conn() {
            let _ = conn.execute("DELETE FROM word_progress", []);
            let _ = conn.execute("DELETE FROM review_log", []);
        }
    }
}
