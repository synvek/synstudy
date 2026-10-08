use serde::{Deserialize, Serialize};

/// A dictionary entry loaded from the read-only database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub id: i64,
    pub word: String,
    pub phonetic: String,
    pub translation: String,
    pub definition: String,
    pub pos: String,
    pub tag: String,
    pub bnc: i64,
    pub frq: i64,
    pub exchange: String,
    /// CEFR-J level (A1..C2), empty when unlabeled. Source: olp-en-cefrj.
    pub cefr: String,
}

impl Word {
    /// Human readable list of exam tags, e.g. "四级 六级".
    pub fn tag_labels(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for t in self.tag.split_whitespace() {
            match t {
                "zk" => out.push("中考"),
                "gk" => out.push("高考"),
                "cet4" => out.push("四级"),
                "cet6" => out.push("六级"),
                "ky" => out.push("考研"),
                "toefl" => out.push("托福"),
                "ielts" => out.push("雅思"),
                "gre" => out.push("GRE"),
                _ => {}
            }
        }
        out
    }

    /// Whether a CEFR-J level is present for display.
    pub fn has_cefr(&self) -> bool {
        !self.cefr.trim().is_empty()
    }
}

/// A natural example sentence for a word, sourced from Wiktionary (kaikki.org).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    pub text: String,
    pub source: String,
}

/// A Chinese->English dictionary entry (CC-CEDICT), keyed by Chinese headword.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZhEntry {
    pub id: i64,
    pub traditional: String,
    pub simplified: String,
    pub pinyin: String,
    /// Raw glosses joined by '/', e.g. `apple/CL:個|个[ge4]/FIG. idiom/`.
    pub glosses: String,
}

impl ZhEntry {
    /// Individual gloss segments with blanks removed.
    pub fn gloss_list(&self) -> Vec<&str> {
        self.glosses
            .split('/')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// English headword candidates for cross-linking back to the ECDICT
    /// (英汉) dictionary. CEDICT meta segments (`CL:` classifiers, `see …`,
    /// `FIG. …`, regional `(Tw)` markers, etc.) are skipped; for each remaining
    /// gloss the first meaningful alphabetic token is taken and de-duplicated.
    pub fn english_heads(&self) -> Vec<String> {
        // Articles/prepositions that never stand alone as a headword.
        const STOP: &[&str] = &[
            "to", "a", "an", "the", "of", "in", "on", "for", "and", "or", "with", "by",
            "from", "at",
        ];
        // Whole-segment meta prefixes to ignore.
        const SKIP_PREFIX: &[&str] = &[
            "cl:", "see ", "also ", "old ", "variant", "erhua", "abbr", "loanword",
            "phonetic", "used ", "fig", "tw ", "prc",
        ];
        let mut out: Vec<String> = Vec::new();
        'seg: for seg in self.gloss_list() {
            let mut s = seg.trim();
            // Drop a leading parenthetical marker such as "(Tw)" / "(coll.)".
            if s.starts_with('(') {
                if let Some(idx) = s.find(')') {
                    s = s[idx + 1..].trim_start();
                }
            }
            let head = s.to_lowercase();
            for p in SKIP_PREFIX {
                if head.starts_with(p) {
                    continue 'seg;
                }
            }
            // First alphabetic token that is not a stopword.
            let mut token = String::new();
            for ch in s.chars() {
                if ch.is_ascii_alphabetic() {
                    token.push(ch.to_ascii_lowercase());
                } else if !token.is_empty() {
                    if token.len() >= 2 && !STOP.contains(&token.as_str()) {
                        break;
                    }
                    token.clear();
                }
            }
            if token.len() >= 2 && !STOP.contains(&token.as_str()) && !out.contains(&token) {
                out.push(token);
            }
            if out.len() >= 8 {
                break;
            }
        }
        out
    }
}

/// A vocabulary list (graded word bank).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub word_count: i64,
}

/// Learning state of a single word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressStatus {
    New = 0,
    Learning = 1,
    Reviewing = 2,
    Mastered = 3,
}

impl ProgressStatus {
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => ProgressStatus::Learning,
            2 => ProgressStatus::Reviewing,
            3 => ProgressStatus::Mastered,
            _ => ProgressStatus::New,
        }
    }

    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            ProgressStatus::New => "未学",
            ProgressStatus::Learning => "学习中",
            ProgressStatus::Reviewing => "复习中",
            ProgressStatus::Mastered => "已掌握",
        }
    }
}

/// Persisted per-word progress within a category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordProgress {
    pub word_id: i64,
    pub category_id: i64,
    pub status: ProgressStatus,
    pub reps: i64,
    pub lapses: i64,
    pub ease: f64,
    pub interval_days: f64,
    /// Unix timestamp (seconds) of the last review.
    pub last_review: Option<i64>,
    /// Unix timestamp (seconds) when the word becomes due again.
    pub next_review: Option<i64>,
}

impl WordProgress {
    pub fn new(word_id: i64, category_id: i64) -> Self {
        Self {
            word_id,
            category_id,
            status: ProgressStatus::New,
            reps: 0,
            lapses: 0,
            ease: 2.5,
            interval_days: 0.0,
            last_review: None,
            next_review: None,
        }
    }
}

/// Aggregate statistics for a category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CategoryStats {
    pub new_count: i64,
    pub learning: i64,
    pub reviewing: i64,
    pub mastered: i64,
    pub due: i64,
}

impl CategoryStats {
    pub fn started(&self) -> i64 {
        self.learning + self.reviewing + self.mastered
    }
}
