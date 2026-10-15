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

/// A synonym / antonym relation for a word (Wiktionary/kaikki, `word_relations`
/// table). `kind` is `"syn"` or `"ant"`; `target` is a lowercase headword that
/// exists in `words`, so the UI can resolve it for a click-through lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordRelation {
    pub kind: String,
    pub target: String,
}

/// A graded sentence from the open-corpus sentence bank (Tatoeba / Wiktionary).
///
/// Each row is tagged with the exam levels whose syllabus vocabulary appears in
/// the sentence, so the runtime can drive 四级/六级/专四/专八/托福/GRE-specific
/// practice without redistributing any copyrighted 真题 content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sentence {
    pub id: i64,
    pub text: String,
    /// Chinese translation when the Tatoeba links file provided one; empty
    /// otherwise. Sentence-translation quizzes require a non-empty value.
    pub translation: String,
    pub source: String,
    pub source_id: String,
    /// Minimum CEFR level across the sentence's content words (A1..C2), or
    /// empty when no CEFR-tagged word was matched.
    pub cefr_min: String,
    /// Comma-separated exam tags: any of `cet4,cet6,ky,tem4,tem8,toefl,ielts,gre`.
    pub exam_levels: String,
    /// 1..=5 tier derived from the hardest exam tag present.
    pub difficulty: i64,
    /// Token count of `text` (whitespace split).
    pub word_count: i64,
}

impl Sentence {
    /// Whether the sentence is tagged with the given exam key (e.g. `cet4`).
    #[allow(dead_code)]
    pub fn has_exam(&self, exam: &str) -> bool {
        self.exam_levels
            .split(',')
            .any(|t| t.eq_ignore_ascii_case(exam))
    }

    /// Human-readable Chinese labels for the tagged exam levels.
    pub fn exam_labels(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for t in self.exam_levels.split(',') {
            match t {
                "cet4" => out.push("四级"),
                "cet6" => out.push("六级"),
                "tem4" => out.push("专四"),
                "tem8" => out.push("专八"),
                "toefl" => out.push("托福"),
                "gre" => out.push("GRE"),
                _ => {}
            }
        }
        out
    }
}

/// A word occurrence inside a `sentence_bank` row. `token_index` is the
/// whitespace-token position and `token_text` is the raw token as it appears
/// (so the cloze quiz can blank it out while preserving punctuation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentenceWord {
    pub sentence_id: i64,
    pub word_id: i64,
    pub token_index: i64,
    pub token_text: String,
    /// 1 when the word is part of an exam syllabus (a valid cloze target),
    /// 0 for filler words kept only for context.
    pub is_target: bool,
}

/// A word root / affix entry (ECDICT wordroot data). The words that use it are
/// linked separately through the `root_words` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordRoot {
    /// Stable key, e.g. `hom` or `-less`.
    pub root_key: String,
    /// Grammatical class, e.g. `root` / `adjective-forming suffix`.
    pub class: String,
    pub meaning: String,
    pub origin: String,
    /// Comma-separated related roots/affixes.
    pub synonyms: String,
    pub antonyms: String,
    /// Usage commentary from the source's `function` field (affix formation
    /// rules); empty for most plain roots.
    pub note: String,
    /// Raw example-word list (comma-separated), kept for display when the
    /// linked `root_words` rows cover only part of it.
    pub examples: String,
}

impl WordRoot {
    /// Chinese caption for the grammatical class; unknown values pass through.
    pub fn class_label(&self) -> &str {
        match self.class.as_str() {
            "root" => "词根",
            "prefix" => "前缀",
            "noun-forming suffix" => "名词后缀",
            "adjective-forming suffix" => "形容词后缀",
            "verb-forming suffix" => "动词后缀",
            "adverb-forming suffix" => "副词后缀",
            "adjective- and noun-forming suffix" => "形容词/名词后缀",
            "" => "",
            other => other,
        }
    }

    /// Chinese caption for the language of origin; unknown values pass through.
    pub fn origin_label(&self) -> &str {
        match self.origin.as_str() {
            "Latin" => "拉丁语",
            "Greek" => "希腊语",
            "Latin and Greek" => "拉丁语/希腊语",
            "Old English" => "古英语",
            "Middle English" => "中古英语",
            "" => "",
            other => other,
        }
    }
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
    /// FSRS memory stability (days). 0 until an FSRS review runs.
    pub stability: f64,
    /// FSRS card difficulty (0..1). 0 until an FSRS review runs.
    pub difficulty: f64,
    /// Scheduler that produced the current state: 0 = SM-2 lite, 1 = FSRS.
    pub scheduler: i64,
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
            stability: 0.0,
            difficulty: 0.0,
            scheduler: 0,
            last_review: None,
            next_review: None,
        }
    }
}

/// One study round: a fixed batch of new words introduced together, with the
/// 认识/不认识 result recorded per word so a finished round can be reviewed
/// later. Persisted in the writable progress database (`study_round` /
/// `study_round_word`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyRound {
    pub id: i64,
    pub category_id: i64,
    /// 1-based round number within the category.
    pub number: i64,
    /// Ordering used when the round was started: `seq` / `reverse` / `random`.
    pub order_mode: String,
    /// How many words the round introduced.
    pub size: i64,
    pub created_at: i64,
    /// Set once every word is answered; `None` while still in progress.
    pub finished_at: Option<i64>,
    /// Words answered 认识.
    pub known_count: i64,
    /// Words answered 不认识.
    pub unknown_count: i64,
}

impl StudyRound {
    /// How many words in the round already have a recorded result.
    pub fn answered(&self) -> i64 {
        self.known_count + self.unknown_count
    }

    /// Human-readable ordering label matching the round's stored `order_mode`.
    pub fn order_label(&self) -> &'static str {
        match self.order_mode.as_str() {
            "reverse" => "倒序",
            "random" => "随机",
            _ => "正序",
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
