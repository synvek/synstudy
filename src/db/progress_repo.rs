use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::models::{CategoryStats, ProgressStatus, WordProgress};
use crate::state::{compute_streaks, day_index};

/// Sequential schema migrations, applied in order and tracked by
/// `PRAGMA user_version`. `MIGRATIONS[0]` brings a fresh database to version 1,
/// `MIGRATIONS[1]` to version 2, and so on.
///
/// A database created before versioning existed reports `user_version = 0`, so
/// it simply runs every migration from the start; each statement is written to
/// be safe against an already-populated schema. Later phases append new
/// tables/columns as additional entries here.
const MIGRATIONS: &[&str] = &[
    // v1: base schema (progress, settings, review log).
    "
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
",
    // v2: wordbook (starred words).
    "
CREATE TABLE IF NOT EXISTS starred (
    word_id  INTEGER PRIMARY KEY,
    added_at INTEGER NOT NULL DEFAULT 0,
    note     TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_starred_added ON starred(added_at);
",
    // v3: FSRS scheduler columns on word_progress.
    "
ALTER TABLE word_progress ADD COLUMN stability  REAL    NOT NULL DEFAULT 0;
ALTER TABLE word_progress ADD COLUMN difficulty REAL    NOT NULL DEFAULT 0;
ALTER TABLE word_progress ADD COLUMN scheduler  INTEGER NOT NULL DEFAULT 0;
",
];

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
        stability: r.get(9)?,
        difficulty: r.get(10)?,
        scheduler: r.get(11)?,
    })
}

const PROGRESS_COLS: &str = "word_id, category_id, status, reps, lapses, ease, \
                             interval_days, last_review, next_review, stability, \
                             difficulty, scheduler";

impl ProgressRepo {
    pub fn new(path: PathBuf) -> Self {
        let repo = Self { path };
        let _ = repo.ensure_schema();
        repo
    }

    fn conn(&self) -> rusqlite::Result<Connection> {
        Connection::open(&self.path)
    }

    /// Apply any pending migrations, advancing `PRAGMA user_version` one step
    /// at a time. Each version is applied inside its own transaction so a
    /// failure rolls back cleanly and can be retried on the next launch.
    pub fn ensure_schema(&self) -> rusqlite::Result<()> {
        let mut conn = self.conn()?;
        let mut version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);
        while (version as usize) < MIGRATIONS.len() {
            let sql = MIGRATIONS[version as usize];
            let next = version + 1;
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            // `user_version` accepts only a literal, not a bound parameter.
            tx.execute_batch(&format!("PRAGMA user_version = {next}"))?;
            tx.commit()?;
            version = next;
        }
        Ok(())
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
                 (word_id, category_id, status, reps, lapses, ease, interval_days, last_review, next_review, stability, difficulty, scheduler) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) \
                 ON CONFLICT(word_id, category_id) DO UPDATE SET \
                   status=excluded.status, reps=excluded.reps, lapses=excluded.lapses, \
                   ease=excluded.ease, interval_days=excluded.interval_days, \
                   last_review=excluded.last_review, next_review=excluded.next_review, \
                   stability=excluded.stability, difficulty=excluded.difficulty, \
                   scheduler=excluded.scheduler",
                params![
                    p.word_id,
                    p.category_id,
                    p.status as i64,
                    p.reps,
                    p.lapses,
                    p.ease,
                    p.interval_days,
                    p.last_review,
                    p.next_review,
                    p.stability,
                    p.difficulty,
                    p.scheduler
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

    /// "错词强化" candidates for a category: words that have lapsed at least
    /// once (or were rated `Again`, code 0, in the review log), ranked by a
    /// weakness score that combines `lapses` with the number of `Again`
    /// ratings. Weakest first, capped at `limit`.
    pub fn weak_word_ids(&self, category_id: i64, limit: i64) -> Vec<i64> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let sql = "SELECT wp.word_id FROM word_progress wp \
                   LEFT JOIN ( \
                       SELECT word_id, COUNT(*) AS again FROM review_log \
                       WHERE category_id = ?1 AND rating = 0 GROUP BY word_id \
                   ) rl ON rl.word_id = wp.word_id \
                   WHERE wp.category_id = ?1 \
                     AND (wp.lapses > 0 OR COALESCE(rl.again, 0) > 0) \
                   ORDER BY (wp.lapses + COALESCE(rl.again, 0)) DESC, wp.word_id ASC \
                   LIMIT ?2";
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map(params![category_id, limit], |r| r.get::<_, i64>(0))
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

    // ---------------- Statistics aggregation (D2) ----------------

    /// Number of reviews per UTC day for every `review_log` entry at or after
    /// `since_ts`. Keyed by `day_index`. Missing days are simply absent.
    pub fn review_day_counts(&self, since_ts: i64) -> HashMap<i64, i64> {
        let mut map = HashMap::new();
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return map,
        };
        let mut stmt = match conn
            .prepare("SELECT reviewed_at FROM review_log WHERE reviewed_at >= ?1")
        {
            Ok(s) => s,
            Err(_) => return map,
        };
        if let Ok(rows) = stmt.query_map(params![since_ts], |r| r.get::<_, i64>(0)) {
            for ts in rows.flatten() {
                *map.entry(day_index(ts)).or_insert(0) += 1;
            }
        }
        map
    }

    /// `(current_streak, longest_streak)` in days, across all categories, as of
    /// `now`.
    pub fn streaks(&self, now: i64) -> (i64, i64) {
        let counts = self.review_day_counts(0);
        let active: HashSet<i64> = counts
            .into_iter()
            .filter(|(_, c)| *c > 0)
            .map(|(d, _)| d)
            .collect();
        compute_streaks(&active, day_index(now))
    }

    /// Memory retention: `(Good + Easy) / total` reviews. `None` when there are
    /// no logged reviews. `category_id == None` aggregates every category.
    pub fn retention_rate(&self, category_id: Option<i64>) -> Option<f64> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return None,
        };
        // rating >= 2 counts Good(2) and Easy(3); Again(0)/Hard(1) do not.
        let expr = "COUNT(*) AS total, COALESCE(SUM(CASE WHEN rating >= 2 THEN 1 ELSE 0 END), 0) AS good";
        let row = match category_id {
            Some(c) => conn
                .query_row(
                    &format!("SELECT {expr} FROM review_log WHERE category_id = ?1"),
                    params![c],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )
                .ok(),
            None => conn
                .query_row(
                    &format!("SELECT {expr} FROM review_log"),
                    [],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )
                .ok(),
        };
        match row {
            Some((total, good)) if total > 0 => Some(good as f64 / total as f64),
            _ => None,
        }
    }

    /// Words becoming due over the next `days` UTC days, bucketed per day.
    /// Index 0 is today and also absorbs everything already overdue; later
    /// indices are future days. Covers all categories. Always returns `days`
    /// buckets (empty buckets are 0).
    pub fn due_forecast(&self, now: i64, days: i64) -> Vec<i64> {
        let len = days.max(1) as usize;
        let mut buckets = vec![0i64; len];
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return buckets,
        };
        let today = day_index(now);
        let mut stmt = match conn
            .prepare("SELECT next_review FROM word_progress WHERE next_review IS NOT NULL")
        {
            Ok(s) => s,
            Err(_) => return buckets,
        };
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, i64>(0)) {
            for ts in rows.flatten() {
                let off = day_index(ts) - today;
                let idx = if off < 0 { 0 } else { off };
                if idx < days {
                    buckets[idx as usize] += 1;
                }
            }
        }
        buckets
    }

    /// Total number of logged reviews (all categories).
    pub fn review_total(&self) -> i64 {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0))
            .unwrap_or(0)
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

    // ---------------- Wordbook (starred words) ----------------

    /// Whether `word_id` is in the wordbook.
    pub fn is_starred(&self, word_id: i64) -> bool {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return false,
        };
        conn.query_row(
            "SELECT 1 FROM starred WHERE word_id=?1",
            params![word_id],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// Add `word_id` to the wordbook. No-op when already present.
    pub fn add_star(&self, word_id: i64, now: i64) {
        if let Ok(conn) = self.conn() {
            let _ = conn.execute(
                "INSERT INTO starred (word_id, added_at) VALUES (?1,?2) \
                 ON CONFLICT(word_id) DO NOTHING",
                params![word_id, now],
            );
        }
    }

    /// Remove `word_id` from the wordbook.
    pub fn remove_star(&self, word_id: i64) {
        if let Ok(conn) = self.conn() {
            let _ = conn.execute("DELETE FROM starred WHERE word_id=?1", params![word_id]);
        }
    }

    /// Flip membership; returns the new state (`true` == now starred).
    pub fn toggle_star(&self, word_id: i64, now: i64) -> bool {
        if self.is_starred(word_id) {
            self.remove_star(word_id);
            false
        } else {
            self.add_star(word_id, now);
            true
        }
    }

    /// Starred word ids, most recently added first.
    pub fn list_starred(&self) -> Vec<i64> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn
            .prepare("SELECT word_id FROM starred ORDER BY added_at DESC, word_id DESC")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_path(tag: &str) -> PathBuf {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "synstudy_prog_{}_{}_{}.db",
            tag,
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn user_version(path: &PathBuf) -> i64 {
        let conn = Connection::open(path).unwrap();
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn fresh_db_migrates_to_latest_version() {
        let path = temp_path("fresh");
        let repo = ProgressRepo::new(path.clone());
        assert!(repo.ensure_schema().is_ok());
        assert_eq!(user_version(&path), MIGRATIONS.len() as i64);
        // Round-trip a progress row to prove the schema is usable.
        let mut p = WordProgress::new(1, 1);
        p.status = crate::models::ProgressStatus::Learning;
        repo.upsert(&p);
        assert_eq!(repo.get(1, 1).unwrap().status, crate::models::ProgressStatus::Learning);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unversioned_legacy_db_upgrades_and_keeps_data() {
        let path = temp_path("legacy");
        // Simulate an old database: base tables exist but user_version was never set.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.execute(
                "INSERT INTO word_progress (word_id, category_id, status, reps) VALUES (7, 3, 1, 5)",
                [],
            )
            .unwrap();
            // user_version stays 0 on purpose.
        }
        assert_eq!(user_version(&path), 0);
        let repo = ProgressRepo::new(path.clone());
        assert_eq!(user_version(&path), MIGRATIONS.len() as i64);
        // Pre-existing row survives the upgrade.
        let p = repo.get(7, 3).expect("legacy row preserved");
        assert_eq!(p.reps, 5);
        assert_eq!(p.status, crate::models::ProgressStatus::Learning);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        let path = temp_path("idem");
        let _ = ProgressRepo::new(path.clone());
        // Reopening must not error even though everything already exists.
        let repo = ProgressRepo::new(path.clone());
        assert!(repo.ensure_schema().is_ok());
        assert_eq!(user_version(&path), MIGRATIONS.len() as i64);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wordbook_star_toggle_and_list() {
        let path = temp_path("star");
        let repo = ProgressRepo::new(path.clone());
        assert!(!repo.is_starred(5));
        assert_eq!(repo.list_starred().len(), 0);
        // Toggle on, then off.
        assert!(repo.toggle_star(5, 100));
        assert!(repo.is_starred(5));
        assert!(!repo.toggle_star(5, 200));
        assert!(!repo.is_starred(5));
        // Multiple entries come back newest-first.
        repo.add_star(1, 10);
        repo.add_star(2, 30);
        repo.add_star(3, 20);
        // Re-adding is a no-op that preserves the original timestamp.
        repo.add_star(2, 99);
        assert_eq!(repo.list_starred(), vec![2, 3, 1]);
        repo.remove_star(3);
        assert_eq!(repo.list_starred(), vec![2, 1]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn weak_words_rank_by_lapses_and_again_ratings() {
        let path = temp_path("weak");
        let repo = ProgressRepo::new(path.clone());
        // Word 10: one lapse. Word 11: two lapses. Word 12: clean.
        for (id, lapses) in [(10, 1), (11, 2), (12, 0)] {
            let mut p = WordProgress::new(id, 3);
            p.lapses = lapses;
            repo.upsert(&p);
        }
        // Word 12 has no lapses but two `Again` (code 0) log entries.
        repo.log_review(12, 3, 0, 100);
        repo.log_review(12, 3, 0, 200);
        // Word 11 also logged one `Again` -> score 2 + 1 = 3 (weakest).
        repo.log_review(11, 3, 0, 300);
        // A different category must not leak in.
        let mut other = WordProgress::new(99, 4);
        other.lapses = 5;
        repo.upsert(&other);

        // Scores: 11 -> 3, 12 -> 2, 10 -> 1. Ties break by word_id ascending.
        assert_eq!(repo.weak_word_ids(3, 50), vec![11, 12, 10]);
        // Limit is respected.
        assert_eq!(repo.weak_word_ids(3, 2), vec![11, 12]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fsrs_columns_round_trip() {
        let path = temp_path("fsrs");
        let repo = ProgressRepo::new(path.clone());
        let mut p = WordProgress::new(4, 2);
        p.stability = 12.5;
        p.difficulty = 0.42;
        p.scheduler = 1;
        repo.upsert(&p);
        let got = repo.get(4, 2).expect("row present");
        assert_eq!(got.stability, 12.5);
        assert_eq!(got.difficulty, 0.42);
        assert_eq!(got.scheduler, 1);
        // Fresh rows default the FSRS columns to zero.
        let fresh = repo.get(99, 2);
        assert!(fresh.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn v2_db_upgrades_to_fsrs_columns() {
        let path = temp_path("v2up");
        // Simulate a version-2 database: base tables + starred, no FSRS columns.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.execute_batch(MIGRATIONS[1]).unwrap();
            conn.execute_batch("PRAGMA user_version = 2").unwrap();
            conn.execute(
                "INSERT INTO word_progress (word_id, category_id, status, reps) VALUES (5, 1, 2, 3)",
                [],
            )
            .unwrap();
        }
        assert_eq!(user_version(&path), 2);
        let repo = ProgressRepo::new(path.clone());
        assert_eq!(user_version(&path), MIGRATIONS.len() as i64);
        // Existing row survives and the new columns default to zero.
        let p = repo.get(5, 1).expect("row preserved");
        assert_eq!(p.reps, 3);
        assert_eq!(p.stability, 0.0);
        assert_eq!(p.difficulty, 0.0);
        assert_eq!(p.scheduler, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stats_aggregates_streaks_retention_forecast() {
        use crate::state::DAY_SECS;
        let path = temp_path("stats");
        let repo = ProgressRepo::new(path.clone());
        // "now" sits mid-day on UTC day 100.
        let now = 100 * DAY_SECS + 3600;

        // Active days 98, 99, 100 -> a 3-day current & longest streak.
        repo.log_review(1, 1, 2, 98 * DAY_SECS + 10);
        repo.log_review(1, 1, 3, 98 * DAY_SECS + 20);
        repo.log_review(2, 1, 0, 99 * DAY_SECS + 10);
        repo.log_review(2, 1, 2, 100 * DAY_SECS + 10);
        repo.log_review(2, 1, 1, 100 * DAY_SECS + 20);
        // A second category, to prove retention can be scoped.
        repo.log_review(3, 2, 0, 100 * DAY_SECS + 30);

        assert_eq!(repo.streaks(now), (3, 3));

        let counts = repo.review_day_counts(98 * DAY_SECS);
        assert_eq!(counts.get(&98), Some(&2));
        assert_eq!(counts.get(&99), Some(&1));
        assert_eq!(counts.get(&100), Some(&3));

        // Overall: 3 of 6 reviews rated Good/Easy.
        assert!((repo.retention_rate(None).unwrap() - 0.5).abs() < 1e-9);
        // Category 1: 3 of 5.
        assert!((repo.retention_rate(Some(1)).unwrap() - 0.6).abs() < 1e-9);
        // Category 2: the single review was `Again`.
        assert_eq!(repo.retention_rate(Some(2)), Some(0.0));
        // No reviews at all -> None.
        assert_eq!(repo.retention_rate(Some(999)), None);
        assert_eq!(repo.review_total(), 6);

        // Due forecast: one overdue, one due today, one in 2 days, one far out.
        let mut a = WordProgress::new(10, 1);
        a.next_review = Some(99 * DAY_SECS);
        let mut b = WordProgress::new(11, 1);
        b.next_review = Some(100 * DAY_SECS + 7200);
        let mut c = WordProgress::new(12, 1);
        c.next_review = Some(102 * DAY_SECS);
        let mut far = WordProgress::new(13, 1);
        far.next_review = Some(120 * DAY_SECS);
        for p in [&a, &b, &c, &far] {
            repo.upsert(p);
        }
        let f = repo.due_forecast(now, 7);
        assert_eq!(f.len(), 7);
        assert_eq!(f[0], 2); // overdue (day 99) + today (day 100)
        assert_eq!(f[1], 0);
        assert_eq!(f[2], 1); // day 102 == today + 2
        assert_eq!(f.iter().sum::<i64>(), 3); // the far-out word is excluded

        let _ = std::fs::remove_file(&path);
    }
}
