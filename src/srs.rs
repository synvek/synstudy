//! Spaced repetition scheduling (a lightweight SM-2 variant).
//!
//! The algorithm is intentionally simple and deterministic so it can be unit
//! tested without a clock: callers pass `now` (unix seconds) explicitly.

use crate::models::{ProgressStatus, WordProgress};
use serde::{Deserialize, Serialize};

/// Which spaced-repetition algorithm schedules a review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scheduler {
    /// The built-in lightweight SM-2 variant (default).
    #[default]
    Sm2,
    /// Free Spaced Repetition Scheduler (`fsrs` crate).
    Fsrs,
}

impl Scheduler {
    /// Stable integer code persisted on each progress row.
    pub fn code(self) -> i64 {
        match self {
            Scheduler::Sm2 => 0,
            Scheduler::Fsrs => 1,
        }
    }

    #[allow(dead_code)]
    pub fn from_code(v: i64) -> Self {
        match v {
            1 => Scheduler::Fsrs,
            _ => Scheduler::Sm2,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scheduler::Sm2 => "SM-2 lite（默认）",
            Scheduler::Fsrs => "FSRS",
        }
    }
}

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

    /// Digit shown on the rating buttons and bound as a keyboard shortcut.
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

/// Desired memory retention targeted by the FSRS scheduler.
const FSRS_DESIRED_RETENTION: f32 = 0.9;

/// Schedule `progress` for `rating` at `now` using the selected `algo`. The
/// SM-2 branch is unchanged; the FSRS branch additionally reads and writes the
/// `stability`/`difficulty` columns so state survives across reviews.
pub fn schedule(progress: &mut WordProgress, rating: Rating, now: i64, algo: Scheduler) {
    match algo {
        Scheduler::Sm2 => review(progress, rating, now),
        Scheduler::Fsrs => review_fsrs(progress, rating, now),
    }
}

/// FSRS-backed scheduling. Any computation error falls back to the SM-2 path so
/// a review is never lost.
fn review_fsrs(progress: &mut WordProgress, rating: Rating, now: i64) {
    use fsrs::{MemoryState, FSRS};
    let fsrs = FSRS::default();
    let previous = if progress.reps > 0 && progress.stability > 0.0 {
        Some(MemoryState {
            stability: progress.stability as f32,
            difficulty: progress.difficulty as f32,
        })
    } else {
        None
    };
    let elapsed_days = match progress.last_review {
        Some(last) if now > last => ((now - last) / DAY_SECS).clamp(0, u32::MAX as i64) as u32,
        _ => 0,
    };
    let next = match fsrs.next_states(previous, FSRS_DESIRED_RETENTION, elapsed_days) {
        Ok(n) => n,
        Err(_) => return review(progress, rating, now),
    };
    let state = match rating {
        Rating::Again => next.again,
        Rating::Hard => next.hard,
        Rating::Good => next.good,
        Rating::Easy => next.easy,
    };
    progress.stability = state.memory.stability as f64;
    progress.difficulty = state.memory.difficulty as f64;
    progress.last_review = Some(now);
    progress.scheduler = Scheduler::Fsrs.code();

    if rating == Rating::Again {
        progress.lapses += 1;
        progress.reps = 0;
        progress.interval_days = 0.0;
        progress.next_review = Some(now + AGAIN_DELAY_SECS);
        progress.status = ProgressStatus::Learning;
        return;
    }

    progress.reps += 1;
    let days = (state.interval.round().max(1.0) as f64).min(MAX_INTERVAL_DAYS);
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

    #[test]
    fn scheduler_code_round_trip() {
        assert_eq!(Scheduler::from_code(Scheduler::Sm2.code()), Scheduler::Sm2);
        assert_eq!(Scheduler::from_code(Scheduler::Fsrs.code()), Scheduler::Fsrs);
        // Unknown codes degrade to the default SM-2.
        assert_eq!(Scheduler::from_code(99), Scheduler::Sm2);
        assert_eq!(Scheduler::default(), Scheduler::Sm2);
    }

    #[test]
    fn fsrs_writes_state_and_grows_interval() {
        let mut p = wp();
        schedule(&mut p, Rating::Good, 0, Scheduler::Fsrs);
        // FSRS stamps its own state on the row.
        assert_eq!(p.scheduler, Scheduler::Fsrs.code());
        assert!(p.stability > 0.0);
        assert!(p.difficulty > 0.0);
        assert_eq!(p.reps, 1);
        assert!(p.interval_days >= 1.0);
        let first = p.interval_days;
        // A later `Easy` review extends the interval monotonically.
        schedule(&mut p, Rating::Easy, 1_000_000, Scheduler::Fsrs);
        assert!(p.interval_days >= first);
        assert_eq!(p.reps, 2);
        assert!(p.next_review.unwrap() > 1_000_000);
    }

    #[test]
    fn fsrs_again_resets_to_learning() {
        let mut p = wp();
        schedule(&mut p, Rating::Good, 0, Scheduler::Fsrs);
        schedule(&mut p, Rating::Again, 10_000, Scheduler::Fsrs);
        assert_eq!(p.reps, 0);
        assert_eq!(p.lapses, 1);
        assert_eq!(p.status, ProgressStatus::Learning);
        assert_eq!(p.next_review, Some(10_000 + AGAIN_DELAY_SECS));
    }

    #[test]
    fn switching_scheduler_does_not_panic() {
        // A row scheduled by FSRS can be handed back to SM-2 and vice versa.
        let mut p = wp();
        schedule(&mut p, Rating::Good, 0, Scheduler::Fsrs);
        schedule(&mut p, Rating::Good, 100_000, Scheduler::Sm2);
        assert_eq!(p.scheduler, Scheduler::Fsrs.code()); // SM-2 leaves the tag alone
        schedule(&mut p, Rating::Good, 200_000, Scheduler::Fsrs);
        assert!(p.next_review.is_some());
    }
}
