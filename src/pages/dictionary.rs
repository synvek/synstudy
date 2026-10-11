use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::word_detail::WordDetail;
use crate::components::zh_detail::ZhDetail;
use crate::db::dict_repo::DictRepo;
use crate::db::progress_repo::ProgressRepo;
use crate::models::{Word, ZhEntry};
use crate::state::first_line;

/// Lookup direction of the dictionary page. Serialized into the persisted
/// search history, so variants stay name-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum SearchMode {
    /// English headword -> Chinese meaning (ECDICT).
    #[default]
    En2Zh,
    /// Chinese headword -> English meaning (CC-CEDICT).
    Zh2En,
}

impl SearchMode {
    fn title(self) -> &'static str {
        match self {
            SearchMode::En2Zh => "英汉词典",
            SearchMode::Zh2En => "汉英词典",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            SearchMode::En2Zh => "输入英文单词查询，如 achieve …",
            SearchMode::Zh2En => "输入中文或拼音查询，如 苹果 / pingguo …",
        }
    }

    fn empty_hint(self) -> &'static str {
        match self {
            SearchMode::En2Zh => "输入单词开始查询",
            SearchMode::Zh2En => "输入中文或拼音开始查询",
        }
    }

    fn eyebrow(self) -> &'static str {
        match self {
            SearchMode::En2Zh => "YOUR EVERYDAY WORD COMPANION",
            SearchMode::Zh2En => "REVERSE LOOKUP · 汉英词典",
        }
    }
}

/// One persisted dictionary lookup: which direction and what was looked up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct HistoryItem {
    mode: SearchMode,
    text: String,
}

/// KV key under which the search history is stored in the progress database.
const DICT_HISTORY_KEY: &str = "dict_history";
/// Most recent lookups kept per direction; oldest entries fall off the tail.
const HISTORY_LIMIT: usize = 20;

/// Handle for recording lookups into the history signal. A `Copy` struct (a
/// bare `FnMut` closure can't be moved into several event handlers).
#[derive(Clone, Copy)]
struct HistoryAdder {
    history: Signal<Vec<HistoryItem>>,
}

impl HistoryAdder {
    /// Newest first, deduped by (mode, text); each direction capped at 20.
    fn add(self, m: SearchMode, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let key = (m, text.clone());
        let cur = self.history.read().clone();
        let mut next = vec![HistoryItem {
            mode: m,
            text: text.clone(),
        }];
        let mut hist = self.history;
        let mut kept = 0usize;
        for it in cur {
            let dup = it.mode == key.0 && it.text == key.1;
            if it.mode == m {
                if !dup {
                    if kept >= HISTORY_LIMIT {
                        continue;
                    }
                    kept += 1;
                    next.push(it);
                }
            } else {
                next.push(it);
            }
        }
        hist.set(next);
    }
}

/// Right-hand caption of the results panel header.
fn result_status(empty_query: bool, no_hits: bool) -> &'static str {
    if empty_query {
        "等待查询"
    } else if no_hits {
        "无匹配"
    } else {
        "相关词条"
    }
}

#[component]
pub fn Dictionary() -> Element {
    let dict: DictRepo = use_context();
    let progress: ProgressRepo = use_context();
    let mut query = use_signal(String::new);
    let mut mode = use_signal(SearchMode::default);
    let mut selected = use_signal(|| Option::<Word>::None);
    let mut selected_zh = use_signal(|| Option::<ZhEntry>::None);

    // Search history, restored once from the progress KV store and written
    // back whenever it changes. Reads only subscribe to the signal itself.
    let history = {
        let progress = progress.clone();
        use_signal(move || {
            progress
                .get_setting(DICT_HISTORY_KEY)
                .and_then(|v| serde_json::from_str::<Vec<HistoryItem>>(&v).ok())
                .unwrap_or_default()
        })
    };
    {
        let progress = progress.clone();
        use_effect(move || {
            let items = history.read().clone();
            match serde_json::to_string(&items) {
                Ok(json) => progress.set_setting(DICT_HISTORY_KEY, &json),
                Err(_) => progress.set_setting(DICT_HISTORY_KEY, "[]"),
            }
        });
    }
    let add_history = HistoryAdder { history };

    // Each direction has its own memo; the inactive one short-circuits to empty.
    let dict_en = dict.clone();
    let results_en = use_memo(move || {
        if mode() != SearchMode::En2Zh {
            return Vec::<Word>::new();
        }
        let text = query.read().clone();
        dict_en.search(&text, 40)
    });
    let dict_zh = dict.clone();
    let results_zh = use_memo(move || {
        if mode() != SearchMode::Zh2En {
            return Vec::<ZhEntry>::new();
        }
        let text = query.read().clone();
        dict_zh.search_zh(&text, 60)
    });
    let dict_counts = dict.clone();
    let counts = use_memo(move || (dict_counts.word_total(), dict_counts.zh_total()));

    let cur_mode = mode();
    let en_list = results_en();
    let zh_list = results_zh();
    let sel = selected().clone();
    let sel_zh = selected_zh().clone();
    let selected_id = sel.as_ref().map(|w| w.id);
    let selected_zh_id = sel_zh.as_ref().map(|e| e.id);
    // On narrow screens the layout pushes to the detail pane once an entry is
    // picked; `has-detail` drives that CSS master-detail toggle.
    let has_detail = sel.is_some() || sel_zh.is_some();
    let (word_total, zh_total) = counts();
    // Current-direction lookups shown as quick-pick chips while the query box
    // is empty (oldest entries beyond the cap are dropped on write).
    let hist_list: Vec<HistoryItem> = history
        .read()
        .iter()
        .filter(|it| it.mode == cur_mode)
        .cloned()
        .collect();
    let count_label = if cur_mode == SearchMode::Zh2En {
        format!("共收录 {} 汉英词条", zh_total)
    } else {
        format!("共收录 {} 词", word_total)
    };

    // Arrow keys move the highlighted result; Enter opens the first/current one.
    let onkey_search = {
        let en = en_list.clone();
        let zh = zh_list.clone();
        let mode = cur_mode;
        move |evt: Event<KeyboardData>| {
            let delta = match evt.code() {
                Code::ArrowDown => 1,
                Code::ArrowUp => -1,
                Code::Enter | Code::NumpadEnter => 0,
                _ => return,
            };
            evt.prevent_default();
            if mode == SearchMode::En2Zh {
                if en.is_empty() {
                    return;
                }
                let cur = selected().and_then(|s| en.iter().position(|w| w.id == s.id));
                let next = match (delta, cur) {
                    (0, c) => c.unwrap_or(0),
                    (_, Some(c)) => {
                        let n = en.len() as i64;
                        (((c as i64 + delta) % n + n) % n) as usize
                    }
                    (_, None) => if delta > 0 { 0 } else { en.len() - 1 },
                };
                if let Some(w) = en.get(next) {
                    add_history.add(mode, w.word.clone());
                    selected.set(Some(w.clone()));
                }
            } else {
                if zh.is_empty() {
                    return;
                }
                let cur = selected_zh().and_then(|s| zh.iter().position(|e| e.id == s.id));
                let next = match (delta, cur) {
                    (0, c) => c.unwrap_or(0),
                    (_, Some(c)) => {
                        let n = zh.len() as i64;
                        (((c as i64 + delta) % n + n) % n) as usize
                    }
                    (_, None) => if delta > 0 { 0 } else { zh.len() - 1 },
                };
                if let Some(e) = zh.get(next) {
                    add_history.add(mode, e.simplified.clone());
                    selected_zh.set(Some(e.clone()));
                }
            }
        }
    };

    // The 查询 button opens the first matching entry (same as pressing Enter).
    let open_first = {
        let en = en_list.clone();
        let zh = zh_list.clone();
        move |_| {
            if cur_mode == SearchMode::En2Zh {
                if let Some(w) = en.first() {
                    add_history.add(cur_mode, w.word.clone());
                    selected.set(Some(w.clone()));
                }
            } else if let Some(e) = zh.first() {
                add_history.add(cur_mode, e.simplified.clone());
                selected_zh.set(Some(e.clone()));
            }
        }
    };

    rsx! {
        div { class: "page dictionary",
            div { class: "page-hero",
                div {
                    div { class: "eyebrow", "{cur_mode.eyebrow()}" }
                    h1 { "{cur_mode.title()}" }
                    p { class: "sub", "{count_label}" }
                }
                div { class: "pill-group",
                    button {
                        class: if cur_mode == SearchMode::En2Zh { "pill active" } else { "pill" },
                        onclick: move |_| {
                            mode.set(SearchMode::En2Zh);
                            query.set(String::new());
                            selected.set(None);
                            selected_zh.set(None);
                        },
                        "英汉"
                    }
                    button {
                        class: if cur_mode == SearchMode::Zh2En { "pill active" } else { "pill" },
                        onclick: move |_| {
                            mode.set(SearchMode::Zh2En);
                            query.set(String::new());
                            selected.set(None);
                            selected_zh.set(None);
                        },
                        "汉英"
                    }
                }
            }

            div { class: "search-box",
                svg {
                    class: "search-ico", width: "20", height: "20",
                    view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
                    stroke_width: "2", stroke_linecap: "round",
                    circle { cx: "11", cy: "11", r: "7" }
                    path { d: "m21 21-4.3-4.3" }
                }
                input {
                    r#type: "text",
                    placeholder: "{cur_mode.placeholder()}",
                    value: "{query}",
                    autofocus: true,
                    oninput: move |e| {
                        query.set(e.value());
                        selected.set(None);
                        selected_zh.set(None);
                    },
                    onkeydown: onkey_search,
                }
                if !query().is_empty() {
                    button {
                        class: "search-clear",
                        onclick: move |_| {
                            query.set(String::new());
                            selected.set(None);
                            selected_zh.set(None);
                        },
                        "✕"
                    }
                }
                span { class: "search-kbd", "Enter ↵" }
                button { class: "btn primary", onclick: open_first, "查询" }
            }

            div { class: if has_detail { "dict-body has-detail" } else { "dict-body" },
                if cur_mode == SearchMode::Zh2En {
                    // --- 汉英 (Chinese -> English) ---
                    div { class: "dict-results",
                        div { class: "dict-results-head",
                            span { "查询结果" }
                            span { class: "muted", "{result_status(query().trim().is_empty(), zh_list.is_empty())}" }
                        }
                        if query().trim().is_empty() {
                            if hist_list.is_empty() {
                                p { class: "muted pad", "{cur_mode.empty_hint()}" }
                            } else {
                                div { class: "hist",
                                    div { class: "hist-title", "最近查询" }
                                    div { class: "wd-links",
                                        for h in hist_list.iter() {
                                            button {
                                                key: "{h.text}",
                                                class: "chip chip-link hist-chip",
                                                onclick: {
                                                    let t = h.text.clone();
                                                    move |_| {
                                                        query.set(t.clone());
                                                        selected.set(None);
                                                        selected_zh.set(None);
                                                    }
                                                },
                                                "{h.text}"
                                            }
                                        }
                                    }
                                }
                            }
                        } else if zh_list.is_empty() {
                            p { class: "muted pad", "未找到匹配的词条" }
                        } else {
                            for e in zh_list.iter() {
                                button {
                                    key: "{e.id}",
                                    class: if selected_zh_id == Some(e.id) { "result active" } else { "result" },
                                    onclick: {
                                        let e = e.clone();
                                        move |_| {
                                            add_history.add(SearchMode::Zh2En, e.simplified.clone());
                                            selected_zh.set(Some(e.clone()))
                                        }
                                    },
                                    span { class: "result-word", "{e.simplified}" }
                                    span { class: "result-pinyin", "[{e.pinyin}]" }
                                    span { class: "result-trans", "{e.glosses.split('/').next().unwrap_or(\"\")}" }
                                }
                            }
                        }
                    }

                    div { class: "dict-detail",
                        button { class: "dict-back", onclick: move |_| { selected.set(None); selected_zh.set(None); }, "\u{2190} 返回列表" }
                        match sel_zh {
                            Some(entry) => rsx! {
                                ZhDetail {
                                    key: "{entry.id}",
                                    entry: entry,
                                    // Cross-link back into the 英汉 detail. When
                                    // ECDICT has no exact hit, fall back to a search.
                                    on_pick: {
                                        let dict_pick = dict.clone();
                                        move |head: String| {
                                            let exact = dict_pick.get_by_word(&head);
                                            add_history.add(
                                                SearchMode::En2Zh,
                                                exact.as_ref().map(|w| w.word.clone()).unwrap_or_else(|| head.clone()),
                                            );
                                            query.set(exact.as_ref().map(|w| w.word.clone()).unwrap_or(head));
                                            selected.set(exact);
                                            selected_zh.set(None);
                                            mode.set(SearchMode::En2Zh);
                                        }
                                    },
                                }
                            },
                            None => rsx! {
                                div { class: "empty-detail",
                                    div { class: "empty-illo",
                                        svg {
                                            width: "38", height: "38", view_box: "0 0 24 24",
                                            fill: "none", stroke: "currentColor", stroke_width: "1.6",
                                            stroke_linecap: "round",
                                            circle { cx: "11", cy: "11", r: "7" }
                                            path { d: "m21 21-4.3-4.3" }
                                        }
                                    }
                                    h3 { "从左侧选择一个词条查看详情" }
                                    p { "词语、拼音，或一个好奇的念头" br {} "从搜索开始探索。" }
                                }
                            }
                        }
                    }
                } else {
                    // --- 英汉 (English -> Chinese) ---
                    div { class: "dict-results",
                        div { class: "dict-results-head",
                            span { "查询结果" }
                            span { class: "muted", "{result_status(query().trim().is_empty(), en_list.is_empty())}" }
                        }
                        if query().trim().is_empty() {
                            if hist_list.is_empty() {
                                p { class: "muted pad", "{cur_mode.empty_hint()}" }
                            } else {
                                div { class: "hist",
                                    div { class: "hist-title", "最近查询" }
                                    div { class: "wd-links",
                                        for h in hist_list.iter() {
                                            button {
                                                key: "{h.text}",
                                                class: "chip chip-link hist-chip",
                                                onclick: {
                                                    let t = h.text.clone();
                                                    move |_| {
                                                        query.set(t.clone());
                                                        selected.set(None);
                                                        selected_zh.set(None);
                                                    }
                                                },
                                                "{h.text}"
                                            }
                                        }
                                    }
                                }
                            }
                        } else if en_list.is_empty() {
                            p { class: "muted pad", "未找到匹配的单词" }
                        } else {
                            for w in en_list.iter() {
                                button {
                                    key: "{w.id}",
                                    class: if selected_id == Some(w.id) { "result active" } else { "result" },
                                    onclick: {
                                        let w = w.clone();
                                        move |_| {
                                            add_history.add(SearchMode::En2Zh, w.word.clone());
                                            selected.set(Some(w.clone()))
                                        }
                                    },
                                    span { class: "result-word", "{w.word}" }
                                    span { class: "result-trans", "{first_line(&w.translation)}" }
                                }
                            }
                        }
                    }

                    div { class: "dict-detail",
                        button { class: "dict-back", onclick: move |_| { selected.set(None); selected_zh.set(None); }, "\u{2190} 返回列表" }
                        match sel {
                            Some(w) => rsx! { WordDetail { key: "{w.id}", word: w } },
                            None => rsx! {
                                div { class: "empty-detail",
                                    div { class: "empty-illo",
                                        svg {
                                            width: "38", height: "38", view_box: "0 0 24 24",
                                            fill: "none", stroke: "currentColor", stroke_width: "1.6",
                                            stroke_linecap: "round", stroke_linejoin: "round",
                                            path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                                            path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
                                        }
                                    }
                                    h3 { "每一个单词，都值得理解" }
                                    p { "从左侧选择一个单词查看详情" }
                                    div { class: "empty-feats",
                                        span { span { class: "ck", "✓" } "中英释义" }
                                        span { span { class: "ck", "✓" } "地道发音" }
                                        span { span { class: "ck", "✓" } "实用例句" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
