use dioxus::prelude::*;

use crate::components::word_detail::WordDetail;
use crate::components::zh_detail::ZhDetail;
use crate::db::dict_repo::DictRepo;
use crate::models::{Word, ZhEntry};
use crate::state::first_line;

/// Lookup direction of the dictionary page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
}

#[component]
pub fn Dictionary() -> Element {
    let dict: DictRepo = use_context();
    let mut query = use_signal(String::new);
    let mut mode = use_signal(SearchMode::default);
    let mut selected = use_signal(|| Option::<Word>::None);
    let mut selected_zh = use_signal(|| Option::<ZhEntry>::None);

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
    let (word_total, zh_total) = counts();
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
                    selected_zh.set(Some(e.clone()));
                }
            }
        }
    };

    rsx! {
        div { class: "page dictionary",
            div { class: "page-head",
                h1 { "{cur_mode.title()}" }
                span { class: "muted", "{count_label}" }
            }

            div { class: "seg",
                button {
                    class: if cur_mode == SearchMode::En2Zh { "seg-btn active" } else { "seg-btn" },
                    onclick: move |_| {
                        mode.set(SearchMode::En2Zh);
                        query.set(String::new());
                        selected.set(None);
                        selected_zh.set(None);
                    },
                    "英汉"
                }
                button {
                    class: if cur_mode == SearchMode::Zh2En { "seg-btn active" } else { "seg-btn" },
                    onclick: move |_| {
                        mode.set(SearchMode::Zh2En);
                        query.set(String::new());
                        selected.set(None);
                        selected_zh.set(None);
                    },
                    "汉英"
                }
            }

            div { class: "search-bar",
                input {
                    r#type: "text",
                    class: "search-input",
                    placeholder: "{cur_mode.placeholder()}",
                    value: "{query}",
                    autofocus: true,
                    oninput: move |e| query.set(e.value()),
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
            }

            div { class: "dict-body",
                if cur_mode == SearchMode::Zh2En {
                    // --- 汉英 (Chinese -> English) ---
                    div { class: "dict-results",
                        if query().trim().is_empty() {
                            p { class: "muted pad", "{cur_mode.empty_hint()}" }
                        } else if zh_list.is_empty() {
                            p { class: "muted pad", "未找到匹配的词条" }
                        } else {
                            for e in zh_list.iter() {
                                button {
                                    key: "{e.id}",
                                    class: if selected_zh_id == Some(e.id) { "result active" } else { "result" },
                                    onclick: {
                                        let e = e.clone();
                                        move |_| selected_zh.set(Some(e.clone()))
                                    },
                                    span { class: "result-word", "{e.simplified}" }
                                    span { class: "result-pinyin", "[{e.pinyin}]" }
                                    span { class: "result-trans", "{e.glosses.split('/').next().unwrap_or(\"\")}" }
                                }
                            }
                        }
                    }

                    div { class: "dict-detail",
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
                                    p { class: "muted", "从左侧选择一个词条查看详情" }
                                }
                            }
                        }
                    }
                } else {
                    // --- 英汉 (English -> Chinese) ---
                    div { class: "dict-results",
                        if query().trim().is_empty() {
                            p { class: "muted pad", "{cur_mode.empty_hint()}" }
                        } else if en_list.is_empty() {
                            p { class: "muted pad", "未找到匹配的单词" }
                        } else {
                            for w in en_list.iter() {
                                button {
                                    key: "{w.id}",
                                    class: if selected_id == Some(w.id) { "result active" } else { "result" },
                                    onclick: {
                                        let w = w.clone();
                                        move |_| selected.set(Some(w.clone()))
                                    },
                                    span { class: "result-word", "{w.word}" }
                                    span { class: "result-trans", "{first_line(&w.translation)}" }
                                }
                            }
                        }
                    }

                    div { class: "dict-detail",
                        match sel {
                            Some(w) => rsx! { WordDetail { key: "{w.id}", word: w } },
                            None => rsx! {
                                div { class: "empty-detail",
                                    p { class: "muted", "从左侧选择一个单词查看详情" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
