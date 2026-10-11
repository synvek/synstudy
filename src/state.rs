use serde::{Deserialize, Serialize};

use crate::srs::Scheduler;

/// Top-level navigation destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dictionary,
    Categories,
    Study,
    Review,
    Wordbook,
    Quiz,
    Roots,
    Stats,
    Settings,
    /// Mobile-only "我的" hub: a card list of the secondary destinations.
    Mine,
}

impl Page {
    pub fn label(&self) -> &'static str {
        match self {
            Page::Dictionary => "词典",
            Page::Categories => "词库",
            Page::Study => "学习",
            Page::Review => "背诵",
            Page::Wordbook => "生词本",
            Page::Quiz => "测验",
            Page::Roots => "词根",
            Page::Stats => "统计",
            Page::Settings => "设置",
            Page::Mine => "我的",
        }
    }

    /// Pages shown as buttons in the navbar (Settings lives in the gear).
    pub fn nav_items() -> &'static [Page] {
        &[
            Page::Dictionary,
            Page::Categories,
            Page::Study,
            Page::Review,
            Page::Wordbook,
            Page::Quiz,
            Page::Roots,
            Page::Stats,
        ]
    }

    /// Ordered targets for the Cmd/Ctrl+1..N global shortcuts. `nav_items`
    /// first, then Settings, so digit N maps to `shortcut_pages()[N - 1]`.
    pub fn shortcut_pages() -> &'static [Page] {
        &[
            Page::Dictionary,
            Page::Categories,
            Page::Study,
            Page::Review,
            Page::Wordbook,
            Page::Quiz,
            Page::Roots,
            Page::Stats,
            Page::Settings,
        ]
    }
}

/// UI color scheme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    /// Follow the OS `prefers-color-scheme`.
    System,
    #[default]
    Light,
    Dark,
}

impl Theme {
    /// Value written to the root element's `data-theme` attribute.
    pub fn as_attr(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "跟随系统",
            Theme::Light => "浅色",
            Theme::Dark => "深色",
        }
    }
}

/// Spelling-quiz presentation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuizMode {
    /// See the Chinese meaning, type the English word.
    #[default]
    FromMeaning,
    /// Hear the word via TTS, type the English word.
    FromSound,
    /// Sentence cloze: pick the missing 考纲词 from 4 options.
    Cloze,
    /// Sentence translation: given the English sentence, pick the correct
    /// Chinese meaning of the highlighted target word.
    SentenceTranslation,
    /// Listen to the sentence, pick which target word was spoken.
    ListenChoose,
}

impl QuizMode {
    pub fn label(self) -> &'static str {
        match self {
            QuizMode::FromMeaning => "看释义拼写",
            QuizMode::FromSound => "听音拼写",
            QuizMode::Cloze => "选词填空",
            QuizMode::SentenceTranslation => "句意选择",
            QuizMode::ListenChoose => "听音辨词",
        }
    }

    /// Sentence-bank-backed modes; they draw questions from `sentence_bank`
    /// rather than the plain word list.
    pub fn is_sentence_mode(self) -> bool {
        matches!(
            self,
            QuizMode::Cloze | QuizMode::SentenceTranslation | QuizMode::ListenChoose
        )
    }

    /// All quiz modes in display order.
    pub fn all() -> &'static [QuizMode] {
        &[
            QuizMode::FromMeaning,
            QuizMode::FromSound,
            QuizMode::Cloze,
            QuizMode::SentenceTranslation,
            QuizMode::ListenChoose,
        ]
    }
}

/// Which exam syllabus the sentence-bank quiz should draw from. `All` uses the
/// full bank; the other variants restrict to sentences tagged with that key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExamFilter {
    #[default]
    All,
    Cet4,
    Cet6,
    Kaoyan,
    Tem4,
    Tem8,
    Toefl,
    Gre,
}

impl ExamFilter {
    /// The key used in `sentence_bank.exam_levels`, or `None` for `All`.
    pub fn key(self) -> Option<&'static str> {
        match self {
            ExamFilter::All => None,
            ExamFilter::Cet4 => Some("cet4"),
            ExamFilter::Cet6 => Some("cet6"),
            ExamFilter::Kaoyan => Some("ky"),
            ExamFilter::Tem4 => Some("tem4"),
            ExamFilter::Tem8 => Some("tem8"),
            ExamFilter::Toefl => Some("toefl"),
            ExamFilter::Gre => Some("gre"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExamFilter::All => "全部",
            ExamFilter::Cet4 => "四级",
            ExamFilter::Cet6 => "六级",
            ExamFilter::Kaoyan => "考研",
            ExamFilter::Tem4 => "专四",
            ExamFilter::Tem8 => "专八",
            ExamFilter::Toefl => "托福",
            ExamFilter::Gre => "GRE",
        }
    }

    pub fn all() -> &'static [ExamFilter] {
        &[
            ExamFilter::All,
            ExamFilter::Cet4,
            ExamFilter::Cet6,
            ExamFilter::Kaoyan,
            ExamFilter::Tem4,
            ExamFilter::Tem8,
            ExamFilter::Toefl,
            ExamFilter::Gre,
        ]
    }
}

/// Ordering applied when picking the next batch of new words in a study round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StudyOrder {
    /// Front-to-back of the category's `order_index`.
    #[default]
    Sequential,
    /// Back-to-front of the category's `order_index`.
    Reverse,
    /// Shuffled among the remaining new words.
    Random,
}

impl StudyOrder {
    pub fn label(self) -> &'static str {
        match self {
            StudyOrder::Sequential => "正序",
            StudyOrder::Reverse => "倒序",
            StudyOrder::Random => "随机",
        }
    }

    pub fn all() -> &'static [StudyOrder] {
        &[
            StudyOrder::Sequential,
            StudyOrder::Reverse,
            StudyOrder::Random,
        ]
    }

    /// Whether the underlying `category_word_ids` fetch should be reversed
    /// (`Random` fetches forward and shuffles afterwards).
    pub fn reverse_fetch(self) -> bool {
        matches!(self, StudyOrder::Reverse)
    }
}

/// Serde helper so boolean preferences can default to `true`.
fn default_true() -> bool {
    true
}

/// User preferences, persisted as JSON in the progress database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// How many new words to introduce per spelling-quiz round.
    pub words_per_session: usize,
    /// Ordering used when a study round picks new words.
    #[serde(default)]
    pub study_order: StudyOrder,
    /// How many new words each study round introduces (20 / 30 / 40 / 50).
    #[serde(default = "default_study_size")]
    pub study_size: usize,
    /// Color scheme. `#[serde(default)]` keeps older saved settings parseable.
    #[serde(default)]
    pub theme: Theme,
    /// Spelling-quiz mode (see meaning vs. hear audio).
    #[serde(default)]
    pub quiz_mode: QuizMode,
    /// Which exam syllabus to draw sentence-bank questions from.
    #[serde(default)]
    pub quiz_exam_filter: ExamFilter,
    /// Auto-play TTS when a quiz question appears.
    #[serde(default = "default_true")]
    pub quiz_auto_tts: bool,
    /// Record a missed quiz word as an `Again` review so it joins 错词本.
    #[serde(default = "default_true")]
    pub quiz_record_miss: bool,
    /// Master switch for text-to-speech playback.
    #[serde(default = "default_true")]
    pub tts_enabled: bool,
    /// Speech rate multiplier (0.5 – 2.0, 1.0 == normal).
    #[serde(default = "default_tts_rate")]
    pub tts_rate: f32,
    /// Auto-pronounce a word when its card is revealed / detail opens.
    #[serde(default)]
    pub tts_autoplay: bool,
    /// Spaced-repetition algorithm used when scheduling reviews.
    #[serde(default)]
    pub scheduler: Scheduler,
}

fn default_tts_rate() -> f32 {
    1.0
}

fn default_study_size() -> usize {
    20
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            words_per_session: 20,
            study_order: StudyOrder::default(),
            study_size: default_study_size(),
            theme: Theme::default(),
            quiz_mode: QuizMode::default(),
            quiz_exam_filter: ExamFilter::default(),
            quiz_auto_tts: true,
            quiz_record_miss: true,
            tts_enabled: true,
            tts_rate: default_tts_rate(),
            tts_autoplay: false,
            scheduler: Scheduler::default(),
        }
    }
}

pub const SETTINGS_KEY: &str = "settings";

/// Key under which the last picked study/review category id is remembered, so
/// reopening the app drops straight back into that word list instead of asking
/// to choose again. An empty value means "nothing selected yet".
pub const LAST_CATEGORY_KEY: &str = "last_category";

/// Seconds in a UTC day.
pub const DAY_SECS: i64 = 24 * 60 * 60;

/// Current unix time in seconds.
pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// UTC day number (whole days since 1970-01-01) for a unix timestamp. Uses
/// `div_euclid` so pre-epoch timestamps still bucket by calendar day.
pub fn day_index(ts: i64) -> i64 {
    ts.div_euclid(DAY_SECS)
}

/// Weekday of a day index: 0 = Sunday .. 6 = Saturday. The epoch
/// (1970-01-01) was a Thursday, hence the `+ 4` offset.
pub fn weekday_sun0(day: i64) -> i64 {
    (day + 4).rem_euclid(7)
}

/// Given the set of day indices that had at least one review and `today`'s day
/// index, return `(current_streak, longest_streak)` in days.
///
/// The current streak tolerates today not being studied yet: it starts from
/// today when active, otherwise from yesterday, and walks backwards. If neither
/// today nor yesterday is active the current streak is 0.
pub fn compute_streaks(active: &std::collections::HashSet<i64>, today: i64) -> (i64, i64) {
    if active.is_empty() {
        return (0, 0);
    }
    // Longest: scan sorted days, counting consecutive runs.
    let mut days: Vec<i64> = active.iter().copied().collect();
    days.sort_unstable();
    let mut longest = 1i64;
    let mut run = 1i64;
    for w in days.windows(2) {
        if w[1] == w[0] + 1 {
            run += 1;
        } else {
            run = 1;
        }
        longest = longest.max(run);
    }
    // Current: allow a grace day for "not yet studied today".
    let mut cursor = today;
    if !active.contains(&cursor) {
        cursor = today - 1;
    }
    let mut current = 0i64;
    while active.contains(&cursor) {
        current += 1;
        cursor -= 1;
    }
    (current, longest)
}

/// First non-empty line of a multi-line translation/definition.
pub fn first_line(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set(days: &[i64]) -> HashSet<i64> {
        days.iter().copied().collect()
    }

    #[test]
    fn day_index_buckets_by_utc_day() {
        assert_eq!(day_index(0), 0);
        assert_eq!(day_index(DAY_SECS - 1), 0); // last second of day 0
        assert_eq!(day_index(DAY_SECS), 1); // first second of day 1
        assert_eq!(day_index(DAY_SECS * 3 + 100), 3);
        // Pre-epoch timestamps still floor to the earlier day.
        assert_eq!(day_index(-1), -1);
        assert_eq!(day_index(-DAY_SECS), -1);
        assert_eq!(day_index(-DAY_SECS - 1), -2);
    }

    #[test]
    fn weekday_matches_known_epoch_dates() {
        // 1970-01-01 was a Thursday -> 4 (Sun=0).
        assert_eq!(weekday_sun0(0), 4);
        // 1970-01-04 was a Sunday -> 0.
        assert_eq!(weekday_sun0(3), 0);
        // 1970-01-07 was a Wednesday -> 3.
        assert_eq!(weekday_sun0(6), 3);
        // Wraps within 0..6 for every day of a week.
        for d in 0..7 {
            assert!((0..7).contains(&weekday_sun0(d)));
        }
    }

    #[test]
    fn streaks_empty_is_zero() {
        assert_eq!(compute_streaks(&set(&[]), 10), (0, 0));
    }

    #[test]
    fn current_streak_counts_back_from_today() {
        let active = set(&[8, 9, 10]);
        assert_eq!(compute_streaks(&active, 10), (3, 3));
    }

    #[test]
    fn current_streak_grace_when_today_not_studied() {
        // Today (10) has no review yet, but yesterday does -> streak still alive.
        let active = set(&[8, 9]);
        assert_eq!(compute_streaks(&active, 10), (2, 2));
    }

    #[test]
    fn current_streak_broken_after_gap() {
        // Neither today (10) nor yesterday (9) active -> current resets to 0,
        // while the older run of 3 remains the longest.
        let active = set(&[5, 6, 7]);
        assert_eq!(compute_streaks(&active, 10), (0, 3));
    }

    #[test]
    fn longest_streak_spans_a_gap() {
        let active = set(&[1, 2, 3, 4, 10, 11]);
        // today == 11 -> current 2 (10,11); longest run is 1..4 == 4.
        assert_eq!(compute_streaks(&active, 11), (2, 4));
    }
}
