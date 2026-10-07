//! Spaced repetition scheduling (a lightweight SM-2 variant).
//!
//! The algorithm is intentionally simple and deterministic so it can be unit
//! tested without a clock: callers pass `now` (unix seconds) explicitly.

use crate::models::{ProgressStatus, WordProgress};

/// Self-assessment rating after recalling a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    /// Forgot completely -> relearn soon.
    Again,
    /// Recalled with serious difficulty.
    Hard,
    /// Recalled with some effort.
    Good,
    /// Recalled effortlessly.
    Easy,
}

impl Rating {
    pub fn label(&self) -> &'static str {
        match self {
            Rating::Again => "重来",
            Rating::Hard => "困难",
            Rating::Good => "良好",
            Rating::Easy => "轻松",
        }
    }

    #[allow(dead_code)]
    pub fn key_hint(&self) -> &'static str {
        match self {
            Rating::Again => "1",
            Rating::Hard => "2",
            Rating::Good => "3",
            Rating::Easy => "4",
        }
    }

    /// Stable integer code used for the review log.
    pub fn code(self) -> i64 {
        match self {
            Rating::Again => 0,
            Rating::Hard => 1,
            Rating::Good => 2,
            Rating::Easy => 3,
        }
    }
}

const MIN_EASE: f64 = 1.3;
const MAX_EASE: f64 = 3.0;
/// Mastered threshold: interval in days at or beyond this counts as mastered.
const MASTERED_DAYS: f64 = 21.0;
/// Upper bound on the scheduling interval (~10 years) to keep growth finite.
const MAX_INTERVAL_DAYS: f64 = 3650.0;
/// "Again" relearning delay (10 minutes) expressed in seconds.
const AGAIN_DELAY_SECS: i64 = 10 * 60;
const DAY_SECS: i64 = 24 * 60 * 60;

/// Apply a rating to `progress`, mutating it in place, given the current time.
pub fn review(progress: &mut WordProgress, rating: Rating, now: i64) {
    progress.last_review = Some(now);

    match rating {
        Rating::Again => {
            progress.lapses += 1;
            progress.reps = 0;
            progress.interval_days = 0.0;
            progress.ease = (progress.ease - 0.2).max(MIN_EASE);
            progress.next_review = Some(now + AGAIN_DELAY_SECS);
            progress.status = ProgressStatus::Learning;
            return;
        }
        Rating::Hard => {
            progress.ease = (progress.ease - 0.15).max(MIN_EASE);
            let next = if progress.reps == 0 {
                1.0
            } else {
                (progress.interval_days * 1.2).max(1.0)
            };
            progress.interval_days = next;
        }
        Rating::Good => {
            let next = if progress.reps == 0 {
                1.0
            } else {
                (progress.interval_days * progress.ease).max(1.0)
            };
            progress.interval_days = next;
        }
        Rating::Easy => {
            progress.ease = (progress.ease + 0.15).min(MAX_EASE);
            let next = if progress.reps == 0 {
                3.0
            } else {
                (progress.interval_days * progress.ease * 1.3).max(3.0)
            };
            progress.interval_days = next;
        }
    }

    progress.reps += 1;
    let days = progress.interval_days.min(MAX_INTERVAL_DAYS);
    progress.interval_days = days;
    let secs = (days * DAY_SECS as f64).round() as i64;
    progress.next_review = Some(now.saturating_add(secs.max(60)));
    progress.status = if days >= MASTERED_DAYS {
        ProgressStatus::Mastered
    } else {
        ProgressStatus::Reviewing
    };
}

/// Number of words to introduce in a study session given remaining new words.
#[allow(dead_code)]
pub fn session_size(remaining_new: usize, words_per_session: usize) -> usize {
    remaining_new.min(words_per_session)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wp() -> WordProgress {
        WordProgress::new(1, 1)
    }

    #[test]
    fn again_resets_and_schedules_short_delay() {
        let mut p = wp();
        review(&mut p, Rating::Good, 1000);
        review(&mut p, Rating::Good, 2000);
        let reps_before = p.reps;
        review(&mut p, Rating::Again, 3000);
        assert_eq!(p.reps, 0);
        assert!(reps_before > 0);
        assert_eq!(p.lapses, 1);
        assert_eq!(p.status, ProgressStatus::Learning);
        assert_eq!(p.next_review, Some(3000 + AGAIN_DELAY_SECS));
    }

    #[test]
    fn good_first_time_is_one_day() {
        let mut p = wp();
        review(&mut p, Rating::Good, 0);
        assert_eq!(p.interval_days, 1.0);
        assert_eq!(p.next_review, Some(DAY_SECS));
        assert_eq!(p.status, ProgressStatus::Reviewing);
    }

    #[test]
    fn easy_first_time_is_three_days() {
        let mut p = wp();
        review(&mut p, Rating::Easy, 0);
        assert_eq!(p.interval_days, 3.0);
        assert_eq!(p.next_review, Some(3 * DAY_SECS));
    }

    #[test]
    fn intervals_grow_with_ease() {
        let mut p = wp();
        review(&mut p, Rating::Good, 0);
        let first = p.interval_days;
        review(&mut p, Rating::Good, 1_000_000);
        assert!(p.interval_days > first);
    }

    #[test]
    fn mastered_after_long_interval() {
        let mut p = wp();
        p.interval_days = 20.0;
        p.reps = 5;
        review(&mut p, Rating::Good, 0);
        assert!(p.interval_days >= MASTERED_DAYS);
        assert_eq!(p.status, ProgressStatus::Mastered);
    }

    #[test]
    fn ease_clamped() {
        let mut p = wp();
        for _ in 0..50 {
            review(&mut p, Rating::Again, 0);
        }
        assert!(p.ease >= MIN_EASE);
        let mut q = wp();
        for i in 0..50 {
            review(&mut q, Rating::Easy, i);
        }
        assert!(q.ease <= MAX_EASE);
    }

    #[test]
    fn session_size_caps() {
        assert_eq!(session_size(5, 20), 5);
        assert_eq!(session_size(50, 20), 20);
    }
}
