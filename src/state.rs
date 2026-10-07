use serde::{Deserialize, Serialize};

/// Top-level navigation destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dictionary,
    Categories,
    Study,
    Review,
    Stats,
    Settings,
}

impl Page {
    pub fn label(&self) -> &'static str {
        match self {
            Page::Dictionary => "词典",
            Page::Categories => "词库",
            Page::Study => "学习",
            Page::Review => "背诵",
            Page::Stats => "统计",
            Page::Settings => "设置",
        }
    }

    pub fn nav_items() -> [Page; 5] {
        [
            Page::Dictionary,
            Page::Categories,
            Page::Study,
            Page::Review,
            Page::Stats,
        ]
    }
}

/// User preferences, persisted as JSON in the progress database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// How many new words to introduce per study session.
    pub words_per_session: usize,
    /// Study/review word lists in reverse order.
    pub reverse: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            words_per_session: 20,
            reverse: false,
        }
    }
}

pub const SETTINGS_KEY: &str = "settings";

/// Current unix time in seconds.
pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// First non-empty line of a multi-line translation/definition.
pub fn first_line(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}
